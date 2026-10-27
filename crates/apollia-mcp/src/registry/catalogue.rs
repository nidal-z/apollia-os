//! The catalogue a caller reads, and the actor that reads the whole registry
//! behind it.
//!
//! [`McpRegistry`] answers from [`McpRegistryClient::read_catalogue`], which
//! never waits for the whole registry, and asks its refresher for the rest
//! when the answer was a stale cache or a first tranche. The refresher is one
//! task fed by a bounded channel: it runs one refresh at a time, and the
//! requests that arrive while it runs are answered by that refresh.

use tokio::sync::{mpsc, watch};

use super::{McpRegistryClient, RegistryClientError, RegistryServer};

/// The registry catalogue with its background refresher.
///
/// Cloning shares the same refresher. The refresher stops once every clone
/// is dropped.
#[derive(Clone)]
pub struct McpRegistry {
    client: McpRegistryClient,
    refresh_tx: mpsc::Sender<()>,
    refreshed: watch::Receiver<u64>,
}

impl McpRegistry {
    /// Start the refresher of `client`'s catalogue. Must be called inside a
    /// Tokio runtime.
    pub fn spawn(client: McpRegistryClient) -> Self {
        let (refresh_tx, refresh_rx) = mpsc::channel(1);
        let (done_tx, refreshed) = watch::channel(0);
        tokio::spawn(run_refresher(client.clone(), refresh_rx, done_tx));
        Self {
            client,
            refresh_tx,
            refreshed,
        }
    }

    /// The servers of the registry, latest version each.
    ///
    /// With `search`, the registry's own search answers (see
    /// [`McpRegistryClient::search`]). Without, the catalogue answers at once
    /// (see [`McpRegistryClient::read_catalogue`]), and the whole registry is
    /// read again in the background when that answer was stale or partial.
    pub async fn list(
        &self,
        search: Option<&str>,
    ) -> Result<Vec<RegistryServer>, RegistryClientError> {
        if let Some(term) = search {
            return self.client.search(term).await;
        }
        let read = self.client.read_catalogue().await?;
        if read.needs_refresh {
            self.request_refresh();
        }
        Ok(read.servers)
    }

    /// Ask for the whole registry to be read again. Returns `false` when a
    /// request is already waiting, which the coming refresh answers.
    pub fn request_refresh(&self) -> bool {
        let queued = self.refresh_tx.try_send(()).is_ok();
        tracing::debug!(queued, "mcp.registry.refresh.requested");
        queued
    }

    /// The number of refreshes finished so far, successful or not, to wait on.
    ///
    /// TEST-ONLY: production never waits on a refresh; a route answers from
    /// what is read and the next read sees what the refresh kept.
    pub fn refreshes(&self) -> watch::Receiver<u64> {
        self.refreshed.clone()
    }
}

