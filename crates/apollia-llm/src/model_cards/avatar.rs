//! The picture of a publisher, read from its organisation page and kept.
//!
//! A publisher is shown with the picture its organisation carries on
//! HuggingFace. The picture is asked once per organisation, by a request that
//! names the organisation and nothing else, then kept in the cache directory:
//! later openings of the catalogue read the copy and ask nothing. No picture
//! ships with the application.

use std::path::Path;
use std::time::Duration;

use base64::Engine as _;

/// Largest picture accepted, in bytes.
pub const MAX_AVATAR_BYTES: u64 = 256 * 1024;

/// Largest answer accepted from the route that names the picture, in bytes.
const MAX_POINTER_BYTES: u64 = 4 * 1024;

const TIMEOUT: Duration = Duration::from_secs(8);

/// The picture formats shown. A vector picture is left out: it is a document,
/// and nothing here needs one.
const FORMATS: &[&str] = &["image/png", "image/jpeg", "image/webp"];

/// The directory the pictures are kept in, under the data directory.
#[must_use]
pub fn default_avatar_dir(data_dir: &Path) -> std::path::PathBuf {
    data_dir.join("cache").join("publisher-avatars")
}

/// Whether `org` is the name of an organisation: letters, digits, `-`, `_`
/// and `.`, and neither `.` nor `..`.
#[must_use]
pub fn is_org_name(org: &str) -> bool {
    !org.is_empty()
        && org.len() <= 96
        && org != "."
        && org != ".."
        && org
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// The picture of the organisation `org` as a `data:` address, from the copy
/// kept in `cache_dir` when there is one, from the host at `base` otherwise.
///
/// `None` when the name is not one of an organisation, the host cannot be
/// reached, the organisation has no picture, or the picture is too large or
/// of another format: a publisher without a picture is shown by its name.
pub async fn publisher_avatar(base: &str, cache_dir: &Path, org: &str) -> Option<String> {
    if !is_org_name(org) {
        return None;
    }
    let kept = cache_dir.join(format!("{}.avatar", org.to_ascii_lowercase()));
    if let Ok(text) = std::fs::read_to_string(&kept) {
        if text.starts_with("data:image/") {
            return Some(text);
        }
    }
    let picture = match fetch(base, org).await {
        Ok(picture) => picture,
        Err(why) => {
            tracing::event!(
                tracing::Level::INFO,
                org = %org,
                detail = %why,
                "llm.model_cards.avatar_unread"
            );
            return None;
        }
    };
    if let Err(error) =
        std::fs::create_dir_all(cache_dir).and_then(|()| std::fs::write(&kept, &picture))
    {
        tracing::event!(
            tracing::Level::WARN,
            org = %org,
            error = %error,
            "llm.model_cards.avatar_not_kept"
        );
    }
    Some(picture)
}

async fn fetch(base: &str, org: &str) -> Result<String, String> {
    let base = base.trim_end_matches('/');
    let client = apollia_core::net::safe_client_builder()
        .timeout(TIMEOUT)
        .build()
        .map_err(|e| e.to_string())?;

    let pointer = client
        .get(format!("{base}/api/organizations/{org}/avatar"))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !pointer.status().is_success() {
        return Err(format!("the organisation answered {}", pointer.status()));
    }
    let pointer: serde_json::Value =
        apollia_core::net::read_capped_json(pointer, MAX_POINTER_BYTES)
            .await
            .map_err(|e| e.to_string())?;
    let address = pointer
        .get("avatarUrl")
        .and_then(serde_json::Value::as_str)
        .ok_or("the organisation names no picture")?;
    let address = picture_address(base, address).ok_or("the picture is not on the host")?;

    let response = client
        .get(&address)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("the picture answered {}", response.status()));
    }
    let format = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|v| {
            v.split(';')
                .next()
                .unwrap_or_default()
                .trim()
                .to_ascii_lowercase()
        })
        .unwrap_or_default();
    if !FORMATS.contains(&format.as_str()) {
        return Err(format!("a picture of format {format:?} is not shown"));
    }
    let bytes = apollia_core::net::read_capped_bytes(response, MAX_AVATAR_BYTES)
        .await
        .map_err(|e| e.to_string())?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    Ok(format!("data:{format};base64,{encoded}"))
}

