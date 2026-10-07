//! The documentation site, embedded in the binary and readable offline.
//!
//! The build script packs the site with `docs/site/scripts/embed_docs.py`: the
//! Markdown sources in both languages, the images, the OpenAPI document, and a
//! manifest that carries the sidebar tree, the routes, the titles, previous and
//! next, the languages of each page and the public address. This module opens
//! that archive and answers four questions: the manifest in a language, the
//! source of a page by route and language, an image by its site path, and the
//! OpenAPI document.
//!
//! The language is the application's. A page without a French mirror is served
//! in English and marked as a fallback; the generated references (HTTP API,
//! SDK, CLI) are English only, as on the site, and are not marked: English is
//! their only language rather than a missing translation.

use std::collections::BTreeMap;
use std::io::{Cursor, Read};

use serde::{Deserialize, Serialize};

use crate::i18n::UiLocale;

/// The archive the build script wrote into `OUT_DIR`.
static ARCHIVE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/embedded-docs.zip"));

/// The folder of the archive that holds the images of the site, and the
/// prefix a page writes them under.
const IMAGES_PREFIX: &str = "img/";

/// The image types the reader shows, by file extension. The webview decides
/// the media type from the same extension.
const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "svg"];

/// The manifest's own entry in the archive.
const MANIFEST_ENTRY: &str = "manifest.json";

/// The manifest layout this module reads.
const MANIFEST_FORMAT: u32 = 1;

/// Upper bound of one entry read from the archive. The largest page is a few
/// hundred kilobytes and the OpenAPI document under one megabyte; the bound
/// only stops a corrupt size field from allocating without limit.
const MAX_ENTRY_BYTES: u64 = 16 * 1024 * 1024;

/// Why the embedded documentation could not answer.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub(crate) enum DocsError {
    /// The archive is not a readable zip.
    #[error("the embedded documentation cannot be opened")]
    Archive(#[source] zip::result::ZipError),
    /// An entry the manifest names is not in the archive.
    #[error("the embedded documentation has no entry {name}")]
    MissingEntry {
        /// The entry looked for.
        name: String,
    },
    /// An entry could not be read, or is larger than [`MAX_ENTRY_BYTES`].
    #[error("the entry {name} of the embedded documentation cannot be read")]
    Read {
        /// The entry read.
        name: String,
        /// The underlying failure.
        #[source]
        source: std::io::Error,
    },
    /// A page source is not UTF-8 text.
    #[error("the entry {name} of the embedded documentation is not text")]
    NotText {
        /// The entry read.
        name: String,
    },
    /// The manifest does not parse, or has another format.
    #[error("the embedded documentation manifest is unreadable")]
    Manifest(#[source] serde_json::Error),
    /// The manifest is of a format this build does not read.
    #[error("the embedded documentation manifest has format {found}, expected {MANIFEST_FORMAT}")]
    Format {
        /// The format the manifest declares.
        found: u32,
    },
    /// The manifest has no view in the requested language.
    #[error("the embedded documentation has no {language} view")]
    UnknownLanguage {
        /// The language asked for.
        language: String,
    },
    /// No page is served at this route.
    #[error("no documentation image at {path}")]
    UnknownImage {
        /// The path asked for.
        path: String,
    },
    #[error("no documentation page at {route}")]
    UnknownRoute {
        /// The route asked for.
        route: String,
    },
}

/// The manifest the build script writes, as stored.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Manifest {
    format: u32,
    default_language: String,
    languages: Vec<String>,
    openapi: String,
    pages: BTreeMap<String, PageInfo>,
    locales: BTreeMap<String, LocaleView>,
}

/// One page: where its source sits, and in which languages it exists.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct PageInfo {
    /// Path of the page under the site's `docs/` folder.
    pub(crate) source: String,
    /// Languages the page is written in; English always.
    pub(crate) languages: Vec<String>,
    /// Whether the page is generated from the code (SDK, CLI).
    pub(crate) generated: bool,
}

/// The documentation as one language shows it.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct LocaleView {
    /// Public address of the site in this language.
    pub(crate) public_url: String,
    /// The sidebar tree.
    pub(crate) tree: Vec<TreeItem>,
    /// Title, previous and next of every route the sidebar opens.
    pub(crate) navigation: BTreeMap<String, NavEntry>,
}

