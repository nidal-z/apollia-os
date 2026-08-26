//! OneDrive client (Microsoft Graph `/me/drive`).
//!
//! Surface in v0.1.0: search across the user's drive, get item metadata,
//! download item content, list recent items. Free-tier (Files.Read.All).

use reqwest::Method;
use serde::{Deserialize, Serialize};

use crate::{
    error::ConnectorError,
    http::{HttpClient, RawRequest},
};

const GRAPH: &str = "https://graph.microsoft.com/v1.0/me/drive";

/// Cap on a downloaded or echoed-back file body. The workspace pattern is meant
/// for agent documents, not for archives, and an unbounded `bytes()` on a
/// remote answer is what this crate used to do on every one of these calls.
const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;

// ─── Domain types ────────────────────────────────────────────────────────────

/// OneDrive item (file or folder).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveItem {
    /// Item id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Size in bytes (when known).
    #[serde(default)]
    pub size: Option<u64>,
    /// MIME type for files, absent for folders.
    #[serde(default)]
    pub file: Option<FileFacet>,
    /// Folder facet, present for folders.
    #[serde(default)]
    pub folder: Option<FolderFacet>,
    /// Web URL (browser-accessible link).
    #[serde(default)]
    pub web_url: Option<String>,
    /// Last modified time (RFC 3339).
    #[serde(default)]
    pub last_modified_date_time: Option<String>,
}

/// File facet (Graph's discriminator for items that are files).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileFacet {
    /// MIME type.
    pub mime_type: String,
}

/// Folder facet (Graph's discriminator for items that are folders).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderFacet {
    /// Number of immediate children.
    #[serde(default)]
    pub child_count: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ItemList {
    value: Vec<DriveItem>,
}

// ─── Client ──────────────────────────────────────────────────────────────────

/// OneDrive client.
#[derive(Clone)]
pub struct OneDriveClient {
    http: HttpClient,
}

impl OneDriveClient {
    /// Build a new client.
    pub fn new(http: HttpClient) -> Self {
        Self { http }
    }

    /// Search the drive for items matching `query`.
    pub async fn search<F, Fut>(
        &self,
        query: &str,
        top: u32,
        bearer: &str,
        refresh: F,
    ) -> Result<Vec<DriveItem>, ConnectorError>
    where
        F: FnOnce() -> Fut + Send,
        Fut: std::future::Future<Output = Result<String, ConnectorError>> + Send,
    {
        let url = format!(
            "{GRAPH}/root/search(q='{}')?$top={top}",
            urlencode_singlequoted(query)
        );
        let resp: ItemList = self.http.get_json(&url, bearer, refresh).await?;
        Ok(resp.value)
    }

    /// Get item metadata by id.
    pub async fn get_metadata<F, Fut>(
        &self,
        item_id: &str,
        bearer: &str,
        refresh: F,
    ) -> Result<DriveItem, ConnectorError>
    where
        F: FnOnce() -> Fut + Send,
        Fut: std::future::Future<Output = Result<String, ConnectorError>> + Send,
    {
        let url = format!("{GRAPH}/items/{item_id}");
        self.http.get_json(&url, bearer, refresh).await
    }

    /// Download an item as raw bytes.
    pub async fn download<F, Fut>(
        &self,
        item_id: &str,
        bearer: &str,
        refresh: F,
    ) -> Result<Vec<u8>, ConnectorError>
    where
        F: FnOnce() -> Fut + Send,
        Fut: std::future::Future<Output = Result<String, ConnectorError>> + Send,
    {
        let url = format!("{GRAPH}/items/{item_id}/content");
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

    /// List recently accessed items.
    pub async fn list_recent<F, Fut>(
        &self,
        top: u32,
        bearer: &str,
        refresh: F,
    ) -> Result<Vec<DriveItem>, ConnectorError>
    where
        F: FnOnce() -> Fut + Send,
        Fut: std::future::Future<Output = Result<String, ConnectorError>> + Send,
    {
        let url = format!("{GRAPH}/recent?$top={top}");
        let resp: ItemList = self.http.get_json(&url, bearer, refresh).await?;
        Ok(resp.value)
    }
}

// ─── URL helpers ─────────────────────────────────────────────────────────────

/// Escape a string for inclusion inside Graph's `search(q='…')` single-quoted
/// literal, then percent-encode for URL transport.
fn urlencode_singlequoted(s: &str) -> String {
    // OData escapes single quotes by doubling them.
    let doubled = s.replace('\'', "''");
    urlencode(&doubled)
}

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

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_urlencode_singlequoted_doubles_quotes() {
        // GIVEN a value carrying a single quote, bound for a quoted OData literal
        // WHEN it is encoded for that position
        // THEN the quote is doubled and then percent encoded, so it cannot close the literal
        assert_eq!(urlencode_singlequoted("a'b"), "a%27%27b");
    }

    #[test]
    fn test_urlencode_preserves_alphanumeric() {
        // GIVEN a value made only of letters and digits
        // WHEN it is url-encoded
        // THEN it comes back unchanged
        assert_eq!(urlencode("abc123"), "abc123");
    }

    #[test]
    fn test_drive_item_deserializes_minimal_payload() {
        // GIVEN a Graph payload describing a file
        let payload = serde_json::json!({
            "id": "abc",
            "name": "doc.txt",
            "size": 42,
            "file": { "mimeType": "text/plain" }
        });
        // WHEN it is decoded into a drive item
        let item: DriveItem = serde_json::from_value(payload).expect("decode");
        // THEN the name, the size and the mime type are read, and no folder facet appears
        assert_eq!(item.name, "doc.txt");
        assert_eq!(item.size, Some(42));
        assert_eq!(item.file.unwrap().mime_type, "text/plain");
        assert!(item.folder.is_none());
    }

    #[test]
    fn test_drive_item_deserializes_folder() {
        // GIVEN a Graph payload describing a folder
        let payload = serde_json::json!({
            "id": "fold",
            "name": "Documents",
            "folder": { "childCount": 3 }
        });
        // WHEN it is decoded into a drive item
        let item: DriveItem = serde_json::from_value(payload).expect("decode");
        // THEN the folder facet carries the child count, and no file facet appears
        assert!(item.file.is_none());
        assert_eq!(item.folder.unwrap().child_count, 3);
    }
}
