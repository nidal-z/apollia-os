//! The documentation embedded in the application, one page at a time.
//!
//! The archive is compiled into this binary ([`crate::embedded_docs`]), so a
//! page is read with no network and no runtime: the command serves the
//! embedded and the attached modes alike. The guide shows the page of the
//! current screen when no model can answer.

use crate::embedded_docs::{DocsError, DocsPage, EmbeddedDocs};
use crate::i18n::UiLocale;

/// The page served at `route`, in the language `language` names when the page
/// has a mirror in it, in English otherwise.
fn page(route: &str, language: &str) -> Result<DocsPage, DocsError> {
    EmbeddedDocs::embedded()?.page(route, UiLocale::from_tag(language))
}

/// Reads the page of the embedded documentation served at `route` (the route
/// of the public site, such as `/operator-help/screens/agents`), in the
/// interface language `language`.
///
/// # Errors
///
/// The message of a [`DocsError`] when no page is served at `route` or the
/// archive cannot be read.
#[tauri::command]
pub async fn get_documentation_page(route: String, language: String) -> Result<DocsPage, String> {
    page(&route, &language).map_err(|error| {
        tracing::event!(
            tracing::Level::WARN,
            route = %route,
            error = %error,
            "documentation.page.read.failed"
        );
        error.to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The contexts the guide documents: one page each on the site.
    const SCREEN_PAGES: &[&str] = &[
        "agents",
        "agent-sheet",
        "ai",
        "automations",
        "batch",
        "batches",
        "chat",
        "connections",
        "developer",
        "first-launch",
        "activity-log",
        "library",
        "memory",
        "observability",
        "organisation-policy",
        "permissions",
        "settings",
        "to-review",
        "transcriptions",
    ];

    #[test]
    fn test_every_screen_has_its_page_in_both_languages() {
        // GIVEN the documentation embedded in this binary
        // WHEN the page of each screen is read in French and in English
        for screen in SCREEN_PAGES {
            let route = format!("/operator-help/screens/{screen}");
            let french = page(&route, "fr").unwrap();
            let english = page(&route, "en-US").unwrap();

            // THEN each is served in its own language, never as a fallback,
            // and names the page of the public site in that language
            assert_eq!(french.language, "fr", "{route}");
            assert!(!french.fallback, "{route}");
            assert_eq!(english.language, "en", "{route}");
            assert_eq!(
                french.public_url,
                format!("https://docs.apollia.fr/fr{route}")
            );
            assert_eq!(
                english.public_url,
                format!("https://docs.apollia.fr{route}")
            );
            assert_ne!(french.markdown, english.markdown, "{route}");
        }
    }

    #[test]
    fn test_a_route_without_a_page_is_refused_by_name() {
        // GIVEN a route the site does not serve
        // WHEN its page is asked for
        let refused = page("/operator-help/screens/pipelines", "fr").unwrap_err();

        // THEN the refusal names the route
        assert_eq!(
            refused.to_string(),
            "no documentation page at /operator-help/screens/pipelines"
        );
    }
}
