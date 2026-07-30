//! `link_preview` Tauri command: fetches Open Graph metadata for a URL.
//!
//! Rationale:
//!   - Fetching from the frontend would hit CORS on most servers.
//!   - Running the fetch in Rust keeps the outbound traffic auditable and
//!     keeps Apollia local-first.
//!   - A 24-hour on-disk cache avoids repeat fetches for the same URL.
//!
//! Opt-in: the command is gated behind `[ui].link_preview_enabled` in
//! `apollia.toml`.  When disabled (default), callers receive an explicit
//! error and the UI falls back to rendering the bare link.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime};

use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};

use super::ssrf::{assert_public, SsrfError};

/// Cache TTL: 24 hours.
const CACHE_TTL_SECS: u64 = 24 * 60 * 60;
/// Maximum HTML bytes downloaded per URL (512 KB).
const MAX_BYTES: usize = 512 * 1024;
/// Request timeout (seconds).
const REQUEST_TIMEOUT_SECS: u64 = 5;
/// Maximum HTTP redirects followed.
const MAX_REDIRECTS: usize = 3;
/// User agent advertised to remote servers.
const USER_AGENT: &str = "Apollia-Desktop/0.1 (+https://apollia.ai)";

/// Serialisable preview payload returned to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkPreview {
    pub url: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub favicon: Option<String>,
    pub image: Option<String>,
    pub site_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedPreview {
    preview: LinkPreview,
    fetched_at: u64,
}

/// Disk-backed cache, lazy-initialised, guarded by a mutex.
static CACHE: OnceLock<Mutex<HashMap<String, CachedPreview>>> = OnceLock::new();

fn cache() -> &'static Mutex<HashMap<String, CachedPreview>> {
    CACHE.get_or_init(|| Mutex::new(load_cache()))
}

fn cache_path() -> PathBuf {
    let base = dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("apollia");
    let _ = std::fs::create_dir_all(&base);
    base.join("link_previews.json")
}

fn load_cache() -> HashMap<String, CachedPreview> {
    let path = cache_path();
    match std::fs::read_to_string(&path) {
        Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
        Err(_) => HashMap::new(),
    }
}

fn persist_cache(cache: &HashMap<String, CachedPreview>) {
    if let Ok(raw) = serde_json::to_string(cache) {
        let _ = std::fs::write(cache_path(), raw);
    }
}

fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}

fn lookup_cached(url: &str) -> Option<LinkPreview> {
    let cache = cache().lock().ok()?;
    let entry = cache.get(url)?;
    if now_epoch().saturating_sub(entry.fetched_at) <= CACHE_TTL_SECS {
        Some(entry.preview.clone())
    } else {
        None
    }
}

fn store_cached(url: String, preview: LinkPreview) {
    if let Ok(mut guard) = cache().lock() {
        guard.insert(
            url,
            CachedPreview {
                preview,
                fetched_at: now_epoch(),
            },
        );
        persist_cache(&guard);
    }
}

/// Check the `[ui].link_preview_enabled` flag in `apollia.toml`.
///
/// Defaults to `false` (local-first, opt-in).  Lookup order matches
/// `main.rs::load_apollia_config`: `~/.apollia/apollia.toml` → CWD →
/// `~/.config/apollia/apollia.toml`.
fn link_preview_enabled() -> bool {
    let home = apollia_core::paths::home_dir();
    let candidates: Vec<PathBuf> = [
        home.as_ref()
            .map(|h| h.join(".apollia").join("apollia.toml")),
        std::env::current_dir().ok().map(|d| d.join("apollia.toml")),
        home.as_ref()
            .map(|h| h.join(".config").join("apollia").join("apollia.toml")),
    ]
    .into_iter()
    .flatten()
    .collect();

    for path in candidates {
        if let Ok(raw) = std::fs::read_to_string(&path) {
            if let Ok(parsed) = toml::from_str::<toml::Value>(&raw) {
                if let Some(flag) = parsed
                    .get("ui")
                    .and_then(|ui| ui.get("link_preview_enabled"))
                    .and_then(|v| v.as_bool())
                {
                    return flag;
                }
            }
        }
    }
    false
}

impl From<SsrfError> for String {
    fn from(err: SsrfError) -> Self {
        err.to_string()
    }
}

