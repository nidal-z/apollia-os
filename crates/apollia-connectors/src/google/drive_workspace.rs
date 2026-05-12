//! Google Drive — Agent Workspace pattern.
//!
//! With the non-restricted `drive.file` scope, an app sees only the files it
//! has created OR that the user has explicitly opened with it. Apollia
//! exploits this to back a scoped workspace at `Drive/Apollia/<agent-slug>/`
//! that each agent uses to read, write, and share files without ever needing
//! restricted scopes (cf. ADR-088 §9bis).
//!
//! ## Layout
//!
//! On first connection, Apollia creates a root folder `Apollia` at the user's
//! Drive root. For each agent, a subfolder `Apollia/<agent-slug>/` is created
//! lazily on the first write. The runtime caches the root folder id locally
//! (`~/.apollia/state.json`) but [`DriveWorkspaceClient`] re-resolves it
//! through `files.list` queries — defensive in case the user moves or
//! recreates the folder out-of-band.
//!
//! ## Operations
//!
//! - `workspace_list(agent_slug)` — list files in the agent folder.
//! - `workspace_read(file_id)` — download a text file.
//! - `workspace_write(agent_slug, name, content)` — create / replace a file.
//! - `workspace_delete(file_id)` — trash a file.
//! - `workspace_share(file_id, email)` — share with an email (HITL approval).

// The `|| refresh()` closures wrap a `FnMut` reference into the `FnOnce` shape
// expected by the HTTP helper. Clippy's redundant_closure lint cannot tell that
// `refresh: &mut F` is not itself `FnOnce`, so silence it locally.
#![allow(clippy::redundant_closure)]

use reqwest::Method;
use serde::{Deserialize, Serialize};

use crate::{error::ConnectorError, http::HttpClient};

const DRIVE_BASE: &str = "https://www.googleapis.com/drive/v3";
const UPLOAD_BASE: &str = "https://www.googleapis.com/upload/drive/v3";
const ROOT_FOLDER_NAME: &str = "Apollia";
const FOLDER_MIME: &str = "application/vnd.google-apps.folder";

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

// ─── Client ──────────────────────────────────────────────────────────────────

/// Drive Workspace client.
///
/// All operations are scoped to `Drive/Apollia/<agent_slug>/`. The folder is
/// resolved (or created) lazily on the first write.
#[derive(Clone)]
pub struct DriveWorkspaceClient {
    http: HttpClient,
}

impl DriveWorkspaceClient {
    /// Build a new workspace client.
    pub fn new(http: HttpClient) -> Self {
        Self { http }
    }

    /// Resolve (or create) the agent's workspace folder id.
    ///
    /// First locates the root `Apollia` folder, then the `<agent_slug>`
    /// subfolder, creating each as needed.
    pub async fn ensure_agent_folder<F, Fut>(
        &self,
        agent_slug: &str,
        bearer: &str,
        mut refresh: F,
    ) -> Result<String, ConnectorError>
    where
        F: FnMut() -> Fut + Send,
        Fut: std::future::Future<Output = Result<String, ConnectorError>> + Send,
    {
        let root = self
            .find_or_create_folder(ROOT_FOLDER_NAME, None, bearer, &mut refresh)
            .await?;
        let agent = self
            .find_or_create_folder(agent_slug, Some(&root), bearer, &mut refresh)
            .await?;
        Ok(agent)
    }

    /// List files in the agent's workspace folder.
    pub async fn workspace_list<F, Fut>(
        &self,
        agent_slug: &str,
        bearer: &str,
        mut refresh: F,
    ) -> Result<Vec<DriveFile>, ConnectorError>
    where
        F: FnMut() -> Fut + Send,
        Fut: std::future::Future<Output = Result<String, ConnectorError>> + Send,
    {
        let folder = self.ensure_agent_folder(agent_slug, bearer, &mut refresh).await?;
        let q = format!("'{folder}' in parents and trashed = false");
        let url = format!(
            "{DRIVE_BASE}/files?q={}&fields=files(id,name,mimeType,modifiedTime,size)",
            urlencode(&q)
        );
        let resp: FileList = self.http.get_json(&url, bearer, || refresh()).await?;
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
            .send_with_retries(Method::GET, &url, None, bearer, refresh)
            .await?;
        let bytes = response
            .bytes()
            .await
            .map_err(|e| ConnectorError::Network(e.to_string()))?;
        Ok(bytes.to_vec())
    }

    /// Create a new text file in the agent's workspace.
    ///
    /// Uses Drive's multipart upload (metadata + content in one request).
    pub async fn workspace_write<F, Fut>(
        &self,
        agent_slug: &str,
        name: &str,
        content: &[u8],
        bearer: &str,
        mut refresh: F,
    ) -> Result<DriveFile, ConnectorError>
    where
        F: FnMut() -> Fut + Send,
        Fut: std::future::Future<Output = Result<String, ConnectorError>> + Send,
    {
        let folder = self.ensure_agent_folder(agent_slug, bearer, &mut refresh).await?;

        // Step 1: create the file metadata (resumable session would be needed
        // for >5MB payloads; v0.1.0 uses simple two-step uploads).
        let metadata = FileCreatePayload {
            name,
            parents: vec![folder],
            mime_type: Some("text/plain"),
        };
        let metadata_url = format!("{DRIVE_BASE}/files");
        let file: DriveFile = self
            .http
            .json_request(Method::POST, &metadata_url, &metadata, bearer, || refresh())
            .await?;

        // Step 2: upload the content with PATCH on /upload endpoint.
        let upload_url = format!(
            "{UPLOAD_BASE}/files/{file_id}?uploadType=media",
            file_id = file.id
        );
        let response = self
            .http
            .send_with_retries(
                Method::PATCH,
                &upload_url,
                Some(content.to_vec()),
                bearer,
                || refresh(),
            )
            .await?;
        // Parse the response to refresh file metadata (size etc).
        let bytes = response
            .bytes()
            .await
            .map_err(|e| ConnectorError::Network(e.to_string()))?;
        let updated: DriveFile = serde_json::from_slice(&bytes)
            .map_err(|e| ConnectorError::Decoding(e.to_string()))?;
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
            .json_request(Method::PATCH, &url, &body, bearer, refresh)
            .await?;
        Ok(())
    }

    /// Share a file with a specific email address (reader role).
    ///
    /// **HITL-approved operation** — the caller must surface this to the user
    /// before invocation (see ADR-082).
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
            .json_request(Method::POST, &url, &body, bearer, refresh)
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
            .json_request(Method::POST, &create_url, &payload, bearer, || refresh())
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