/// The address the picture is read from: on the host itself, or on one of its
/// subdomains over HTTPS. Anything else is refused, so the answer of the host
/// cannot send the request elsewhere.
fn picture_address(base: &str, address: &str) -> Option<String> {
    if address.starts_with('/') && !address.starts_with("//") {
        return Some(format!("{base}{address}"));
    }
    if address.starts_with(&format!("{base}/")) {
        return Some(address.to_owned());
    }
    let host = address.strip_prefix("https://")?.split('/').next()?;
    let own = base.strip_prefix("https://")?;
    (host == own || host.ends_with(&format!(".{own}"))).then(|| address.to_owned())
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    async fn host_with_picture(format: &str, bytes: Vec<u8>) -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/organizations/mistralai/avatar"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "avatarUrl": format!("{}/avatars/mistral.png", server.uri())
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/avatars/mistral.png"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(bytes, format))
            .mount(&server)
            .await;
        server
    }

    #[tokio::test]
    async fn a_picture_is_read_once_then_served_from_the_copy() {
        // GIVEN a host that carries the picture of an organisation
        let server = host_with_picture("image/png", vec![1, 2, 3]).await;
        let cache = tempfile::tempdir().expect("tempdir");

        // WHEN the picture is asked twice
        let first = publisher_avatar(&server.uri(), cache.path(), "mistralai").await;
        let second = publisher_avatar(&server.uri(), cache.path(), "MistralAI").await;

        // THEN it comes as a data address both times, and the host was asked
        // for it once: the pointer, then the picture
        assert_eq!(first.as_deref(), Some("data:image/png;base64,AQID"));
        assert_eq!(second, first);
        let requests = server.received_requests().await.expect("recorded");
        assert_eq!(requests.len(), 2);
    }

    #[tokio::test]
    async fn a_picture_of_another_format_or_too_large_is_not_shown() {
        // GIVEN a host whose picture is a vector document, and one whose
        // picture passes the cap
        let vector = host_with_picture("image/svg+xml", b"<svg/>".to_vec()).await;
        let heavy = host_with_picture("image/png", vec![0; MAX_AVATAR_BYTES as usize + 1]).await;
        let cache = tempfile::tempdir().expect("tempdir");

        // WHEN each is asked
        let from_vector = publisher_avatar(&vector.uri(), cache.path(), "mistralai").await;
        let from_heavy = publisher_avatar(&heavy.uri(), cache.path(), "mistralai").await;

        // THEN neither is shown, and nothing is kept
        assert_eq!(from_vector, None);
        assert_eq!(from_heavy, None);
        assert_eq!(
            std::fs::read_dir(cache.path()).expect("readable").count(),
            0
        );
    }

    #[tokio::test]
    async fn a_name_that_is_not_an_organisation_asks_nothing() {
        // GIVEN a host, and a name that climbs out of the route
        let server = MockServer::start().await;
        let cache = tempfile::tempdir().expect("tempdir");

        // WHEN the picture is asked
        let picture = publisher_avatar(&server.uri(), cache.path(), "../models").await;

        // THEN nothing is returned and nothing was requested
        assert_eq!(picture, None);
        assert!(server
            .received_requests()
            .await
            .expect("recorded")
            .is_empty());
    }

    #[test]
    fn the_picture_is_only_read_from_the_host_or_its_subdomains() {
        // GIVEN the public host
        let base = "https://huggingface.co";

        // WHEN the host names a picture on itself, on its subdomain, and elsewhere
        let own = picture_address(base, "/avatars/a.png");
        let sub = picture_address(base, "https://cdn-avatars.huggingface.co/v1/a.png");
        let elsewhere = picture_address(base, "https://example.org/a.png");
        let lookalike = picture_address(base, "https://evilhuggingface.co/a.png");
        let plain = picture_address(base, "http://cdn-avatars.huggingface.co/a.png");

        // THEN the first two are read and the others refused
        assert_eq!(own.as_deref(), Some("https://huggingface.co/avatars/a.png"));
        assert!(sub.is_some());
        assert_eq!(elsewhere, None);
        assert_eq!(lookalike, None);
        assert_eq!(plain, None);
    }
}
