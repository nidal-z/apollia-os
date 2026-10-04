//! Whether a public registry entry is the official one of its service.
//!
//! The registry verifies a reverse-domain namespace (`com.notion/...`) by DNS
//! or HTTP against the domain it names; a GitHub namespace (`io.github.<user>`)
//! is verified by account only, so it says nothing about who runs a service.
//! An entry is official when its namespace is such a verified domain and the
//! service it points at lives under that same domain: one of its remote
//! endpoints, or its website. Every other entry is community. A service whose
//! official server is published elsewhere is marked in the curated selection.

use crate::mcp::registry_client::RegistryServerDetail;

/// Whether `detail` is published by the editor of the service it connects to.
#[must_use]
pub fn is_official_registry_entry(detail: &RegistryServerDetail) -> bool {
    let Some(domain) = verified_domain(&detail.name) else {
        return false;
    };
    detail
        .remotes
        .iter()
        .map(|remote| remote.url.as_str())
        .chain(detail.website_url.as_deref())
        .filter_map(host_of)
        .any(|host| is_under(&host, &domain))
}

/// The domain a registry name is published under, when its namespace is a
/// reverse domain: `com.notion/mcp` gives `notion.com`. A GitHub namespace,
/// a single label or a malformed label gives none.
fn verified_domain(name: &str) -> Option<String> {
    let (namespace, _) = name.split_once('/')?;
    let namespace = namespace.to_ascii_lowercase();
    if namespace == "io.github" || namespace.starts_with("io.github.") {
        return None;
    }
    let labels: Vec<&str> = namespace.split('.').collect();
    let well_formed = labels.iter().all(|label| {
        !label.is_empty()
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    });
    if labels.len() < 2 || !well_formed {
        return None;
    }
    Some(labels.into_iter().rev().collect::<Vec<_>>().join("."))
}

/// The lower-cased host of an `http` or `https` address.
fn host_of(address: &str) -> Option<String> {
    let parsed = url::Url::parse(address).ok()?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return None;
    }
    parsed.host_str().map(str::to_ascii_lowercase)
}

/// Whether `host` is `domain` or one of its subdomains.
fn is_under(host: &str, domain: &str) -> bool {
    host == domain
        || host
            .strip_suffix(domain)
            .is_some_and(|prefix| prefix.ends_with('.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detail(name: &str, website: Option<&str>, remotes: &[&str]) -> RegistryServerDetail {
        let remotes: Vec<serde_json::Value> = remotes
            .iter()
            .map(|url| serde_json::json!({ "type": "streamable-http", "url": url }))
            .collect();
        serde_json::from_value(serde_json::json!({
            "name": name,
            "version": "1.0.0",
            "websiteUrl": website,
            "remotes": remotes,
        }))
        .unwrap()
    }

    #[test]
    fn test_a_verified_domain_serving_its_own_endpoint_is_official() {
        // GIVEN entries under a verified domain, served from that domain or
        // documented on it
        let remote = detail("com.notion/mcp", None, &["https://mcp.notion.com/mcp"]);
        let website = detail("app.linear/mcp", Some("https://linear.app/docs/mcp"), &[]);

        // WHEN their badge is decided
        // THEN both are official
        assert!(is_official_registry_entry(&remote));
        assert!(is_official_registry_entry(&website));
    }

    #[test]
    fn test_a_domain_that_does_not_serve_the_service_is_community() {
        // GIVEN an entry whose namespace names a domain while its endpoint and
        // website live elsewhere, and one whose host only ends with the domain
        let elsewhere = detail(
            "ai.smithery/notion",
            Some("https://github.com/someone/notion-mcp"),
            &["https://mcp.notion.com/mcp"],
        );
        let lookalike = detail("com.notion/mcp", None, &["https://evilnotion.com/mcp"]);

        // WHEN their badge is decided
        // THEN neither is official
        assert!(!is_official_registry_entry(&elsewhere));
        assert!(!is_official_registry_entry(&lookalike));
    }

    #[test]
    fn test_a_github_namespace_or_a_bare_name_is_community() {
        // GIVEN a GitHub-verified namespace pointing at github.com, a bare
        // package name, and a namespace with no dot
        let github = detail(
            "io.github.github/github-mcp-server",
            Some("https://github.com/github/github-mcp-server"),
            &["https://api.githubcopilot.com/mcp/"],
        );
        let bare = detail(
            "@notionhq/notion-mcp-server",
            Some("https://notion.so"),
            &[],
        );
        let single = detail("notion/mcp", Some("https://notion"), &[]);

        // WHEN their badge is decided
        // THEN none is official
        assert!(!is_official_registry_entry(&github));
        assert!(!is_official_registry_entry(&bare));
        assert!(!is_official_registry_entry(&single));
    }

    #[test]
    fn test_an_entry_with_no_address_is_community() {
        // GIVEN an entry under a verified domain with no endpoint and no website
        let entry = detail("io.apollia/seed-stub", None, &[]);

        // WHEN its badge is decided
        // THEN it is community
        assert!(!is_official_registry_entry(&entry));
    }
}
