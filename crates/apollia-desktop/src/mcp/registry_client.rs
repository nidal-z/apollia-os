//! Client for the MCP Registry API.
//!
//! Fetches the catalogue of available MCP servers from the official registry
//! with automatic pagination, keyword search, and local disk cache. Falls
//! back to the cached data when the registry is unreachable.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Client for the MCP Registry API (`registry.modelcontextprotocol.io`).
///
/// The client is `Clone` and `Send + Sync`, suitable for storage in Tauri
/// managed state. The internal `reqwest::Client` connection pool is shared
/// across clones.
#[derive(Clone)]
pub struct McpRegistryClient {
    http: reqwest::Client,
    cache_path: PathBuf,
    base_url: String,
}

/// A server entry from the MCP Registry list response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryServer {
    /// Server detail returned by the registry.
    pub server: RegistryServerDetail,
    /// Opaque registry metadata (reserved for pagination helpers).
    #[serde(rename = "_meta")]
    pub meta: Option<serde_json::Value>,
}

/// Detailed metadata for a single MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryServerDetail {
    /// Unique identifier in the registry (e.g. `@modelcontextprotocol/server-notion`).
    pub name: String,
    /// Human-readable display name.
    pub title: Option<String>,
    /// Short description of the server's capabilities.
    pub description: Option<String>,
    /// Semantic version of this registry entry.
    pub version: String,
    /// Source code repository reference.
    pub repository: Option<RegistryRepository>,
    /// Documentation or product website URL.
    #[serde(rename = "websiteUrl")]
    pub website_url: Option<String>,
    /// Installable packages for this server (npm, pip, …).
    pub packages: Option<Vec<RegistryPackage>>,
    /// Icon assets for display in the catalogue UI.
    pub icons: Option<Vec<RegistryIcon>>,
    /// Remote connection endpoints (streamable-http, SSE).
    ///
    /// Empty when the server is only distributed as an installable package.
    /// Approximately 70% of registry servers expose only remotes with no packages.
    #[serde(default)]
    pub remotes: Vec<RegistryRemote>,
}

/// Repository reference for an MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryRepository {
    /// Repository URL (GitHub, GitLab, …). Absent on some registry entries.
    pub url: Option<String>,
    /// Registry source identifier (e.g. `github`).
    pub source: Option<String>,
}

/// An installable package for an MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryPackage {
    /// Package registry type (e.g. `npm`, `pypi`).
    #[serde(rename = "registryType")]
    pub registry_type: String,
    /// Package identifier within its registry (e.g. `@modelcontextprotocol/server-notion`).
    pub identifier: String,
    /// Package version string (absent on some registry entries).
    pub version: Option<String>,
    /// Suggested runtime (e.g. `node`, `python`).
    #[serde(rename = "runtimeHint")]
    pub runtime_hint: Option<String>,
    /// MCP transport used by this package.
    pub transport: RegistryTransport,
    /// Environment variables required at runtime.
    #[serde(rename = "environmentVariables", default)]
    pub environment_variables: Vec<RegistryEnvVar>,
    /// Positional or named arguments forwarded to the package at launch.
    #[serde(rename = "packageArguments", default)]
    pub package_arguments: Vec<RegistryPackageArg>,
}

/// MCP transport declared by a package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryTransport {
    /// Transport type string (e.g. `stdio`, `sse`).
    #[serde(rename = "type")]
    pub transport_type: String,
}

/// An environment variable required by an MCP server package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryEnvVar {
    /// Environment variable name (e.g. `NOTION_API_TOKEN`).
    pub name: String,
    /// Human-readable description shown in the wizard.
    pub description: Option<String>,
    /// Whether the variable must be provided before the server can start.
    #[serde(rename = "isRequired", default)]
    pub is_required: bool,
    /// Whether the value should be stored in the keychain instead of plain text.
    #[serde(rename = "isSecret", default)]
    pub is_secret: bool,
}

