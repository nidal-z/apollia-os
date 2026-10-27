//! REST routes over the public MCP registry, under `/api/v1/mcp/registry`.
//!
//! The desktop wizard has always browsed `registry.modelcontextprotocol.io`
//! through [`McpRegistryClient`]; a caller without a desktop had no catalogue
//! but one written by hand. These routes read the same client, so they share
//! its cache (`<data_dir>/mcp-registry.json`, fresh for fifteen minutes), its
//! `APOLLIA_MCP_REGISTRY_URL` override, and its fallback: when the registry
//! does not answer, the cached catalogue is served, and only when there is no
//! cache either does the route fail.
//!
//! They never wait for the whole registry: the cache answers even when it is
//! stale, a first tranche answers when there is none, and the runtime's
//! `McpRegistry` reads the rest in the background.

use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::Deserialize;

use apollia_mcp::registry::{McpRegistryClient, RegistryClientError, RegistryServer};

use crate::api::openapi::ApiErrorBody;
use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;

/// Build the MCP registry router.
///
/// Mounted under the root router by [`crate::api::server::build_router`].
pub fn mcp_registry_router<B: ExecutionBackend + Clone>() -> Router<AppState<B>> {
    Router::new()
        .route("/api/v1/mcp/registry", get(list_registry_servers::<B>))
        .route("/api/v1/mcp/registry/server", get(get_registry_server::<B>))
}

type ApiError = (StatusCode, Json<ApiErrorBody>);

fn api_err(code: StatusCode, msg: impl std::fmt::Display) -> ApiError {
    (
        code,
        Json(ApiErrorBody {
            error: msg.to_string(),
        }),
    )
}

fn registry_err(err: RegistryClientError) -> ApiError {
    let code = match err {
        RegistryClientError::Offline => StatusCode::SERVICE_UNAVAILABLE,
        _ => StatusCode::BAD_GATEWAY,
    };
    api_err(code, err)
}

/// The servers of the registry, through the runtime's catalogue and its
/// refresher; without one (a test state), through a client that reads the
/// cache or a first tranche and refreshes nothing.
async fn list_servers<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
    search: Option<&str>,
) -> Result<Vec<RegistryServer>, ApiError> {
    if let Some(registry) = &state.mcp_registry {
        return registry.list(search).await.map_err(registry_err);
    }
    let client = McpRegistryClient::new(&state.data_dir)
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    let read = match search {
        Some(term) => client.search(term).await,
        None => client.read_catalogue().await.map(|read| read.servers),
    };
    read.map_err(registry_err)
}

/// Query of `GET /api/v1/mcp/registry`.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct RegistryListQuery {
    /// Keyword forwarded to the registry's own search. A search always asks
    /// the network and falls back to the cache; without one, the cache answers
    /// even when stale, and the whole registry is read again behind it.
    pub search: Option<String>,
}

/// Query of `GET /api/v1/mcp/registry/server`.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct RegistryServerQuery {
    /// Registry name of the server, e.g. `io.github.owner/server`. A query
    /// parameter rather than a path segment, because registry names carry a
    /// slash.
    pub name: String,
}

/// `GET /api/v1/mcp/registry`: the servers the MCP registry lists, the latest
/// version of each, in the registry's own shape.
#[utoipa::path(
    get,
    path = "/api/v1/mcp/registry",
    tag = "mcp",
    params(RegistryListQuery),
    responses(
        (status = 200, description = "Registry entries, latest version per server; from the cache when the registry is unreachable"),
        (status = 502, description = "The registry answered something unreadable and no cache covers it", body = ApiErrorBody),
        (status = 503, description = "The registry is unreachable and no cache exists", body = ApiErrorBody),
    )
)]
pub(crate) async fn list_registry_servers<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Query(query): Query<RegistryListQuery>,
) -> Result<Json<Vec<RegistryServer>>, ApiError> {
    let search = query
        .search
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    Ok(Json(list_servers(&state, search).await?))
}