/// One item of the sidebar tree.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub(crate) enum TreeItem {
    /// A page.
    Doc {
        /// Its route.
        route: String,
        /// Its label in the tree.
        label: String,
    },
    /// A folder, opening a page or an index of its children when it has a route.
    Category {
        /// Its label in the tree.
        label: String,
        /// The route it opens, if any.
        route: Option<String>,
        /// Its children.
        items: Vec<TreeItem>,
    },
}

/// What a route shows, and its neighbours in reading order.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct NavEntry {
    /// `page` for a page with a source, `generated-index` for the index of a
    /// folder, which lists the folder's children and has no source.
    pub(crate) kind: String,
    /// Title in this language.
    pub(crate) title: String,
    /// The route before, if any.
    pub(crate) previous: Option<NavLink>,
    /// The route after, if any.
    pub(crate) next: Option<NavLink>,
}

/// A neighbour in reading order.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct NavLink {
    /// Its route.
    pub(crate) route: String,
    /// Its title in this language.
    pub(crate) title: String,
}

/// The manifest in one language, as the interface reads it.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct DocsManifest {
    /// The language of the tree and the titles.
    pub(crate) language: String,
    /// The languages the documentation exists in.
    pub(crate) languages: Vec<String>,
    /// Entry of the OpenAPI document in the archive.
    pub(crate) openapi: String,
    /// Every page with a source, by route.
    pub(crate) pages: BTreeMap<String, PageInfo>,
    /// The tree, titles and navigation in this language.
    #[serde(flatten)]
    pub(crate) view: LocaleView,
}

/// A page source, as stored on the site.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct DocsPage {
    /// The route read.
    pub(crate) route: String,
    /// The language asked for.
    pub(crate) requested_language: String,
    /// The language of `markdown`.
    pub(crate) language: String,
    /// Set when the page has no mirror in the requested language and is
    /// served in English instead. Never set on a generated reference.
    pub(crate) fallback: bool,
    /// Whether the page is generated from the code.
    pub(crate) generated: bool,
    /// The Markdown source, byte for byte the site's file.
    pub(crate) markdown: String,
    /// The page on the public site, in the requested language.
    pub(crate) public_url: String,
}

/// The language code of an interface locale.
pub(crate) fn language_of(locale: UiLocale) -> &'static str {
    match locale {
        UiLocale::Fr => "fr",
        UiLocale::En => "en",
    }
}

/// An opened documentation archive.
#[derive(Debug)]
pub(crate) struct EmbeddedDocs<'a> {
    bytes: &'a [u8],
    manifest: Manifest,
}

impl EmbeddedDocs<'static> {
    /// The archive embedded in this binary.
    ///
    /// # Errors
    ///
    /// A [`DocsError`] when the archive or its manifest cannot be read.
    pub(crate) fn embedded() -> Result<Self, DocsError> {
        Self::open(ARCHIVE)
    }
}

impl<'a> EmbeddedDocs<'a> {
    /// Opens an archive and reads its manifest.
    ///
    /// # Errors
    ///
    /// A [`DocsError`] when the archive or its manifest cannot be read, or the
    /// manifest has another format.
    pub(crate) fn open(bytes: &'a [u8]) -> Result<Self, DocsError> {
        let raw = read_entry(bytes, MANIFEST_ENTRY)?;
        let manifest: Manifest = serde_json::from_slice(&raw).map_err(DocsError::Manifest)?;
        if manifest.format != MANIFEST_FORMAT {
            return Err(DocsError::Format {
                found: manifest.format,
            });
        }
        Ok(Self { bytes, manifest })
    }

    /// The manifest as `locale` shows it.
    ///
    /// # Errors
    ///
    /// [`DocsError::UnknownLanguage`] when the manifest has no view in it.
    pub(crate) fn manifest(&self, locale: UiLocale) -> Result<DocsManifest, DocsError> {
        let language = language_of(locale);
        let view = self.locale_view(language)?;
        Ok(DocsManifest {
            language: language.to_owned(),
            languages: self.manifest.languages.clone(),
            openapi: self.manifest.openapi.clone(),
            pages: self.manifest.pages.clone(),
            view: view.clone(),
        })
    }