/// A positional or named argument forwarded to an MCP package at launch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryPackageArg {
    /// Argument type (e.g. `positional`, `named`).
    #[serde(rename = "type")]
    pub arg_type: String,
    /// Fixed value (when not provided by the user).
    pub value: Option<String>,
    /// Hint shown in the wizard when the user must supply the value.
    #[serde(rename = "valueHint")]
    pub value_hint: Option<String>,
    /// Human-readable description.
    pub description: Option<String>,
    /// Whether the argument must be provided before the server can start.
    #[serde(rename = "isRequired", default)]
    pub is_required: bool,
    /// Whether the user can supply multiple whitespace-separated values that
    /// expand to one argv entry each (e.g. multiple allowed directories).
    #[serde(rename = "isRepeatable", default)]
    pub is_repeatable: bool,
}

/// An icon asset for an MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryIcon {
    /// URL or data URI for the icon image.
    pub src: String,
    /// MIME type of the icon (e.g. `image/svg+xml`).
    #[serde(rename = "mimeType")]
    pub mime_type: Option<String>,
}

/// A remote connection endpoint for an MCP server.
///
/// Represents a `streamable-http` or `sse` entry in the registry `remotes` array.
/// Remote servers are connected directly over the network without a local package install.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryRemote {
    /// Transport type (e.g. `"streamable-http"`, `"sse"`).
    #[serde(rename = "type")]
    pub transport_type: String,
    /// Connection URL (e.g. `"https://mcp.notion.com/mcp"`).
    pub url: String,
    /// HTTP headers required to authenticate or configure the connection.
    #[serde(default)]
    pub headers: Vec<RegistryRemoteHeader>,
}

/// A required HTTP header for a remote MCP connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryRemoteHeader {
    /// Header name (e.g. `"Authorization"`).
    pub name: String,
    /// Human-readable description shown in the connection wizard.
    pub description: Option<String>,
    /// Whether this header must be provided before connecting.
    #[serde(rename = "isRequired", default)]
    pub is_required: bool,
    /// Whether the value should be stored in the OS keychain instead of plain text.
    #[serde(rename = "isSecret", default)]
    pub is_secret: bool,
}

/// Paginated response from the MCP Registry `/v0.1/servers` endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryListResponse {
    /// Server entries on this page.
    pub servers: Vec<RegistryServer>,
    /// Pagination metadata.
    pub metadata: RegistryMetadata,
}

/// Pagination metadata included in every registry list response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryMetadata {
    /// Total number of servers returned on this page.
    pub count: u32,
    /// Opaque cursor to pass as `?cursor=` to fetch the next page.
    /// `None` or empty string indicates the last page.
    #[serde(rename = "nextCursor")]
    pub next_cursor: Option<String>,
}

/// Errors produced by [`McpRegistryClient`].
#[derive(Debug, thiserror::Error)]
pub enum RegistryClientError {
    /// HTTP transport error or non-2xx response from the registry.
    #[error("HTTP request failed: {0}")]
    HttpError(String),

    /// JSON deserialization failed on the registry response.
    #[error("failed to parse registry response: {0}")]
    ParseError(String),

    /// Local cache read or write failed.
    #[error("cache read/write failed: {0}")]
    CacheError(String),

    /// Registry is unreachable and no local cache is available.
    #[error("registry unreachable and no local cache available")]
    Offline,
}

/// Outcome of fetching one registry page over HTTP.
enum PageFetch {
    /// Raw response body, ready to parse.
    Body(String),
    /// Request, status, or body read failed (description string).
    Fatal(String),
}

/// Outcome of parsing a single registry page body.
enum PageParse {
    /// Page parsed: append `servers`, follow `next` cursor if present.
    Servers {
        servers: Vec<RegistryServer>,
        next: Option<String>,
    },
    /// Page failed to parse but a cursor was recovered; skip to `cursor`
    /// with the page counter advanced to `page`.
    SkipTo { cursor: String, page: usize },
    /// Stop pagination (parse failed with no recoverable cursor).
    Stop,
}

/// Try to extract the `nextCursor` from a JSON body that failed full parsing.
///
/// This allows pagination to continue past pages with invalid characters
/// in server descriptions/names while the metadata is still valid JSON.
fn extract_cursor_raw(body: &str) -> Option<String> {
    // Look for "nextCursor":"<value>" near the end of the body.
    let marker = "\"nextCursor\":\"";
    let start = body.rfind(marker)? + marker.len();
    let end = body[start..].find('"')? + start;
    let cursor = &body[start..end];
    if cursor.is_empty() {
        None
    } else {
        Some(cursor.to_string())
    }
}