/// The refresher: one refresh per request, the requests that came in during
/// a refresh dropped once it is over, since its cache answers them.
async fn run_refresher(
    client: McpRegistryClient,
    mut requests: mpsc::Receiver<()>,
    done: watch::Sender<u64>,
) {
    while requests.recv().await.is_some() {
        match client.refresh().await {
            Ok(servers) => tracing::info!(servers, "mcp.registry.refresh.completed"),
            Err(error) => tracing::warn!(error = %error, "mcp.registry.refresh.failed"),
        }
        while requests.try_recv().is_ok() {}
        done.send_modify(|count| *count += 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, SystemTime};

    use axum::extract::{Query, State};
    use axum::routing::get;
    use axum::{Json, Router};
    use tempfile::TempDir;

    /// A registry of `pages` pages of two servers each, whose pages from
    /// `slow_from` on wait until the gate opens.
    #[derive(Clone)]
    struct FakeRegistry {
        pages: usize,
        slow_from: usize,
        gate: watch::Receiver<bool>,
        first_page_reads: Arc<AtomicUsize>,
        searches: Arc<AtomicUsize>,
    }

    struct Fake {
        uri: String,
        gate: watch::Sender<bool>,
        first_page_reads: Arc<AtomicUsize>,
        searches: Arc<AtomicUsize>,
    }

    fn page_names(page: usize) -> Vec<String> {
        vec![format!("p{page}-a"), format!("p{page}-b")]
    }

    fn body(names: &[String], next: Option<String>) -> serde_json::Value {
        let servers: Vec<_> = names
            .iter()
            .map(|n| serde_json::json!({ "server": { "name": n, "version": "1.0.0" }, "_meta": null }))
            .collect();
        serde_json::json!({ "servers": servers, "metadata": { "count": servers.len(), "nextCursor": next } })
    }

    async fn servers(
        State(fake): State<FakeRegistry>,
        Query(query): Query<HashMap<String, String>>,
    ) -> Json<serde_json::Value> {
        if let Some(term) = query.get("search") {
            fake.searches.fetch_add(1, Ordering::SeqCst);
            let found: Vec<String> = (0..fake.pages)
                .flat_map(page_names)
                .filter(|n| n.contains(term.as_str()))
                .collect();
            return Json(body(&found, None));
        }
        let page = query
            .get("cursor")
            .and_then(|c| c.trim_start_matches('p').parse().ok())
            .unwrap_or(0usize);
        if page == 0 {
            fake.first_page_reads.fetch_add(1, Ordering::SeqCst);
        }
        if page >= fake.slow_from {
            let mut gate = fake.gate.clone();
            let _ = gate.wait_for(|open| *open).await;
        }
        let next = (page + 1 < fake.pages).then(|| format!("p{}", page + 1));
        Json(body(&page_names(page), next))
    }

    async fn fake_registry(pages: usize, slow_from: usize) -> Fake {
        let (gate, gate_rx) = watch::channel(false);
        let first_page_reads = Arc::new(AtomicUsize::new(0));
        let searches = Arc::new(AtomicUsize::new(0));
        let state = FakeRegistry {
            pages,
            slow_from,
            gate: gate_rx,
            first_page_reads: Arc::clone(&first_page_reads),
            searches: Arc::clone(&searches),
        };
        let app = Router::new()
            .route("/v0.1/servers", get(servers))
            .with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await });
        Fake {
            uri: format!("http://{addr}"),
            gate,
            first_page_reads,
            searches,
        }
    }

    fn write_cache(dir: &TempDir, names: &[&str], age: Duration) {
        let entries: Vec<_> = names
            .iter()
            .map(|n| serde_json::json!({ "server": { "name": n, "version": "1.0.0" }, "_meta": null }))
            .collect();
        let path = dir.path().join("mcp-registry.json");
        std::fs::write(&path, serde_json::to_string(&entries).unwrap()).unwrap();
        let file = std::fs::File::options().write(true).open(&path).unwrap();
        file.set_modified(SystemTime::now() - age).unwrap();
    }

    fn names(servers: &[RegistryServer]) -> Vec<String> {
        servers.iter().map(|s| s.server.name.clone()).collect()
    }

    /// A safety net, not a wait: an answer that needs the slow pages never
    /// comes, and the test fails here instead of hanging.
    const NO_HANG: Duration = Duration::from_secs(10);

    /// `future`'s output, or a failure naming `what` once [`NO_HANG`] is
    /// spent: the one deadline these tests arm.
    async fn within<T>(what: &str, future: impl std::future::Future<Output = T>) -> T {
        match tokio::time::timeout(NO_HANG, future).await {
            Ok(output) => output,
            Err(_) => panic!("{what}"),
        }
    }

    async fn wait_refreshes(registry: &McpRegistry, count: u64) {
        let mut done = registry.refreshes();
        within("the refresh did not finish", done.wait_for(|n| *n >= count))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn a_stale_cache_answers_at_once_and_the_refresh_replaces_it() {
        // GIVEN a cache an hour old, and a registry whose every page waits
        let tmp = TempDir::new().unwrap();
        write_cache(&tmp, &["kept-a", "kept-b"], Duration::from_secs(3600));
        let fake = fake_registry(2, 0).await;
        let registry =
            McpRegistry::spawn(McpRegistryClient::with_base_url(tmp.path(), &fake.uri).unwrap());

        // WHEN the catalogue is read
        let first = within(
            "the stale cache must answer without the registry",
            registry.list(None),
        )
        .await
        .unwrap();

        // THEN the kept catalogue answers while the registry is still held
        assert_eq!(names(&first), ["kept-a", "kept-b"]);

        // WHEN the registry answers and the background refresh finishes
        fake.gate.send(true).unwrap();
        wait_refreshes(&registry, 1).await;

        // THEN the next read answers with the whole registry, from the cache
        let second = registry.list(None).await.unwrap();
        assert_eq!(names(&second), ["p0-a", "p0-b", "p1-a", "p1-b"]);
    }

    #[tokio::test]
    async fn without_a_cache_a_first_tranche_answers_and_the_rest_is_kept() {
        // GIVEN no cache, and a registry of seven pages whose pages past the
        // first tranche of five wait
        let tmp = TempDir::new().unwrap();
        let fake = fake_registry(7, 5).await;
        let registry =
            McpRegistry::spawn(McpRegistryClient::with_base_url(tmp.path(), &fake.uri).unwrap());

        // WHEN the catalogue is read for the first time
        let first = within(
            "the first tranche must answer without the slow pages",
            registry.list(None),
        )
        .await
        .unwrap();

        // THEN the five first pages answer, and no partial cache is written
        assert_eq!(first.len(), 10);
        assert_eq!(first[9].server.name, "p4-b");

        // WHEN the slow pages answer and the background refresh finishes
        fake.gate.send(true).unwrap();
        wait_refreshes(&registry, 1).await;

        // THEN the whole registry is kept and answers the next read
        let second = registry.list(None).await.unwrap();
        assert_eq!(second.len(), 14);
        assert_eq!(second[13].server.name, "p6-b");
    }

    #[tokio::test]
    async fn one_refresh_runs_at_a_time() {
        // GIVEN a stale cache and a registry whose pages wait
        let tmp = TempDir::new().unwrap();
        write_cache(&tmp, &["kept"], Duration::from_secs(3600));
        let fake = fake_registry(1, 0).await;
        let registry =
            McpRegistry::spawn(McpRegistryClient::with_base_url(tmp.path(), &fake.uri).unwrap());

        // WHEN the catalogue is read five times while the first refresh is held
        for _ in 0..5 {
            within("the read must not wait", registry.list(None))
                .await
                .unwrap();
        }
        fake.gate.send(true).unwrap();
        wait_refreshes(&registry, 1).await;

        // THEN the registry was read once, and no second refresh is pending
        assert_eq!(fake.first_page_reads.load(Ordering::SeqCst), 1);
        assert_eq!(*registry.refreshes().borrow(), 1);
        assert!(registry.request_refresh());
    }

    #[tokio::test]
    async fn only_a_stale_cache_asks_for_a_refresh() {
        // GIVEN a cache written a minute ago, then the same cache an hour old
        let tmp = TempDir::new().unwrap();
        let client = McpRegistryClient::with_base_url(tmp.path(), "http://127.0.0.1:1").unwrap();
        write_cache(&tmp, &["kept"], Duration::from_secs(60));

        // WHEN the catalogue is read at each age
        let fresh = client.read_catalogue().await.unwrap();
        write_cache(&tmp, &["kept"], Duration::from_secs(3600));
        let stale = client.read_catalogue().await.unwrap();

        // THEN both answer from the cache, and only the stale one asks for a refresh
        assert_eq!(names(&fresh.servers), ["kept"]);
        assert!(!fresh.needs_refresh);
        assert_eq!(names(&stale.servers), ["kept"]);
        assert!(stale.needs_refresh);
    }

    #[tokio::test]
    async fn a_search_asks_the_registry_and_keeps_the_cache() {
        // GIVEN a fresh cache of one entry and a registry of seven pages
        let tmp = TempDir::new().unwrap();
        write_cache(&tmp, &["kept"], Duration::from_secs(60));
        let fake = fake_registry(7, usize::MAX).await;
        let registry =
            McpRegistry::spawn(McpRegistryClient::with_base_url(tmp.path(), &fake.uri).unwrap());

        // WHEN a server of the last page is searched
        let found = registry.list(Some("p6")).await.unwrap();

        // THEN the registry's search finds it, and the cache still holds the catalogue
        assert_eq!(names(&found), ["p6-a", "p6-b"]);
        assert_eq!(fake.searches.load(Ordering::SeqCst), 1);
        assert_eq!(names(&registry.list(None).await.unwrap()), ["kept"]);
    }

    #[tokio::test]
    async fn a_search_without_the_registry_searches_the_cache() {
        // GIVEN a cache and a registry that is not there
        let tmp = TempDir::new().unwrap();
        write_cache(&tmp, &["weather-now", "notion"], Duration::from_secs(3600));
        let registry = McpRegistry::spawn(
            McpRegistryClient::with_base_url(tmp.path(), "http://127.0.0.1:1").unwrap(),
        );

        // WHEN a word is searched
        let found = registry.list(Some("WEATHER")).await.unwrap();

        // THEN the cached entries holding it answer
        assert_eq!(names(&found), ["weather-now"]);
    }

    #[tokio::test]
    async fn without_the_registry_nor_a_cache_the_catalogue_is_offline() {
        // GIVEN no cache and a registry that is not there
        let tmp = TempDir::new().unwrap();
        let registry = McpRegistry::spawn(
            McpRegistryClient::with_base_url(tmp.path(), "http://127.0.0.1:1").unwrap(),
        );

        // WHEN the catalogue and a search are read
        let listed = registry.list(None).await;
        let searched = registry.list(Some("notion")).await;

        // THEN both say the registry is offline
        assert!(matches!(listed, Err(RegistryClientError::Offline)));
        assert!(matches!(searched, Err(RegistryClientError::Offline)));
    }

    #[tokio::test]
    async fn a_registry_slower_than_the_budget_is_offline_without_a_cache() {
        // GIVEN no cache, a registry whose first page waits, and a short budget
        let tmp = TempDir::new().unwrap();
        let fake = fake_registry(1, 0).await;
        let client = McpRegistryClient::with_base_url(tmp.path(), &fake.uri)
            .unwrap()
            .with_tranche_budget(Duration::from_millis(50));

        // WHEN the catalogue is read
        let read = within(
            "the budget must end the first tranche",
            client.read_catalogue(),
        )
        .await;

        // THEN it is offline rather than late
        assert!(matches!(read, Err(RegistryClientError::Offline)));
    }
}
