//! YouTube Data API v3 client, non-sensitive scope (`youtube.readonly`).
//!
//! Search videos, fetch video metadata, list a channel's uploads. Useful
//! for agents that do video-based information retrieval.

use serde::{Deserialize, Serialize};

use crate::{error::ConnectorError, http::HttpClient};

const BASE: &str = "https://www.googleapis.com/youtube/v3";

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoSnippet {
    pub title: String,
    pub description: String,
    pub channel_id: String,
    pub channel_title: String,
    pub published_at: String,
    #[serde(default)]
    pub thumbnails: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoSearchResult {
    pub id: VideoSearchResultId,
    pub snippet: VideoSnippet,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoSearchResultId {
    pub kind: String,
    #[serde(default)]
    pub video_id: Option<String>,
    #[serde(default)]
    pub channel_id: Option<String>,
    #[serde(default)]
    pub playlist_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct SearchResponse {
    #[serde(default)]
    items: Vec<VideoSearchResult>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoDetails {
    pub id: String,
    pub snippet: VideoSnippet,
    #[serde(default)]
    pub statistics: Option<serde_json::Value>,
    #[serde(default)]
    pub content_details: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
struct VideosResponse {
    #[serde(default)]
    items: Vec<VideoDetails>,
}

#[derive(Clone)]
pub struct YouTubeClient {
    http: HttpClient,
}

impl YouTubeClient {
    pub fn new(http: HttpClient) -> Self {
        Self { http }
    }

    /// Free-text search across YouTube videos.
    pub async fn search_videos<F, Fut>(
        &self,
        query: &str,
        max_results: u32,
        bearer: &str,
        refresh: F,
    ) -> Result<Vec<VideoSearchResult>, ConnectorError>
    where
        F: FnOnce() -> Fut + Send,
        Fut: std::future::Future<Output = Result<String, ConnectorError>> + Send,
    {
        let n = max_results.clamp(1, 50);
        let url = format!(
            "{BASE}/search?part=snippet&type=video&maxResults={n}&q={}",
            urlencode(query)
        );
        let resp: SearchResponse = self.http.get_json(&url, bearer, refresh).await?;
        Ok(resp.items)
    }

    /// Get full metadata (snippet + statistics + contentDetails) for a video id.
    pub async fn video_details<F, Fut>(
        &self,
        video_id: &str,
        bearer: &str,
        refresh: F,
    ) -> Result<Option<VideoDetails>, ConnectorError>
    where
        F: FnOnce() -> Fut + Send,
        Fut: std::future::Future<Output = Result<String, ConnectorError>> + Send,
    {
        let url = format!(
            "{BASE}/videos?part=snippet,statistics,contentDetails&id={}",
            urlencode(video_id)
        );
        let resp: VideosResponse = self.http.get_json(&url, bearer, refresh).await?;
        Ok(resp.items.into_iter().next())
    }
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
