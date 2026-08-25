//! Google Drive: Agent Workspace pattern.
//!
//! With the non-restricted `drive.file` scope, an app sees only the files it
//! has created OR that the user has explicitly opened with it. Apollia
//! exploits this to back a scoped workspace at `Drive/Apollia/<agent-slug>/`
//! that each agent uses to read, write, and share files without ever needing
//! restricted scopes.
//!
//! ## Layout
//!
//! On first connection, Apollia creates a root folder `Apollia` at the user's
//! Drive root. For each agent, a subfolder `Apollia/<agent-slug>/` is created
//! lazily on the first write. The runtime caches the root folder id locally
//! (`~/.apollia/state.json`) but [`DriveWorkspaceClient`] re-resolves it
//! through `files.list` queries, defensive in case the user moves or
//! recreates the folder out-of-band.
//!
//! ## Operations
//!
//! - `workspace_list(agent_slug)`: list files in the agent folder.
//! - `workspace_read(file_id)`: download a text file.
//! - `workspace_write(agent_slug, name, content)`: create / replace a file.
//! - `workspace_delete(file_id)`: trash a file.
//! - `workspace_share(file_id, email)`: share with an email (HITL approval).

// The `|| refresh()` closures wrap a `FnMut` reference into the `FnOnce` shape
// expected by the HTTP helper. Clippy's redundant_closure lint cannot tell that
// `refresh: &mut F` is not itself `FnOnce`, so silence it locally.
#![allow(clippy::redundant_closure)]

use reqwest::Method;
use serde::{Deserialize, Serialize};

use crate::{
    error::ConnectorError,
    http::{HttpClient, JsonRequest, RawRequest},
};

const DRIVE_BASE: &str = "https://www.googleapis.com/drive/v3";
const UPLOAD_BASE: &str = "https://www.googleapis.com/upload/drive/v3";
/// Fallback root folder name used when callers don't pass an explicit
/// `root_path` (legacy + default-on-empty behaviour). End-user installs
/// resolve their preferred path via [`apollia_auth::drive_prefs`] and
/// pass it down explicitly; this constant is the safety net.
pub const DEFAULT_ROOT_FOLDER_NAME: &str = "Apollia";
const FOLDER_MIME: &str = "application/vnd.google-apps.folder";

/// Cap on a downloaded or echoed-back file body. The workspace pattern is meant
/// for agent documents, not for archives, and an unbounded `bytes()` on a
/// remote answer is what this crate used to do on every one of these calls.
const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;

// ─── Domain types ────────────────────────────────────────────────────────────

/// Metadata for a file in the agent's workspace.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveFile {
    /// Drive-assigned file id.
    pub id: String,
    /// File name (last segment of the path).
    pub name: String,
    /// MIME type (e.g. `text/plain`, `application/json`).
    #[serde(default)]
    pub mime_type: Option<String>,
    /// Modified time (RFC 3339).
    #[serde(default)]
    pub modified_time: Option<String>,
    /// Size in bytes when available.
    #[serde(default)]
    pub size: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileList {
    #[serde(default)]
    files: Vec<DriveFile>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FileCreatePayload<'a> {
    name: &'a str,
    /// Empty Vec means "create at My Drive root". Drive's docs say omit the
    /// `parents` field for that case, so we skip serialisation when empty.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    parents: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mime_type: Option<&'a str>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PermissionResponse {
    id: String,
}

#[derive(Debug, Serialize)]
struct PermissionRequest<'a> {
    role: &'a str,
    #[serde(rename = "type")]
    permission_type: &'a str,
    #[serde(rename = "emailAddress")]
    email_address: &'a str,
}

/// Search parameters for [`DriveWorkspaceClient::find_by_name`].
pub struct NameSearch<'a> {
    /// Name (or substring when `exact` is false) to match.
    pub name: &'a str,
    /// Optional MIME-type filter. Accepts Google shortcuts (`spreadsheet`,
    /// `document`, `presentation`, `folder`) or a raw mime type string.
    pub mime_type_filter: Option<&'a str>,
    /// When true, query `name = 'X'`; otherwise `name contains 'X'`.
    pub exact: bool,
}