/// Tauri command: fetch Open Graph metadata for a URL.
///
/// Returns [`LinkPreview`] on success, or a human-readable error string.
#[tauri::command]
pub async fn link_preview(url: String) -> Result<LinkPreview, String> {
    if !link_preview_enabled() {
        return Err("link_preview disabled in apollia.toml".to_string());
    }

    // Normalise + SSRF-check before any network I/O.
    let parsed = url::Url::parse(&url).map_err(|e| format!("invalid URL: {e}"))?;
    assert_public(&parsed).map_err(|e| e.to_string())?;

    let canonical = parsed.as_str().to_string();

    if let Some(cached) = lookup_cached(&canonical) {
        return Ok(cached);
    }

    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
        // Re-validate every redirect hop, not just the initial URL: a public
        // page can `302` the fetch onto a private host.
        .redirect(super::ssrf::public_redirect_policy(MAX_REDIRECTS))
        .build()
        .map_err(|e| format!("reqwest client init failed: {e}"))?;

    let response = client
        .get(parsed.clone())
        .send()
        .await
        .map_err(|e| format!("fetch failed: {e}"))?;

    if !response.status().is_success() {
        return Err(format!(
            "remote responded with status {}",
            response.status()
        ));
    }

    let final_url = response.url().clone();
    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("response body read failed: {e}"))?;

    let slice = if bytes.len() > MAX_BYTES {
        &bytes[..MAX_BYTES]
    } else {
        &bytes[..]
    };
    let html = String::from_utf8_lossy(slice);
    let preview = extract_og_tags(&final_url, &html);

    store_cached(canonical, preview.clone());

    Ok(preview)
}

/// Open Graph / description metadata gathered from `<meta>` tags.
#[derive(Default)]
struct MetaTags {
    title: Option<String>,
    description: Option<String>,
    image: Option<String>,
    site_name: Option<String>,
}

/// Collect the relevant `<meta og:*>` / `<meta name="description">` values.
fn collect_meta_tags(doc: &Html) -> MetaTags {
    let meta_sel = Selector::parse("meta").expect("static selector");
    let mut out = MetaTags::default();
    for meta in doc.select(&meta_sel) {
        let property = meta
            .value()
            .attr("property")
            .or_else(|| meta.value().attr("name"));
        let content = match meta.value().attr("content") {
            Some(c) => c.trim().to_string(),
            None => continue,
        };
        if content.is_empty() {
            continue;
        }
        match property {
            Some("og:title") => out.title.get_or_insert(content),
            Some("og:description") | Some("description") => out.description.get_or_insert(content),
            Some("og:image") => out.image.get_or_insert(content),
            Some("og:site_name") => out.site_name.get_or_insert(content),
            _ => continue,
        };
    }
    out
}

/// Find the first `<link rel="...icon...">` href, if any.
fn find_favicon(doc: &Html) -> Option<String> {
    let link_sel = Selector::parse("link[rel]").expect("static selector");
    for link in doc.select(&link_sel) {
        let rel = link.value().attr("rel").unwrap_or("");
        if rel.to_ascii_lowercase().contains("icon") {
            if let Some(href) = link.value().attr("href") {
                return Some(href.to_string());
            }
        }
    }
    None
}

/// Parse `<title>`, `<meta og:*>`, `<meta name="description">`, and
/// `<link rel="icon">` from an HTML string.
fn extract_og_tags(base_url: &url::Url, html: &str) -> LinkPreview {
    let doc = Html::parse_document(html);

    let title_sel = Selector::parse("title").expect("static selector");

    let mut meta = collect_meta_tags(&doc);

    if meta.title.is_none() {
        if let Some(t) = doc.select(&title_sel).next() {
            let text: String = t.text().collect::<String>().trim().to_string();
            if !text.is_empty() {
                meta.title = Some(text);
            }
        }
    }

    let favicon = find_favicon(&doc);
    let MetaTags {
        title,
        description,
        image,
        site_name,
    } = meta;

    // Resolve relative URLs against the final (post-redirect) URL.
    let resolve = |candidate: Option<String>| -> Option<String> {
        let raw = candidate?;
        match base_url.join(&raw) {
            Ok(u) => Some(u.to_string()),
            Err(_) => Some(raw),
        }
    };

    LinkPreview {
        url: base_url.to_string(),
        title,
        description,
        favicon: resolve(favicon).or_else(|| {
            // Fallback: try /favicon.ico at the origin.
            base_url.join("/favicon.ico").ok().map(|u| u.to_string())
        }),
        image: resolve(image),
        site_name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_og_tags() {
        let html = r#"
            <html>
              <head>
                <title>Fallback title</title>
                <meta property="og:title" content="OG title">
                <meta property="og:description" content="short desc">
                <meta property="og:image" content="/hero.png">
                <meta property="og:site_name" content="Example">
                <link rel="icon" href="/favicon.ico">
              </head>
            </html>
        "#;
        let base = url::Url::parse("https://example.com/article").unwrap();
        let preview = extract_og_tags(&base, html);
        assert_eq!(preview.title.as_deref(), Some("OG title"));
        assert_eq!(preview.description.as_deref(), Some("short desc"));
        assert_eq!(
            preview.image.as_deref(),
            Some("https://example.com/hero.png")
        );
        assert_eq!(preview.site_name.as_deref(), Some("Example"));
        assert_eq!(
            preview.favicon.as_deref(),
            Some("https://example.com/favicon.ico")
        );
    }

    #[test]
    fn falls_back_to_title_tag() {
        let html = "<html><head><title>Only title</title></head></html>";
        let base = url::Url::parse("https://example.com/").unwrap();
        let preview = extract_og_tags(&base, html);
        assert_eq!(preview.title.as_deref(), Some("Only title"));
    }
}
