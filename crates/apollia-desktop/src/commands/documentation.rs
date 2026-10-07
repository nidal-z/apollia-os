//! The documentation embedded in the application: its tree, one page at a
//! time, the images a page shows, and the OpenAPI document its HTTP reference
//! is rendered from.
//!
//! The archive is compiled into this binary ([`crate::embedded_docs`]), so
//! everything here is read with no network and no runtime: the commands serve
//! the embedded and the attached modes alike. The guide shows the page of the
//! current screen when no model can answer; the documentation reader of the
//! developer screen shows the whole site.

use crate::embedded_docs::{DocsError, DocsManifest, DocsPage, EmbeddedDocs};
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

/// The tree, the titles, previous and next of the documentation, as the
/// interface language `language` shows them.
fn manifest(language: &str) -> Result<DocsManifest, DocsError> {
    EmbeddedDocs::embedded()?.manifest(UiLocale::from_tag(language))
}

/// The image a page names by `path`, the path of the public site.
fn image(path: &str) -> Result<Vec<u8>, DocsError> {
    EmbeddedDocs::embedded()?.image(path)
}

/// Reads the manifest of the embedded documentation in the interface language
/// `language`: the sidebar tree, the title of each page with its previous and
/// its next, the languages each page exists in, and the public address.
///
/// # Errors
///
/// The message of a [`DocsError`] when the archive cannot be read.
#[tauri::command]
pub async fn get_documentation_manifest(language: String) -> Result<DocsManifest, String> {
    manifest(&language).map_err(|error| {
        tracing::event!(
            tracing::Level::WARN,
            error = %error,
            "documentation.manifest.read.failed"
        );
        error.to_string()
    })
}

/// Reads an image of the embedded documentation by `path`, the path a page
/// writes it under (such as `/img/operator-help/chat.png`). The answer is the
/// file itself, as raw bytes.
///
/// # Errors
///
/// The message of a [`DocsError`] when `path` names no image of the archive.
#[tauri::command]
pub async fn get_documentation_image(path: String) -> Result<tauri::ipc::Response, String> {
    image(&path)
        .map(tauri::ipc::Response::new)
        .map_err(|error| {
            tracing::event!(
                tracing::Level::WARN,
                path = %path,
                error = %error,
                "documentation.image.read.failed"
            );
            error.to_string()
        })
}

/// The OpenAPI document of the archive, as JSON text.
fn openapi() -> Result<String, DocsError> {
    EmbeddedDocs::embedded()?.openapi()
}

/// Reads the OpenAPI document of the embedded documentation: the published
/// contract of the native API, which the reader renders as the HTTP
/// reference. The answer is the document itself, handed over as the JSON it
/// already is rather than parsed and written again.
///
/// # Errors
///
/// The message of a [`DocsError`] when the archive cannot be read.
#[tauri::command]
pub async fn get_documentation_openapi() -> Result<tauri::ipc::Response, String> {
    openapi().map(tauri::ipc::Response::new).map_err(|error| {
        tracing::event!(
            tracing::Level::WARN,
            error = %error,
            "documentation.openapi.read.failed"
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

    #[test]
    fn test_the_manifest_follows_the_interface_language() {
        // GIVEN the documentation embedded in this binary
        // WHEN its manifest is read for a French and for an English interface
        let french = manifest("fr-FR").unwrap();
        let english = manifest("en").unwrap();

        // THEN each names the public site in its language, carries a tree,
        // and gives every page of the site a title with its neighbours
        assert_eq!(french.view.public_url, "https://docs.apollia.fr/fr");
        assert_eq!(english.view.public_url, "https://docs.apollia.fr");
        assert!(!french.view.tree.is_empty());
        for route in english.pages.keys() {
            assert!(english.view.navigation.contains_key(route), "{route}");
            assert!(french.view.navigation.contains_key(route), "{route}");
        }
        assert_ne!(
            french.view.navigation["/operator-help"].title,
            english.view.navigation["/operator-help"].title
        );
    }

    #[test]
    fn test_an_image_is_the_file_of_the_site_byte_for_byte() {
        // GIVEN an image of the documentation site
        let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/site/static/img/logo.svg");
        let expected = std::fs::read(file).unwrap();

        // WHEN it is read by the path a page writes
        let read = image("/img/logo.svg").unwrap();

        // THEN the archive answers the same bytes
        assert_eq!(read, expected);
    }

    #[test]
    fn test_every_image_a_page_shows_is_in_the_archive() {
        // GIVEN every page of the embedded documentation, in both languages
        let docs = EmbeddedDocs::embedded().unwrap();
        let pages = manifest("en").unwrap().pages;
        let mut seen = 0_usize;

        // WHEN the images its Markdown names under the images folder are read
        for (route, info) in &pages {
            for language in &info.languages {
                let page = docs.page(route, UiLocale::from_tag(language)).unwrap();
                for target in page.markdown.split("](/img/").skip(1) {
                    let rest = target.split(')').next().unwrap();
                    let path = format!("/img/{}", rest.split_whitespace().next().unwrap());

                    // THEN each one is served
                    assert!(docs.image(&path).is_ok(), "{route} ({language}): {path}");
                    seen += 1;
                }
            }
        }
        assert!(seen > 50, "the pages name their screenshots, {seen} read");
    }

    #[test]
    fn test_a_path_outside_the_images_is_refused_by_name() {
        // GIVEN paths that leave the images folder, name another kind of
        // entry, or name an image the site does not have
        let refused = [
            "/img/../manifest.json",
            "/manifest.json",
            "/docs/en/intro.mdx",
            "/img/operator-help/../../openapi.json",
            "/img//logo.svg",
            "/img/logo.exe",
            "/img/no-such-image.png",
        ];

        for path in refused {
            // WHEN each is asked for
            let error = image(path).unwrap_err();

            // THEN the refusal names the path and nothing is read
            assert_eq!(
                error.to_string(),
                format!("no documentation image at {path}")
            );
        }
    }

    #[test]
    fn test_the_openapi_document_is_the_published_contract() {
        // GIVEN the OpenAPI document the clients are generated from
        let file =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../clients/openapi.json");
        let expected = std::fs::read_to_string(file).unwrap();

        // WHEN the archive is asked for its OpenAPI document
        let read = openapi().unwrap();

        // THEN it answers the same text, which is JSON with its paths
        assert_eq!(read, expected);
        let document: serde_json::Value = serde_json::from_str(&read).unwrap();
        assert!(document["paths"]
            .as_object()
            .is_some_and(|paths| !paths.is_empty()));
    }
}