/// Content to write into the agent's workspace, for
/// [`DriveWorkspaceClient::workspace_write`].
pub struct WorkspaceWrite<'a> {
    /// User-configured root path (slash-separated, e.g. `"Documents/Apollia"`).
    pub root_path: &'a str,
    /// Agent slug subfolder under the root.
    pub agent_slug: &'a str,
    /// File name.
    pub name: &'a str,
    /// File content bytes.
    pub content: &'a [u8],
    /// MIME type; `None` defaults to `text/plain`.
    pub mime_type: Option<&'a str>,
}

/// Content to write into an arbitrary folder, for
/// [`DriveWorkspaceClient::write_file_in_folder`].
pub struct FolderWrite<'a> {
    /// Target folder id; `None` writes at My Drive root (no parents).
    pub folder_id: Option<&'a str>,
    /// File name.
    pub name: &'a str,
    /// File content bytes.
    pub content: &'a [u8],
    /// MIME type; `None` defaults to `text/plain`.
    pub mime_type: Option<&'a str>,
}

// ─── Client ──────────────────────────────────────────────────────────────────

/// Drive Workspace client.
///
/// All operations are scoped to `<root_path>/<agent_slug>/` inside the
/// user's Drive. `root_path` is user-configurable (per Google account,
/// stored in `apollia_auth::drive_prefs`); it defaults to `Apollia`. The
/// folder hierarchy is resolved lazily; segments missing on the user's
/// Drive are created on first use (free-tier `drive.file` scope allows
/// this because Apollia owns each folder it touches).
#[derive(Clone)]
pub struct DriveWorkspaceClient {
    http: HttpClient,
}

impl DriveWorkspaceClient {
    /// Build a new workspace client.
    pub fn new(http: HttpClient) -> Self {
        Self { http }
    }

    /// Resolve (or create) the agent's workspace folder id given the
    /// user-configured `root_path` (slash-separated, e.g.
    /// `"Documents/Apollia"`) and `agent_slug`.
    ///
    /// **Semantics**:
    /// - `root_path` non-empty (e.g. `"Documents/Apollia"`) → walk each
    ///   segment, creating missing folders, then nest `agent_slug`.
    /// - `root_path` empty string → user explicitly chose "My Drive root":
    ///   place `agent_slug` directly under root, no intermediate folder.
    pub async fn ensure_agent_folder<F, Fut>(
        &self,
        root_path: &str,
        agent_slug: &str,
        bearer: &str,
        mut refresh: F,
    ) -> Result<String, ConnectorError>
    where
        F: FnMut() -> Fut + Send,
        Fut: std::future::Future<Output = Result<String, ConnectorError>> + Send,
    {
        let parent_id = self
            .ensure_path_folder_id(root_path, bearer, &mut refresh)
            .await?;
        let agent = self
            .find_or_create_folder(agent_slug, parent_id.as_deref(), bearer, &mut refresh)
            .await?;
        Ok(agent)
    }

    /// Walk `root_path` segment by segment, creating each missing folder.
    /// Returns the leaf folder id; `None` when `root_path` is empty (= the
    /// user wants files directly under My Drive root, so callers pass
    /// `None` as the parent in subsequent operations).
    ///
    /// Use this when you intend to **write** something inside the root.
    /// For listings, prefer [`Self::find_path_folder_id`] which never
    /// creates folders.
    pub async fn ensure_path_folder_id<F, Fut>(
        &self,
        root_path: &str,
        bearer: &str,
        refresh: &mut F,
    ) -> Result<Option<String>, ConnectorError>
    where
        F: FnMut() -> Fut + Send,
        Fut: std::future::Future<Output = Result<String, ConnectorError>> + Send,
    {
        let segments: Vec<&str> = root_path
            .split('/')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        let mut parent_id: Option<String> = None;
        for segment in segments {
            let next = self
                .find_or_create_folder(segment, parent_id.as_deref(), bearer, refresh)
                .await?;
            parent_id = Some(next);
        }
        Ok(parent_id)
    }