/// `GET /api/v1/mcp/registry/server?name=`: one registry entry, its latest
/// version, read from the same catalogue as the listing.
#[utoipa::path(
    get,
    path = "/api/v1/mcp/registry/server",
    tag = "mcp",
    params(RegistryServerQuery),
    responses(
        (status = 200, description = "The registry entry"),
        (status = 404, description = "No server of that name in the registry", body = ApiErrorBody),
        (status = 502, description = "The registry answered something unreadable and no cache covers it", body = ApiErrorBody),
        (status = 503, description = "The registry is unreachable and no cache exists", body = ApiErrorBody),
    )
)]
pub(crate) async fn get_registry_server<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Query(query): Query<RegistryServerQuery>,
) -> Result<Json<RegistryServer>, ApiError> {
    let named = |servers: Vec<RegistryServer>| {
        servers
            .into_iter()
            .find(|entry| entry.server.name == query.name)
    };
    if let Some(entry) = named(list_servers(&state, None).await?) {
        return Ok(Json(entry));
    }
    // The catalogue may be a first tranche still: the registry's search by
    // the name finds an entry past it.
    named(list_servers(&state, Some(&query.name)).await?)
        .map(Json)
        .ok_or_else(|| {
            api_err(
                StatusCode::NOT_FOUND,
                format!("no server named {} in the registry", query.name),
            )
        })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::time::Duration;

    use apollia_mcp::registry::{McpRegistry, McpRegistryClient};
    use axum::extract::{Query, State};
    use axum::http::StatusCode;
    use axum::routing::get;
    use axum::{Json, Router};
    use tokio::sync::watch;

    use crate::api::test_support::{app_state, call};

    /// A safety net, not a wait: a route that waits for the held pages fails here.
    const NO_HANG: Duration = Duration::from_secs(10);

    /// `future`'s output, or a failure naming `what` once [`NO_HANG`] is
    /// spent: the one deadline these tests arm.
    async fn within<T>(what: &str, future: impl std::future::Future<Output = T>) -> T {
        match tokio::time::timeout(NO_HANG, future).await {
            Ok(output) => output,
            Err(_) => panic!("{what}"),
        }
    }

    fn page_names(page: usize) -> Vec<String> {
        vec![
            format!("io.example/p{page}-a"),
            format!("io.example/p{page}-b"),
        ]
    }

    fn body(names: &[String], next: Option<String>) -> serde_json::Value {
        let servers: Vec<_> = names
            .iter()
            .map(|n| serde_json::json!({ "server": { "name": n, "version": "1.0.0" }, "_meta": null }))
            .collect();
        serde_json::json!({ "servers": servers, "metadata": { "count": servers.len(), "nextCursor": next } })
    }

    /// Seven pages; from the sixth on, a page waits until the gate opens.
    async fn servers(
        State(gate): State<watch::Receiver<bool>>,
        Query(query): Query<HashMap<String, String>>,
    ) -> Json<serde_json::Value> {
        if let Some(term) = query.get("search") {
            let found: Vec<String> = (0..7)
                .flat_map(page_names)
                .filter(|n| n.contains(term.as_str()))
                .collect();
            return Json(body(&found, None));
        }
        let page = query
            .get("cursor")
            .and_then(|c| c.parse().ok())
            .unwrap_or(0usize);
        if page >= 5 {
            let mut gate = gate.clone();
            let _ = gate.wait_for(|open| *open).await;
        }
        let next = (page < 6).then(|| (page + 1).to_string());
        Json(body(&page_names(page), next))
    }

    async fn fake_registry() -> (String, watch::Sender<bool>) {
        let (gate, gate_rx) = watch::channel(false);
        let app = Router::new()
            .route("/v0.1/servers", get(servers))
            .with_state(gate_rx);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(async move { axum::serve(listener, app).await });
        (format!("http://{addr}"), gate)
    }

    fn names(listed: &serde_json::Value) -> Vec<String> {
        listed
            .as_array()
            .expect("a list")
            .iter()
            .map(|e| e["server"]["name"].as_str().unwrap_or_default().to_owned())
            .collect()
    }

    #[tokio::test]
    async fn the_first_listing_answers_with_a_first_tranche_then_the_whole_registry() {
        // GIVEN a runtime without a registry cache, over a registry of seven
        // pages whose two last pages are held
        let dir = tempfile::tempdir().expect("temp dir");
        let (uri, gate) = fake_registry().await;
        let registry =
            McpRegistry::spawn(McpRegistryClient::with_base_url(dir.path(), &uri).expect("client"));
        let mut state = app_state(dir.path().to_path_buf());
        state.mcp_registry = Some(registry.clone());

        // WHEN the catalogue is listed
        let (status, listed) = within(
            "the route must not wait for the held pages",
            call(state.clone(), "GET", "/api/v1/mcp/registry", None),
        )
        .await;

        // THEN the five first pages answer
        assert_eq!(status, StatusCode::OK);
        assert_eq!(names(&listed).len(), 10);

        // WHEN the held pages answer and the background refresh finishes
        gate.send(true).expect("gate");
        let mut done = registry.refreshes();
        within("the refresh finishes", done.wait_for(|n| *n >= 1))
            .await
            .expect("refresher alive");
        let (status, listed) = call(state, "GET", "/api/v1/mcp/registry", None).await;

        // THEN the next listing holds the whole registry
        assert_eq!(status, StatusCode::OK);
        assert_eq!(names(&listed).len(), 14);
    }

    #[tokio::test]
    async fn a_search_finds_a_server_past_the_first_tranche() {
        // GIVEN a runtime over a registry of seven pages
        let dir = tempfile::tempdir().expect("temp dir");
        let (uri, _gate) = fake_registry().await;
        let mut state = app_state(dir.path().to_path_buf());
        state.mcp_registry = Some(McpRegistry::spawn(
            McpRegistryClient::with_base_url(dir.path(), &uri).expect("client"),
        ));

        // WHEN a server of the seventh page is searched
        let (status, listed) = within(
            "a search does not wait for the held pages",
            call(state, "GET", "/api/v1/mcp/registry?search=p6", None),
        )
        .await;

        // THEN the registry's own search finds it
        assert_eq!(status, StatusCode::OK);
        assert_eq!(names(&listed), ["io.example/p6-a", "io.example/p6-b"]);
    }

    #[tokio::test]
    async fn a_server_past_the_first_tranche_is_found_by_its_name() {
        // GIVEN a runtime without a cache, over a registry whose last pages are held
        let dir = tempfile::tempdir().expect("temp dir");
        let (uri, _gate) = fake_registry().await;
        let mut state = app_state(dir.path().to_path_buf());
        state.mcp_registry = Some(McpRegistry::spawn(
            McpRegistryClient::with_base_url(dir.path(), &uri).expect("client"),
        ));

        // WHEN a server of the seventh page is asked for by its name
        let (status, entry) = within(
            "the detail does not wait for the held pages",
            call(
                state,
                "GET",
                "/api/v1/mcp/registry/server?name=io.example%2Fp6-a",
                None,
            ),
        )
        .await;

        // THEN the registry's search finds it
        assert_eq!(status, StatusCode::OK);
        assert_eq!(entry["server"]["name"], "io.example/p6-a");
    }

    #[tokio::test]
    async fn without_the_registry_a_stale_cache_answers_and_without_it_the_route_fails() {
        // GIVEN a registry that is not there, and a runtime with no cache
        let dir = tempfile::tempdir().expect("temp dir");
        let mut state = app_state(dir.path().to_path_buf());
        state.mcp_registry = Some(McpRegistry::spawn(
            McpRegistryClient::with_base_url(dir.path(), "http://127.0.0.1:1").expect("client"),
        ));

        // WHEN the catalogue is listed
        let (offline, _) = call(state.clone(), "GET", "/api/v1/mcp/registry", None).await;

        // THEN the route says the registry is unreachable
        assert_eq!(offline, StatusCode::SERVICE_UNAVAILABLE);

        // WHEN a cache an hour old is there and the catalogue is listed again
        let cache = dir.path().join("mcp-registry.json");
        std::fs::write(
            &cache,
            serde_json::to_string(&serde_json::json!([
                { "server": { "name": "io.example/kept", "version": "1.0.0" }, "_meta": null }
            ]))
            .expect("json"),
        )
        .expect("write cache");
        std::fs::File::options()
            .write(true)
            .open(&cache)
            .expect("open cache")
            .set_modified(std::time::SystemTime::now() - Duration::from_secs(3600))
            .expect("age cache");
        let (status, listed) = call(state, "GET", "/api/v1/mcp/registry", None).await;

        // THEN the kept catalogue answers
        assert_eq!(status, StatusCode::OK);
        assert_eq!(names(&listed), ["io.example/kept"]);
    }
}