impl McpRegistryClient {
    /// Create a new registry client.
    ///
    /// `cache_dir` is the directory where `mcp-registry.json` will be written.
    /// The HTTP client is configured with a 15-second connect + read timeout.
    pub fn new(cache_dir: &Path) -> Result<Self, RegistryClientError> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(|e| RegistryClientError::HttpError(e.to_string()))?;

        Ok(Self {
            http,
            cache_path: cache_dir.join("mcp-registry.json"),
            base_url: "https://registry.modelcontextprotocol.io".to_string(),
        })
    }

    /// Create a registry client that targets `base_url` instead of the official registry.
    ///
    /// The base URL must not include a trailing slash (e.g. `http://localhost:8080`).
    #[cfg(test)]
    fn with_base_url(
        cache_dir: &Path,
        base_url: impl Into<String>,
    ) -> Result<Self, RegistryClientError> {
        let mut client = Self::new(cache_dir)?;
        client.base_url = base_url.into();
        Ok(client)
    }

    /// Return the path of the local cache file.
    pub fn cache_path(&self) -> &Path {
        &self.cache_path
    }

    /// Fetch all MCP servers from the registry, with offline fallback.
    ///
    /// When the registry is reachable the full paginated list is returned and
    /// the local cache is updated. When the registry is unreachable the cached
    /// data is returned with a `warn` log. When neither source is available
    /// [`RegistryClientError::Offline`] is returned.
    ///
    /// `search` is an optional keyword filter forwarded to the registry API.
    /// Cache TTL: skip network fetch if cache is younger than this.
    const CACHE_TTL: Duration = Duration::from_secs(15 * 60); // 15 minutes

    /// Fetch a single server by its exact registry name.
    ///
    /// Uses the registry `search` parameter as a targeted lookup. Returns
    /// `None` when the server is not found or the registry is unreachable.
    /// Does not read or write the local bulk cache.
    pub async fn fetch_server_by_name(
        &self,
        name: &str,
    ) -> Result<Option<RegistryServer>, RegistryClientError> {
        let servers = self.fetch_from_network(Some(name)).await?;
        Ok(servers.into_iter().find(|s| s.server.name == name))
    }

    pub async fn fetch_servers(
        &self,
        search: Option<&str>,
    ) -> Result<Vec<RegistryServer>, RegistryClientError> {
        // Stale-while-revalidate: if cache is fresh enough, return it immediately.
        if search.is_none() {
            if let Some(age) = self.cache_age() {
                if age < Self::CACHE_TTL {
                    if let Ok(cached) = self.read_cache() {
                        tracing::debug!(
                            age_secs = age.as_secs(),
                            servers = cached.len(),
                            "returning fresh cache"
                        );
                        return Ok(Self::dedup_latest(cached));
                    }
                }
            }
        }

        let servers = match self.fetch_from_network(search).await {
            Ok(servers) => {
                if let Err(e) = self.write_cache(&servers) {
                    tracing::warn!(error = %e, "failed to update MCP registry cache");
                }
                servers
            }
            Err(network_err) => {
                tracing::warn!(
                    error = %network_err,
                    "MCP Registry unreachable - falling back to local cache"
                );
                self.read_cache()?
            }
        };
        Ok(Self::dedup_latest(servers))
    }

    /// Returns the age of the cache file, or `None` if it doesn't exist.
    fn cache_age(&self) -> Option<Duration> {
        std::fs::metadata(&self.cache_path)
            .ok()?
            .modified()
            .ok()?
            .elapsed()
            .ok()
    }

    /// Deduplicate server entries, keeping only the latest version per name.
    ///
    /// The registry returns multiple versions of the same server. The `_meta`
    /// object contains an `isLatest` flag; when present the flagged entry wins.
    /// Otherwise the last occurrence (highest index in the paginated response)
    /// is kept.
    fn dedup_latest(servers: Vec<RegistryServer>) -> Vec<RegistryServer> {
        let mut best: HashMap<String, (usize, bool)> = HashMap::new();
        for (idx, entry) in servers.iter().enumerate() {
            let is_latest = entry
                .meta
                .as_ref()
                .and_then(|m| m.pointer("/io.modelcontextprotocol.registry~1official/isLatest"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            match best.get(&entry.server.name) {
                Some(&(_, true)) if !is_latest => {} // existing is already latest
                _ => {
                    best.insert(entry.server.name.clone(), (idx, is_latest));
                }
            }
        }

        let mut keep: Vec<usize> = best.into_values().map(|(idx, _)| idx).collect();
        keep.sort_unstable();

        keep.into_iter()
            .filter_map(|idx| servers.get(idx).cloned())
            .collect()
    }

    /// Safety limit to avoid infinite pagination loops.
    const MAX_PAGES: usize = 200;

    /// Fetch one registry page body, collapsing the HTTP/read error layers.
    ///
    /// Returns the raw response body on success, or [`PageFetch::Fatal`] with a
    /// description string when the request, status, or body read fails. The
    /// caller decides whether a fatal outcome aborts (no servers yet) or simply
    /// stops pagination (some servers already collected).
    async fn fetch_page_body(&self, search: Option<&str>, cursor: Option<&str>) -> PageFetch {
        let mut query: Vec<(&str, String)> = vec![("limit", "100".to_string())];
        if let Some(q) = search {
            query.push(("search", q.to_string()));
        }
        if let Some(c) = cursor {
            query.push(("cursor", c.to_string()));
        }

        let resp = match self
            .http
            .get(format!("{}/v0.1/servers", self.base_url))
            .query(&query)
            .header("Accept", "application/json")
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(e) => return PageFetch::Fatal(e.to_string()),
        };
        let resp = match resp.error_for_status() {
            Ok(resp) => resp,
            Err(e) => return PageFetch::Fatal(e.to_string()),
        };
        match resp.text().await {
            Ok(body) => PageFetch::Body(body),
            Err(e) => PageFetch::Fatal(e.to_string()),
        }
    }

    /// Fetch all pages from the network, following `nextCursor` until exhausted.
    ///
    /// Pages that fail to parse (e.g. invalid control characters in JSON) are
    /// skipped; the servers fetched so far are still returned.
    async fn fetch_from_network(
        &self,
        search: Option<&str>,
    ) -> Result<Vec<RegistryServer>, RegistryClientError> {
        let mut all_servers: Vec<RegistryServer> = Vec::new();
        let mut cursor: Option<String> = None;
        let mut page = 0usize;

        loop {
            let body = match self.fetch_page_body(search, cursor.as_deref()).await {
                PageFetch::Body(body) => body,
                PageFetch::Fatal(e) if all_servers.is_empty() => {
                    return Err(RegistryClientError::HttpError(e));
                }
                PageFetch::Fatal(e) => {
                    tracing::warn!(page, error = %e, "registry page fetch failed - stopping");
                    break;
                }
            };

            match Self::parse_page(&body, page) {
                PageParse::Servers { servers, next } => {
                    all_servers.extend(servers);
                    page += 1;
                    match next {
                        Some(c) if page < Self::MAX_PAGES => cursor = Some(c),
                        _ => break,
                    }
                }
                PageParse::SkipTo {
                    cursor: next,
                    page: next_page,
                } => {
                    cursor = Some(next);
                    page = next_page;
                    if page >= Self::MAX_PAGES {
                        break;
                    }
                }
                PageParse::Stop => break,
            }
        }

        if all_servers.is_empty() && page > 0 {
            return Err(RegistryClientError::ParseError(
                "all registry pages failed to parse".to_string(),
            ));
        }

        tracing::info!(
            servers = all_servers.len(),
            pages = page,
            "MCP Registry fetch complete"
        );

        Ok(all_servers)
    }

    /// Parse one registry page body into a [`PageParse`] outcome.
    ///
    /// On a serde failure, attempts to recover the `nextCursor` from raw JSON so
    /// pagination can skip past pages with invalid characters; if none is found,
    /// pagination stops.
    fn parse_page(body: &str, page: usize) -> PageParse {
        match serde_json::from_str::<RegistryListResponse>(body) {
            Ok(response) => {
                let next = response.metadata.next_cursor.filter(|c| !c.is_empty());
                PageParse::Servers {
                    servers: response.servers,
                    next,
                }
            }
            Err(e) => {
                tracing::warn!(
                    page,
                    serde_error = %e,
                    "registry page JSON parse failed - skipping to next page"
                );
                match extract_cursor_raw(body) {
                    Some(next) => PageParse::SkipTo {
                        cursor: next,
                        page: page + 1,
                    },
                    None => PageParse::Stop,
                }
            }
        }
    }

    /// Serialize `servers` to JSON and write to the cache file.
    fn write_cache(&self, servers: &[RegistryServer]) -> Result<(), RegistryClientError> {
        if let Some(parent) = self.cache_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| RegistryClientError::CacheError(e.to_string()))?;
        }
        let json = serde_json::to_string(servers)
            .map_err(|e| RegistryClientError::CacheError(e.to_string()))?;
        std::fs::write(&self.cache_path, json)
            .map_err(|e| RegistryClientError::CacheError(e.to_string()))
    }

    /// Read and deserialize the local cache file.
    ///
    /// Returns [`RegistryClientError::Offline`] when the file does not exist.
    fn read_cache(&self) -> Result<Vec<RegistryServer>, RegistryClientError> {
        let json =
            std::fs::read_to_string(&self.cache_path).map_err(|_| RegistryClientError::Offline)?;
        serde_json::from_str(&json).map_err(|e| RegistryClientError::ParseError(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use tempfile::TempDir;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    // ── helpers ──────────────────────────────────────────────────────────────

    fn make_server_json(name: &str) -> serde_json::Value {
        serde_json::json!({
            "server": {
                "name": name,
                "title": format!("{} title", name),
                "description": null,
                "version": "1.0.0",
                "repository": null,
                "websiteUrl": null,
                "packages": null,
                "icons": null
            },
            "_meta": null
        })
    }

    fn single_page_response(names: &[&str]) -> serde_json::Value {
        let servers: Vec<_> = names.iter().map(|n| make_server_json(n)).collect();
        serde_json::json!({
            "servers": servers,
            "metadata": { "count": servers.len(), "nextCursor": null }
        })
    }

    fn page_response_with_cursor(names: &[&str], cursor: &str) -> serde_json::Value {
        let servers: Vec<_> = names.iter().map(|n| make_server_json(n)).collect();
        serde_json::json!({
            "servers": servers,
            "metadata": { "count": servers.len(), "nextCursor": cursor }
        })
    }

    // ── fetch all servers (single-page fast-path) ─────────────────────

    #[tokio::test]
    async fn test_fetch_servers_returns_list() {
        // GIVEN a mock server returning two servers on a single page
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v0.1/servers"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(single_page_response(&["server-a", "server-b"])),
            )
            .mount(&mock_server)
            .await;

        let tmp = TempDir::new().unwrap();
        let client = McpRegistryClient::with_base_url(tmp.path(), mock_server.uri()).unwrap();

        // WHEN fetching all servers
        let servers = client.fetch_servers(None).await.unwrap();

        // THEN both entries are present
        assert_eq!(servers.len(), 2);
        assert_eq!(servers[0].server.name, "server-a");
        assert_eq!(servers[1].server.name, "server-b");
    }

    // ── keyword search forwarded to registry ───────────────────────────

    #[tokio::test]
    async fn test_fetch_servers_with_search_filter() {
        // GIVEN a mock server that only responds to requests with search=notion
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v0.1/servers"))
            .and(query_param("search", "notion"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(single_page_response(&["notion-server"])),
            )
            .mount(&mock_server)
            .await;

        let tmp = TempDir::new().unwrap();
        let client = McpRegistryClient::with_base_url(tmp.path(), mock_server.uri()).unwrap();

        // WHEN searching for "notion"
        let servers = client.fetch_servers(Some("notion")).await.unwrap();

        // THEN only the notion server is returned
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].server.name, "notion-server");
    }

    // ── successful fetch updates the cache file ────────────────────────

    #[tokio::test]
    async fn test_write_cache_creates_file() {
        // GIVEN a successful fetch
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v0.1/servers"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(single_page_response(&["cached-server"])),
            )
            .mount(&mock_server)
            .await;

        let tmp = TempDir::new().unwrap();
        let client = McpRegistryClient::with_base_url(tmp.path(), mock_server.uri()).unwrap();

        // WHEN the fetch succeeds
        let _ = client.fetch_servers(None).await.unwrap();

        // THEN the cache file exists and contains valid JSON
        assert!(client.cache_path().exists());
        let raw = std::fs::read_to_string(client.cache_path()).unwrap();
        let parsed: Vec<RegistryServer> = serde_json::from_str(&raw).unwrap();
        assert_eq!(parsed[0].server.name, "cached-server");
    }

    // ── offline fallback returns cached data ───────────────────────────

    #[tokio::test]
    async fn test_offline_returns_cache() {
        // GIVEN a cache file with known data
        let tmp = TempDir::new().unwrap();
        let cached_servers = vec![make_server_json("cached-offline")];
        let json = serde_json::to_string(&cached_servers).unwrap();
        std::fs::write(tmp.path().join("mcp-registry.json"), json).unwrap();

        // AND a registry that is unreachable (nothing bound on this port)
        let client = McpRegistryClient::with_base_url(tmp.path(), "http://127.0.0.1:1").unwrap();

        // WHEN fetching servers
        let servers = client.fetch_servers(None).await.unwrap();

        // THEN cached data is returned
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].server.name, "cached-offline");
    }

    // ── offline with no cache returns Offline error ───────────────────

    #[tokio::test]
    async fn test_offline_no_cache_returns_error() {
        // GIVEN an unreachable registry and no cache file
        let tmp = TempDir::new().unwrap();
        let client = McpRegistryClient::with_base_url(tmp.path(), "http://127.0.0.1:1").unwrap();

        // WHEN fetching servers
        let result = client.fetch_servers(None).await;

        // THEN Offline error is returned
        assert!(matches!(result, Err(RegistryClientError::Offline)));
    }

    // ── pagination follows nextCursor until exhausted ─────────────────

    #[tokio::test]
    async fn test_pagination_follows_next_cursor() {
        // GIVEN a mock server returning page-1 with a cursor, then page-2 without
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v0.1/servers"))
            .and(query_param("limit", "100"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(page_response_with_cursor(&["page1-server"], "cursor-abc")),
            )
            .up_to_n_times(1)
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/v0.1/servers"))
            .and(query_param("cursor", "cursor-abc"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(single_page_response(&["page2-server"])),
            )
            .mount(&mock_server)
            .await;

        let tmp = TempDir::new().unwrap();
        let client = McpRegistryClient::with_base_url(tmp.path(), mock_server.uri()).unwrap();

        // WHEN fetching all servers
        let servers = client.fetch_servers(None).await.unwrap();

        // THEN servers from both pages are concatenated
        assert_eq!(servers.len(), 2);
        assert_eq!(servers[0].server.name, "page1-server");
        assert_eq!(servers[1].server.name, "page2-server");
    }

    // ── Deserialization round-trip ────────────────────────────────────────────

    #[test]
    fn test_registry_list_response_deserializes() {
        // GIVEN a representative registry JSON payload
        let json = serde_json::json!({
            "servers": [{
                "server": {
                    "name": "@modelcontextprotocol/server-notion",
                    "title": "Notion",
                    "description": "Read and write Notion pages",
                    "version": "0.6.2",
                    "repository": { "url": "https://github.com/example/notion", "source": "github" },
                    "websiteUrl": "https://notion.so",
                    "packages": [{
                        "registryType": "npm",
                        "identifier": "@modelcontextprotocol/server-notion",
                        "version": "0.6.2",
                        "runtimeHint": "node",
                        "transport": { "type": "stdio" },
                        "environmentVariables": [{
                            "name": "NOTION_API_TOKEN",
                            "description": "Notion integration token",
                            "isRequired": true,
                            "isSecret": true
                        }],
                        "packageArguments": []
                    }],
                    "icons": [{ "src": "https://example.com/icon.svg", "mimeType": "image/svg+xml" }]
                },
                "_meta": null
            }],
            "metadata": { "count": 1, "nextCursor": null }
        });

        // WHEN deserializing
        let response: RegistryListResponse = serde_json::from_value(json).unwrap();

        // THEN all fields are correctly mapped
        assert_eq!(response.servers.len(), 1);
        let detail = &response.servers[0].server;
        assert_eq!(detail.name, "@modelcontextprotocol/server-notion");
        assert_eq!(detail.title.as_deref(), Some("Notion"));
        assert_eq!(detail.version, "0.6.2");

        let pkg = detail.packages.as_ref().unwrap().first().unwrap();
        assert_eq!(pkg.registry_type, "npm");
        assert_eq!(pkg.transport.transport_type, "stdio");
        assert_eq!(pkg.environment_variables.len(), 1);
        assert!(pkg.environment_variables[0].is_secret);
        assert!(pkg.environment_variables[0].is_required);

        assert!(response.metadata.next_cursor.is_none());
    }

    // ── dedup_latest ────────────────────────────────────────────────────────

    #[test]
    fn test_dedup_keeps_latest_version() {
        // GIVEN two entries with the same name, one marked isLatest
        let old = serde_json::from_value::<RegistryServer>(serde_json::json!({
            "server": { "name": "foo/bar", "description": "old", "version": "1.0.0" },
            "_meta": { "io.modelcontextprotocol.registry/official": { "isLatest": false } }
        }))
        .unwrap();

        let latest = serde_json::from_value::<RegistryServer>(serde_json::json!({
            "server": { "name": "foo/bar", "description": "new", "version": "2.0.0" },
            "_meta": { "io.modelcontextprotocol.registry/official": { "isLatest": true } }
        }))
        .unwrap();

        // WHEN deduplicating
        let result = McpRegistryClient::dedup_latest(vec![old, latest]);

        // THEN only the latest entry is kept
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].server.version, "2.0.0");
    }

    // ── remotes deserialization ───────────────────────────────────────────────

    #[test]
    fn test_registry_server_with_remotes_deserializes() {
        // GIVEN a registry JSON payload with a remotes array
        let json = serde_json::json!({
            "server": {
                "name": "@notionhq/notion-mcp-server",
                "title": "Notion",
                "description": "Read and write Notion pages",
                "version": "1.0.0",
                "repository": null,
                "websiteUrl": null,
                "packages": null,
                "icons": null,
                "remotes": [{
                    "type": "streamable-http",
                    "url": "https://mcp.notion.com/mcp",
                    "headers": [{
                        "name": "Authorization",
                        "description": "Bearer token",
                        "isRequired": true,
                        "isSecret": true
                    }]
                }]
            },
            "_meta": null
        });

        // WHEN deserialized
        let server: RegistryServer = serde_json::from_value(json).unwrap();

        // THEN remotes contains the expected URL and transport type
        assert_eq!(server.server.remotes.len(), 1);
        let remote = &server.server.remotes[0];
        assert_eq!(remote.transport_type, "streamable-http");
        assert_eq!(remote.url, "https://mcp.notion.com/mcp");
        assert_eq!(remote.headers.len(), 1);
        assert_eq!(remote.headers[0].name, "Authorization");
        assert!(remote.headers[0].is_required);
        assert!(remote.headers[0].is_secret);
    }

    #[test]
    fn test_registry_server_without_remotes_defaults_empty() {
        // GIVEN a registry JSON payload with no remotes field
        let json = serde_json::json!({
            "server": {
                "name": "@modelcontextprotocol/server-sqlite",
                "title": "SQLite",
                "description": "Query local SQLite databases",
                "version": "0.3.0",
                "repository": null,
                "websiteUrl": null,
                "packages": null,
                "icons": null
            },
            "_meta": null
        });

        // WHEN deserialized
        let server: RegistryServer = serde_json::from_value(json).unwrap();

        // THEN remotes defaults to an empty Vec
        assert!(server.server.remotes.is_empty());
    }

    #[test]
    fn test_dedup_preserves_unique_names() {
        // GIVEN three entries with distinct names
        let a = serde_json::from_value::<RegistryServer>(make_server_json("server-a")).unwrap();
        let b = serde_json::from_value::<RegistryServer>(make_server_json("server-b")).unwrap();
        let c = serde_json::from_value::<RegistryServer>(make_server_json("server-c")).unwrap();

        // WHEN deduplicating
        let result = McpRegistryClient::dedup_latest(vec![a, b, c]);

        // THEN all three are preserved in order
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].server.name, "server-a");
        assert_eq!(result[1].server.name, "server-b");
        assert_eq!(result[2].server.name, "server-c");
    }
}