    /// Read-only variant: walks `root_path` and returns the leaf folder id
    /// **only if every segment exists**. Returns `None` when any segment
    /// is missing OR when `root_path` is empty. Never creates a folder.
    ///
    /// Used by listing operations so we don't silently materialise an
    /// empty `Apollia/` folder on a Drive that has none yet.
    pub async fn find_path_folder_id<F, Fut>(
        &self,
        root_path: &str,
        bearer: &str,
        mut refresh: F,
    ) -> Result<Option<String>, ConnectorError>
    where
        F: FnMut() -> Fut + Send,
        Fut: std::future::Future<Output = Result<String, ConnectorError>> + Send,
    {
        let segments: Vec<&str> = root_path
            .split('/')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        if segments.is_empty() {
            return Ok(None);
        }
        let mut parent_id: Option<String> = None;
        for segment in segments {
            let mut q = format!(
                "name = '{}' and mimeType = '{}' and trashed = false",
                escape_query(segment),
                FOLDER_MIME
            );
            match parent_id.as_deref() {
                Some(pid) => q.push_str(&format!(" and '{pid}' in parents")),
                None => q.push_str(" and 'root' in parents"),
            }
            let url = format!(
                "{DRIVE_BASE}/files?q={}&fields=files(id,name)",
                urlencode(&q)
            );
            let resp: FileList = self.http.get_json(&url, bearer, || refresh()).await?;
            match resp.files.into_iter().next() {
                Some(found) => parent_id = Some(found.id),
                None => return Ok(None),
            }
        }
        Ok(parent_id)
    }

    /// List files in the agent's workspace folder under `root_path`.
    pub async fn workspace_list<F, Fut>(
        &self,
        root_path: &str,
        agent_slug: &str,
        bearer: &str,
        mut refresh: F,
    ) -> Result<Vec<DriveFile>, ConnectorError>
    where
        F: FnMut() -> Fut + Send,
        Fut: std::future::Future<Output = Result<String, ConnectorError>> + Send,
    {
        let folder = self
            .ensure_agent_folder(root_path, agent_slug, bearer, &mut refresh)
            .await?;
        let q = format!("'{folder}' in parents and trashed = false");
        let url = format!(
            "{DRIVE_BASE}/files?q={}&fields=files(id,name,mimeType,modifiedTime,size)",
            urlencode(&q)
        );
        let resp: FileList = self.http.get_json(&url, bearer, || refresh()).await?;
        Ok(resp.files)
    }

    /// List every file Apollia has visibility on with the `drive.file`
    /// scope: files Apollia created itself + files inside folders the
    /// user granted via the Drive Picker. Optional `folder_id` filters
    /// to one parent; otherwise the query is unscoped and returns
    /// top-level visibility (Drive applies the `drive.file` ACL
    /// implicitly; Apollia never sees files it didn't create or get
    /// granted to).
    ///
    /// `page_size` is capped at 100 to keep responses small; pagination
    /// is left for a future iteration when Apollia has agents that need
    /// to walk large file inventories.
    pub async fn list_visible_files<F, Fut>(
        &self,
        folder_id: Option<&str>,
        page_size: u32,
        bearer: &str,
        refresh: F,
    ) -> Result<Vec<DriveFile>, ConnectorError>
    where
        F: FnOnce() -> Fut + Send,
        Fut: std::future::Future<Output = Result<String, ConnectorError>> + Send,
    {
        let page_size = page_size.clamp(1, 100);
        let mut url = format!(
            "{DRIVE_BASE}/files?pageSize={page_size}&fields=files(id,name,mimeType,modifiedTime,size)&q=trashed%20%3D%20false"
        );
        if let Some(id) = folder_id {
            let q = format!("trashed = false and '{id}' in parents");
            url = format!(
                "{DRIVE_BASE}/files?pageSize={page_size}&fields=files(id,name,mimeType,modifiedTime,size)&q={}",
                urlencode(&q)
            );
        }
        let resp: FileList = self.http.get_json(&url, bearer, refresh).await?;
        Ok(resp.files)
    }