    /// The source of the page at `route`, in `locale` when it has a mirror,
    /// in English otherwise.
    ///
    /// # Errors
    ///
    /// [`DocsError::UnknownRoute`] when no page is served at `route`, and a
    /// [`DocsError`] when its source cannot be read.
    pub(crate) fn page(&self, route: &str, locale: UiLocale) -> Result<DocsPage, DocsError> {
        let requested = language_of(locale);
        let info = self
            .manifest
            .pages
            .get(route)
            .ok_or_else(|| DocsError::UnknownRoute {
                route: route.to_owned(),
            })?;
        let language = if info.languages.iter().any(|l| l == requested) {
            requested
        } else {
            self.manifest.default_language.as_str()
        };
        let name = format!("docs/{language}/{}", info.source);
        let raw = read_entry(self.bytes, &name)?;
        let markdown = String::from_utf8(raw).map_err(|_| DocsError::NotText { name })?;
        let public_url = format!("{}{route}", self.locale_view(requested)?.public_url);
        Ok(DocsPage {
            route: route.to_owned(),
            requested_language: requested.to_owned(),
            language: language.to_owned(),
            fallback: language != requested && !info.generated,
            generated: info.generated,
            markdown,
            public_url,
        })
    }

    /// The image a page names by `path`, the path of the site (such as
    /// `/img/operator-help/chat.png`).
    ///
    /// # Errors
    ///
    /// [`DocsError::UnknownImage`] when the path leaves the images folder, has
    /// a type the reader does not show, or names no entry of the archive.
    pub(crate) fn image(&self, path: &str) -> Result<Vec<u8>, DocsError> {
        let unknown = || DocsError::UnknownImage {
            path: path.to_owned(),
        };
        let name = path.strip_prefix('/').unwrap_or(path);
        let inside = name.strip_prefix(IMAGES_PREFIX).ok_or_else(unknown)?;
        let plain = inside
            .split('/')
            .all(|segment| !segment.is_empty() && segment != "." && segment != "..");
        if !plain || inside.contains('\\') {
            return Err(unknown());
        }
        let shown = inside.rsplit_once('.').is_some_and(|(_, extension)| {
            IMAGE_EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str())
        });
        if !shown {
            return Err(unknown());
        }
        match read_entry(self.bytes, name) {
            Err(DocsError::MissingEntry { .. }) => Err(unknown()),
            read => read,
        }
    }

    /// The OpenAPI document the HTTP reference is rendered from, as the
    /// archive holds it: the JSON text of the published contract.
    ///
    /// # Errors
    ///
    /// A [`DocsError`] when the entry the manifest names is absent, cannot be
    /// read, or is not text.
    pub(crate) fn openapi(&self) -> Result<String, DocsError> {
        let name = self.manifest.openapi.clone();
        let raw = read_entry(self.bytes, &name)?;
        String::from_utf8(raw).map_err(|_| DocsError::NotText { name })
    }

    fn locale_view(&self, language: &str) -> Result<&LocaleView, DocsError> {
        self.manifest
            .locales
            .get(language)
            .ok_or_else(|| DocsError::UnknownLanguage {
                language: language.to_owned(),
            })
    }
}

/// Reads one entry of the archive, bounded by [`MAX_ENTRY_BYTES`].
fn read_entry(bytes: &[u8], name: &str) -> Result<Vec<u8>, DocsError> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(DocsError::Archive)?;
    let entry = match archive.by_name(name) {
        Ok(entry) => entry,
        Err(zip::result::ZipError::FileNotFound) => {
            return Err(DocsError::MissingEntry {
                name: name.to_owned(),
            })
        }
        Err(error) => return Err(DocsError::Archive(error)),
    };
    let mut out = Vec::new();
    entry
        .take(MAX_ENTRY_BYTES + 1)
        .read_to_end(&mut out)
        .map_err(|source| DocsError::Read {
            name: name.to_owned(),
            source,
        })?;
    if out.len() as u64 > MAX_ENTRY_BYTES {
        return Err(DocsError::Read {
            name: name.to_owned(),
            source: std::io::Error::other("entry larger than the read bound"),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