    /// Search Drive for files whose name matches `name`. When `exact` is
    /// true, queries `name = 'X'`; otherwise `name contains 'X'` (Drive's
    /// substring match is case-insensitive). Optional `mime_type_filter`
    /// narrows by file type. Pass a Google shortcut (`spreadsheet`,
    /// `document`, `presentation`, `folder`) or a raw mime type string.
    ///
    /// Returns at most 50 matches (sorted by Drive's relevance default).
    /// Use this as the answer to "find <title>" requests; `drive.file`
    /// scope guarantees Apollia only ever sees its own files + Picker
    /// grants, so no privacy footgun.
    pub async fn find_by_name<F, Fut>(
        &self,
        search: NameSearch<'_>,
        bearer: &str,
        refresh: F,
    ) -> Result<Vec<DriveFile>, ConnectorError>
    where
        F: FnOnce() -> Fut + Send,
        Fut: std::future::Future<Output = Result<String, ConnectorError>> + Send,
    {
        // Drive's `q` parameter requires escaping single quotes in name
        // values: we double them per the Drive search syntax.
        let escaped = search.name.replace('\'', "\\'");
        let name_clause = if search.exact {
            format!("name = '{escaped}'")
        } else {
            format!("name contains '{escaped}'")
        };
        let mut clauses: Vec<String> = vec![name_clause, "trashed = false".to_string()];
        if let Some(raw) = search.mime_type_filter {
            let mime = match raw {
                "spreadsheet" => "application/vnd.google-apps.spreadsheet",
                "document" => "application/vnd.google-apps.document",
                "presentation" => "application/vnd.google-apps.presentation",
                "folder" => "application/vnd.google-apps.folder",
                other => other,
            };
            clauses.push(format!("mimeType = '{mime}'"));
        }
        let q = clauses.join(" and ");
        let url = format!(
            "{DRIVE_BASE}/files?pageSize=50&fields=files(id,name,mimeType,modifiedTime,size)&q={}",
            urlencode(&q)
        );
        let resp: FileList = self.http.get_json(&url, bearer, refresh).await?;
        Ok(resp.files)
    }

    /// Read a workspace file as raw bytes.
    ///
    /// Caller responsibility: ensure the file id belongs to the agent's
    /// workspace (typically by listing first). The connector trusts the id.
    pub async fn workspace_read<F, Fut>(
        &self,
        file_id: &str,
        bearer: &str,
        refresh: F,
    ) -> Result<Vec<u8>, ConnectorError>
    where
        F: FnOnce() -> Fut + Send,
        Fut: std::future::Future<Output = Result<String, ConnectorError>> + Send,
    {
        let url = format!("{DRIVE_BASE}/files/{file_id}?alt=media");
        let response = self
            .http
            .send(
                RawRequest {
                    method: Method::GET,
                    url: &url,
                    body: None,
                    content_type: None,
                },
                bearer,
                refresh,
            )
            .await?;
        let bytes = apollia_core::net::read_capped_bytes(response, MAX_FILE_BYTES)
            .await
            .map_err(|e| ConnectorError::Network(e.to_string()))?;
        Ok(bytes.to_vec())
    }

    /// Create a new text file in the agent's workspace under `root_path`.
    ///
    /// Uses Drive's two-step upload (create metadata, then PATCH `uploadType=media`).
    /// `mime_type` controls both the metadata field AND the upload's
    /// Content-Type header; both must agree, otherwise Drive auto-detects from
    /// the content bytes (producing surprises like "application/json" for short
    /// strings). Pass `None` to default to `text/plain`.
    pub async fn workspace_write<F, Fut>(
        &self,
        write: WorkspaceWrite<'_>,
        bearer: &str,
        mut refresh: F,
    ) -> Result<DriveFile, ConnectorError>
    where
        F: FnMut() -> Fut + Send,
        Fut: std::future::Future<Output = Result<String, ConnectorError>> + Send,
    {
        let folder = self
            .ensure_agent_folder(write.root_path, write.agent_slug, bearer, &mut refresh)
            .await?;

        let mime = write.mime_type.unwrap_or("text/plain");
        // Step 1: create the file metadata (resumable session would be needed
        // for >5MB payloads; v0.1.0 uses simple two-step uploads).
        let metadata = FileCreatePayload {
            name: write.name,
            parents: vec![folder],
            mime_type: Some(mime),
        };
        let metadata_url = format!("{DRIVE_BASE}/files");
        let file: DriveFile = self
            .http
            .json_request(
                JsonRequest {
                    method: Method::POST,
                    url: &metadata_url,
                    body: &metadata,
                },
                bearer,
                || refresh(),
            )
            .await?;

        // Step 2: upload the content with PATCH on /upload endpoint.
        let upload_url = format!(
            "{UPLOAD_BASE}/files/{file_id}?uploadType=media",
            file_id = file.id
        );
        let response = self
            .http
            .send(
                RawRequest {
                    method: Method::PATCH,
                    url: &upload_url,
                    body: Some(write.content.to_vec()),
                    content_type: Some(mime),
                },
                bearer,
                || refresh(),
            )
            .await?;
        // Parse the response to refresh file metadata (size etc).
        let bytes = apollia_core::net::read_capped_bytes(response, MAX_FILE_BYTES)
            .await
            .map_err(|e| ConnectorError::Network(e.to_string()))?;
        let updated: DriveFile =
            serde_json::from_slice(&bytes).map_err(|e| ConnectorError::Decoding(e.to_string()))?;
        Ok(updated)
    }

    /// Trash a file (Drive's soft-delete). The user can restore from trash.
    pub async fn workspace_delete<F, Fut>(
        &self,
        file_id: &str,
        bearer: &str,
        refresh: F,
    ) -> Result<(), ConnectorError>
    where
        F: FnOnce() -> Fut + Send,
        Fut: std::future::Future<Output = Result<String, ConnectorError>> + Send,
    {
        // Send to trash via PATCH instead of permanent delete, matching the
        // user's expectation of "I can recover from the Trash".
        let url = format!("{DRIVE_BASE}/files/{file_id}");
        let body = serde_json::json!({ "trashed": true });
        let _: serde_json::Value = self
            .http
            .json_request(
                JsonRequest {
                    method: Method::PATCH,
                    url: &url,
                    body: &body,
                },
                bearer,
                refresh,
            )
            .await?;
        Ok(())
    }

    // ─── Picker-friendly methods (direct folder_id, no path walking) ─────

    /// List files inside a folder identified directly by its Drive `folder_id`.
    /// Skips the path-walking of [`workspace_list`]; used by agents acting
    /// on folders the user designated via Google Picker.
    pub async fn list_files_in_folder<F, Fut>(
        &self,
        folder_id: &str,
        bearer: &str,
        refresh: F,
    ) -> Result<Vec<DriveFile>, ConnectorError>
    where
        F: FnOnce() -> Fut + Send,
        Fut: std::future::Future<Output = Result<String, ConnectorError>> + Send,
    {
        let q = format!("'{folder_id}' in parents and trashed = false");
        let url = format!(
            "{DRIVE_BASE}/files?q={}&fields=files(id,name,mimeType,modifiedTime,size)",
            urlencode(&q)
        );
        let resp: FileList = self.http.get_json(&url, bearer, refresh).await?;
        Ok(resp.files)
    }

    /// Create or replace a text file inside an arbitrary folder identified
    /// by its Drive `folder_id`. Pass `folder_id = None` to drop the file
    /// at My Drive root (no parents). `mime_type = None` defaults to
    /// `text/plain`; pass `text/markdown`, `application/json`, etc., to
    /// avoid Drive's content-sniffing surprises.
    pub async fn write_file_in_folder<F, Fut>(
        &self,
        write: FolderWrite<'_>,
        bearer: &str,
        mut refresh: F,
    ) -> Result<DriveFile, ConnectorError>
    where
        F: FnMut() -> Fut + Send,
        Fut: std::future::Future<Output = Result<String, ConnectorError>> + Send,
    {
        let mime = write.mime_type.unwrap_or("text/plain");
        let parents = match write.folder_id {
            Some(id) => vec![id.to_string()],
            None => Vec::new(),
        };
        let metadata = FileCreatePayload {
            name: write.name,
            parents,
            mime_type: Some(mime),
        };
        let metadata_url = format!("{DRIVE_BASE}/files");
        let file: DriveFile = self
            .http
            .json_request(
                JsonRequest {
                    method: Method::POST,
                    url: &metadata_url,
                    body: &metadata,
                },
                bearer,
                || refresh(),
            )
            .await?;

        let upload_url = format!(
            "{UPLOAD_BASE}/files/{file_id}?uploadType=media",
            file_id = file.id
        );
        let response = self
            .http
            .send(
                RawRequest {
                    method: Method::PATCH,
                    url: &upload_url,
                    body: Some(write.content.to_vec()),
                    content_type: Some(mime),
                },
                bearer,
                || refresh(),
            )
            .await?;
        let bytes = apollia_core::net::read_capped_bytes(response, MAX_FILE_BYTES)
            .await
            .map_err(|e| ConnectorError::Network(e.to_string()))?;
        let updated: DriveFile =
            serde_json::from_slice(&bytes).map_err(|e| ConnectorError::Decoding(e.to_string()))?;
        Ok(updated)
    }

    /// Share a file with a specific email address (reader role).
    ///
    /// **HITL-approved operation**: the caller must surface this to the user
    /// before invocation.
    pub async fn workspace_share<F, Fut>(
        &self,
        file_id: &str,
        email: &str,
        bearer: &str,
        refresh: F,
    ) -> Result<String, ConnectorError>
    where
        F: FnOnce() -> Fut + Send,
        Fut: std::future::Future<Output = Result<String, ConnectorError>> + Send,
    {
        let url = format!("{DRIVE_BASE}/files/{file_id}/permissions");
        let body = PermissionRequest {
            role: "reader",
            permission_type: "user",
            email_address: email,
        };
        let resp: PermissionResponse = self
            .http
            .json_request(
                JsonRequest {
                    method: Method::POST,
                    url: &url,
                    body: &body,
                },
                bearer,
                refresh,
            )
            .await?;
        Ok(resp.id)
    }

    // ─── Internal ───────────────────────────────────────────────────────────

    async fn find_or_create_folder<F, Fut>(
        &self,
        name: &str,
        parent_id: Option<&str>,
        bearer: &str,
        refresh: &mut F,
    ) -> Result<String, ConnectorError>
    where
        F: FnMut() -> Fut + Send,
        Fut: std::future::Future<Output = Result<String, ConnectorError>> + Send,
    {
        // 1. Search for an existing folder with this name and parent.
        let mut q = format!(
            "name = '{}' and mimeType = '{}' and trashed = false",
            escape_query(name),
            FOLDER_MIME
        );
        if let Some(pid) = parent_id {
            q.push_str(&format!(" and '{pid}' in parents"));
        } else {
            q.push_str(" and 'root' in parents");
        }
        let url = format!(
            "{DRIVE_BASE}/files?q={}&fields=files(id,name)",
            urlencode(&q)
        );
        let resp: FileList = self.http.get_json(&url, bearer, || refresh()).await?;
        if let Some(existing) = resp.files.into_iter().next() {
            return Ok(existing.id);
        }

        // 2. Create it.
        let payload = FileCreatePayload {
            name,
            parents: parent_id.map(str::to_owned).into_iter().collect(),
            mime_type: Some(FOLDER_MIME),
        };
        let create_url = format!("{DRIVE_BASE}/files");
        let created: DriveFile = self
            .http
            .json_request(
                JsonRequest {
                    method: Method::POST,
                    url: &create_url,
                    body: &payload,
                },
                bearer,
                || refresh(),
            )
            .await?;
        Ok(created.id)
    }
}

// ─── URL / query helpers ─────────────────────────────────────────────────────

fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~') {
            out.push(c);
        } else {
            for b in c.to_string().as_bytes() {
                out.push_str(&format!("%{b:02X}"));
            }
        }
    }
    out
}

/// Escape single quotes for embedding into a Drive query.
fn escape_query(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\'', "\\'")
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_query_escapes_single_quotes() {
        assert_eq!(escape_query("foo's"), "foo\\'s");
    }

    #[test]
    fn test_escape_query_escapes_backslashes() {
        assert_eq!(escape_query("a\\b"), "a\\\\b");
    }

    #[test]
    fn test_urlencode_escapes_spaces_and_equals() {
        let q = "name = 'Apollia' and trashed = false";
        let encoded = urlencode(q);
        assert!(encoded.contains("%20"));
        assert!(encoded.contains("%3D"));
        assert!(!encoded.contains(' '));
    }

    #[test]
    fn test_drive_workspace_client_constructs_from_http_client() {
        let http = HttpClient::new("google").expect("http");
        let _client = DriveWorkspaceClient::new(http);
    }

    #[test]
    fn test_file_create_payload_serializes_with_camel_case() {
        let payload = FileCreatePayload {
            name: "doc.txt",
            parents: vec!["abc123".into()],
            mime_type: Some("text/plain"),
        };
        let json = serde_json::to_value(&payload).expect("serialize");
        assert_eq!(json["name"], "doc.txt");
        assert!(json["mimeType"].is_string()); // camelCase
        assert_eq!(json["parents"][0], "abc123");
    }
}
