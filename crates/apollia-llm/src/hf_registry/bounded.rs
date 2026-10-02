//! The search, bounded to the official publishers and the recognised
//! converters.
//!
//! Nothing outside the two lists is ever asked: the Hub is queried once per
//! organisation, by its `author=` parameter, and the pages are merged and
//! deduplicated. A query names a model, never an owner, so a free query cannot
//! reach another organisation.
//!
//! - Formats: GGUF everywhere, MLX on Apple Silicon only, where its engine
//!   runs; elsewhere MLX is refused with its reason.
//! - Sorts: likes, downloads and last update go to the Hub; the best match,
//!   the default, is Apollia's order: exact name, then the Apollia selection,
//!   then an official publisher before a converter, then downloads.
//! - Fit: each result's tree is read so its variants are sized for this
//!   machine; `fits_only` keeps the results one of whose variants is
//!   recommended, and the page carries the estimated capacity.
//! - Offline: when no organisation could be reached, the search answers
//!   [`BoundedSearchError::Offline`], never an empty page.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use tracing::{event, Level};

use crate::hardware::{AcceleratorProfile, HardwareProfile};
use crate::hf_registry::card::fill;
use crate::hf_registry::compat::ConfigFacts;
use crate::hf_registry::generated::{generate, GeneratedCard};
use crate::hf_registry::parse::parse_model_list_item;
use crate::hf_registry::{HfModelCard, HfRegistryClient, TreeEntry, HF_MAX_JSON_BYTES};
use crate::model_cards::RegistryIndex;
use crate::recommend::{PublisherKind, PublisherLists};

/// Results returned when the caller names no limit.
pub const DEFAULT_LIMIT: u32 = 20;

/// Most results one search returns.
pub const MAX_LIMIT: u32 = 50;

/// The fields asked of the Hub for each result: enough to classify it, read
/// its origin, date, chat template, parameter count and file names.
const EXPAND: &[&str] = &[
    "author",
    "cardData",
    "config",
    "downloads",
    "gated",
    "gguf",
    "lastModified",
    "likes",
    "pipeline_tag",
    "safetensors",
    "siblings",
    "tags",
];

/// The format a search looks for.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchFormat {
    /// GGUF files, which llama-server loads on every platform.
    #[default]
    Gguf,
    /// MLX folders, which only run on Apple Silicon.
    Mlx,
}

impl SearchFormat {
    /// The Hub tag of the format.
    fn tag(self) -> &'static str {
        match self {
            Self::Gguf => "gguf",
            Self::Mlx => "mlx",
        }
    }

    /// The name a user reads.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Gguf => "GGUF",
            Self::Mlx => "MLX",
        }
    }
}

/// The order of the results.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchSort {
    /// Exact name, then the Apollia selection, then an official publisher
    /// before a converter, then downloads.
    #[default]
    BestMatch,
    /// Most liked first.
    Likes,
    /// Most downloaded first.
    Downloads,
    /// Most recently updated first.
    Updated,
}

impl SearchSort {
    /// Read a sort key: `best_match`, `likes`, `downloads`, `updated` (or the
    /// Hub's `lastModified`). `None` for any other key.
    #[must_use]
    pub fn from_key(key: &str) -> Option<Self> {
        match key.trim() {
            "" | "best_match" => Some(Self::BestMatch),
            "likes" => Some(Self::Likes),
            "downloads" => Some(Self::Downloads),
            "updated" | "lastModified" => Some(Self::Updated),
            _ => None,
        }
    }

    /// The Hub's sort key.
    fn api_key(self) -> &'static str {
        match self {
            Self::BestMatch | Self::Downloads => "downloads",
            Self::Likes => "likes",
            Self::Updated => "lastModified",
        }
    }
}

/// What the caller asks.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct BoundedSearch {
    /// Words of a model name. Empty lists what the organisations publish.
    #[serde(default)]
    pub query: String,
    /// The format looked for.
    #[serde(default)]
    pub format: SearchFormat,
    /// The order of the results.
    #[serde(default)]
    pub sort: SearchSort,
    /// How many results, [`DEFAULT_LIMIT`] when absent, at most [`MAX_LIMIT`].
    #[serde(default)]
    pub limit: Option<u32>,
    /// Keep only the results one of whose variants is recommended here.
    #[serde(default)]
    pub fits_only: bool,
}

/// One organisation the search asks, and what it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SearchAuthor {
    /// The exact HuggingFace handle.
    pub org: String,
    /// Official publisher or recognised converter.
    pub kind: PublisherKind,
}

/// The organisations of the two lists: those the signed index carries when it
/// carries any, the lists embedded in the binary otherwise.
#[must_use]
pub fn search_authors(index: Option<&RegistryIndex>) -> Vec<SearchAuthor> {
    let mut authors: Vec<SearchAuthor> = Vec::new();
    let from_index = index
        .map(|i| &i.publishers)
        .filter(|p| !p.official.is_empty() || !p.converters.is_empty());
    match from_index {
        Some(lists) => {
            for org in lists.official.iter().flat_map(|p| p.orgs.iter()) {
                push_author(&mut authors, org, PublisherKind::Official);
            }
            for org in &lists.converters {
                push_author(&mut authors, org, PublisherKind::Converter);
            }
        }
        None => {
            if let Ok(lists) = PublisherLists::embedded() {
                for org in lists.official().iter().flat_map(|p| p.orgs.iter()) {
                    push_author(&mut authors, org, PublisherKind::Official);
                }
                for org in lists.converters() {
                    push_author(&mut authors, org, PublisherKind::Converter);
                }
            }
        }
    }
    authors
}

fn push_author(authors: &mut Vec<SearchAuthor>, org: &str, kind: PublisherKind) {
    let org = org.trim();
    if org.is_empty() || authors.iter().any(|a| a.org.eq_ignore_ascii_case(org)) {
        return;
    }
    authors.push(SearchAuthor {
        org: org.to_owned(),
        kind,
    });
}

/// What the search reads besides the Hub.
#[derive(Debug, Clone, Copy)]
pub struct SearchContext<'a> {
    /// The organisations asked, from [`search_authors`].
    pub authors: &'a [SearchAuthor],
    /// The signed index, for the cards a result inherits from.
    pub index: Option<&'a RegistryIndex>,
    /// Repositories of the Apollia selection, `owner/name`.
    pub selection: &'a [String],
    /// This machine, when known.
    pub hardware: Option<&'a HardwareProfile>,
}

/// One result: the Hub's facts and the card generated for it. The Hub's facts
/// are serialized at the top level, beside `card` and `in_selection`.
#[derive(Debug, Clone, Serialize)]
pub struct FoundModel {
    /// What the Hub says about the repository.
    #[serde(flatten)]
    pub model: HfModelCard,
    /// The card Apollia builds for it.
    pub card: GeneratedCard,
    /// The repository belongs to the Apollia selection.
    pub in_selection: bool,
}

/// A page of results.
#[derive(Debug, Clone, Serialize)]
pub struct BoundedPage {
    /// The results, in the asked order.
    pub models: Vec<FoundModel>,
    /// The format looked for.
    pub format: SearchFormat,
    /// The order applied.
    pub sort: SearchSort,
    /// What a model can occupy on this machine, in GB rounded to the tenth.
    pub capacity_gb: Option<f64>,
    /// The organisations that could not be reached, when some could.
    pub unreachable: Vec<String>,
}

/// Why a search returned no page.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BoundedSearchError {
    /// No organisation could be reached.
    #[error("search is unavailable offline")]
    Offline,
    /// MLX was asked on a machine that is not Apple Silicon.
    #[error("MLX models only run on Apple Silicon; this machine runs GGUF models")]
    MlxNeedsAppleSilicon,
    /// The two lists are empty, so there is nobody to ask.
    #[error("no publisher list is available to search")]
    NoAuthors,
    /// Every organisation answered with an error.
    #[error("huggingface answered with an error: {0}")]
    Upstream(String),
}

impl BoundedSearchError {
    /// A stable code for the interface.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Offline => "offline",
            Self::MlxNeedsAppleSilicon => "mlx_needs_apple_silicon",
            Self::NoAuthors => "no_authors",
            Self::Upstream(_) => "upstream",
        }
    }
}

/// How one organisation answered.
enum AuthorAnswer {
    Items(Vec<serde_json::Value>),
    Unreachable,
    Failed(String),
}

impl HfRegistryClient {
    /// Search the official publishers and the recognised converters.
    ///
    /// # Errors
    ///
    /// [`BoundedSearchError::MlxNeedsAppleSilicon`] before any request when
    /// MLX is asked elsewhere than on Apple Silicon,
    /// [`BoundedSearchError::NoAuthors`] when the lists are empty,
    /// [`BoundedSearchError::Offline`] when no organisation could be reached,
    /// and [`BoundedSearchError::Upstream`] when every one answered an error.
    pub async fn search_bounded(
        &self,
        search: &BoundedSearch,
        ctx: &SearchContext<'_>,
    ) -> Result<BoundedPage, BoundedSearchError> {
        check_format(search.format, ctx.hardware)?;
        if ctx.authors.is_empty() {
            return Err(BoundedSearchError::NoAuthors);
        }
        let limit = effective_limit(search.limit);

        let answers = futures::future::join_all(
            ctx.authors
                .iter()
                .map(|author| self.author_page(&author.org, search, limit)),
        )
        .await;

        let mut pages = Vec::new();
        let mut unreachable = Vec::new();
        let mut failures = Vec::new();
        for (author, answer) in ctx.authors.iter().zip(answers) {
            match answer {
                AuthorAnswer::Items(items) => pages.push(items),
                AuthorAnswer::Unreachable => unreachable.push(author.org.clone()),
                AuthorAnswer::Failed(why) => failures.push(format!("{}: {why}", author.org)),
            }
        }
        if pages.is_empty() {
            if failures.is_empty() {
                event!(Level::WARN, "llm.hf_search.offline");
                return Err(BoundedSearchError::Offline);
            }
            return Err(BoundedSearchError::Upstream(failures.join("; ")));
        }

        let mut merged = merge(pages, search, ctx);
        merged.truncate(limit as usize);

        let trees =
            futures::future::join_all(merged.iter().map(|(model, _)| self.tree(&model.repo_id)))
                .await;

        let mut results = Vec::with_capacity(merged.len());
        for ((model, item), tree) in merged.into_iter().zip(trees) {
            let tree = tree.unwrap_or_default();
            results.push(found_model(model, &item, &tree, search.format, ctx));
        }
        if search.fits_only {
            results.retain(|r: &FoundModel| r.card.has_recommendation());
        }

        event!(
            Level::INFO,
            results = results.len(),
            authors = ctx.authors.len(),
            unreachable = unreachable.len(),
            "llm.hf_search.bounded"
        );
        Ok(BoundedPage {
            models: results,
            format: search.format,
            sort: search.sort,
            capacity_gb: ctx.hardware.map(crate::recommend::estimated_capacity_gb),
            unreachable,
        })
    }

    /// One organisation's page.
    async fn author_page(&self, org: &str, search: &BoundedSearch, limit: u32) -> AuthorAnswer {
        let url = format!("{}/models", self.api_base);
        let limit = limit.to_string();
        let mut query: Vec<(&str, &str)> = vec![
            ("author", org),
            ("filter", search.format.tag()),
            ("sort", search.sort.api_key()),
            ("direction", "-1"),
            ("limit", &limit),
        ];
        let words = search.query.trim();
        if !words.is_empty() {
            query.push(("search", words));
        }
        query.extend(EXPAND.iter().map(|field| ("expand[]", *field)));

        let resp = match self
            .client
            .get(&url)
            .query(&query)
            .headers(self.auth_headers())
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(e) if e.is_connect() || e.is_timeout() => return AuthorAnswer::Unreachable,
            Err(e) => return AuthorAnswer::Failed(e.to_string()),
        };
        if !resp.status().is_success() {
            return AuthorAnswer::Failed(resp.status().to_string());
        }
        match apollia_core::net::read_capped_json::<serde_json::Value>(resp, HF_MAX_JSON_BYTES)
            .await
        {
            Ok(serde_json::Value::Array(items)) => AuthorAnswer::Items(items),
            Ok(_) => AuthorAnswer::Failed("the answer is not a list".to_owned()),
            Err(e) => AuthorAnswer::Failed(e.to_string()),
        }
    }
}

/// Refuse MLX where its engine does not run.
fn check_format(
    format: SearchFormat,
    hardware: Option<&HardwareProfile>,
) -> Result<(), BoundedSearchError> {
    let apple =
        hardware.is_some_and(|h| matches!(h.accelerator, AcceleratorProfile::AppleSilicon { .. }));
    if format == SearchFormat::Mlx && !apple {
        return Err(BoundedSearchError::MlxNeedsAppleSilicon);
    }
    Ok(())
}

fn effective_limit(limit: Option<u32>) -> u32 {
    limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT)
}

/// Parse every page, drop what is not from an organisation of the lists,
/// deduplicate, and order.
pub(super) fn merge(
    pages: Vec<Vec<serde_json::Value>>,
    search: &BoundedSearch,
    ctx: &SearchContext<'_>,
) -> Vec<(HfModelCard, serde_json::Value)> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut merged = Vec::new();
    for item in pages.into_iter().flatten() {
        let Some(mut model) = parse_model_list_item(&item, None) else {
            continue;
        };
        let owner = model.repo_id.split('/').next().unwrap_or_default();
        let Some(author) = ctx
            .authors
            .iter()
            .find(|a| a.org.eq_ignore_ascii_case(owner))
        else {
            continue;
        };
        if !seen.insert(model.repo_id.to_ascii_lowercase()) {
            continue;
        }
        model.publisher_kind = Some(author.kind);
        merged.push((model, item));
    }
    sort(&mut merged, search, ctx.selection);
    merged
}

/// Order the merged results.
fn sort(
    merged: &mut [(HfModelCard, serde_json::Value)],
    search: &BoundedSearch,
    selection: &[String],
) {
    match search.sort {
        SearchSort::Likes => merged.sort_by_key(|m| std::cmp::Reverse(m.0.likes)),
        SearchSort::Downloads => merged.sort_by_key(|m| std::cmp::Reverse(m.0.downloads)),
        SearchSort::Updated => merged.sort_by(|a, b| b.0.last_modified.cmp(&a.0.last_modified)),
        SearchSort::BestMatch => {
            let query = search.query.trim();
            merged.sort_by_key(|(m, _)| {
                (
                    !is_exact_name(&m.repo_id, query),
                    !in_selection(&m.repo_id, selection),
                    m.publisher_kind != Some(PublisherKind::Official),
                    std::cmp::Reverse(m.downloads),
                )
            });
        }
    }
}

/// The repository's name is the query, with or without the owner and with or
/// without the format suffix a converter appends.
fn is_exact_name(repo_id: &str, query: &str) -> bool {
    if query.is_empty() {
        return false;
    }
    let query = query.rsplit('/').next().unwrap_or(query);
    let name = repo_id.rsplit('/').next().unwrap_or(repo_id);
    let stripped = name
        .strip_suffix("-GGUF")
        .or_else(|| name.strip_suffix("-gguf"))
        .or_else(|| name.strip_suffix("-MLX"))
        .or_else(|| name.strip_suffix("-mlx"))
        .unwrap_or(name);
    // Converters prefix the upstream owner: `Qwen_Qwen3-4B-GGUF`.
    let unprefixed = stripped.split_once('_').map_or(stripped, |(_, rest)| rest);
    [name, stripped, unprefixed]
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(query))
}

fn in_selection(repo_id: &str, selection: &[String]) -> bool {
    selection.iter().any(|r| r.eq_ignore_ascii_case(repo_id))
}

/// Size the variants of one result from its tree and build its card.
pub(super) fn found_model(
    mut model: HfModelCard,
    item: &serde_json::Value,
    tree: &[TreeEntry],
    format: SearchFormat,
    ctx: &SearchContext<'_>,
) -> FoundModel {
    let config = item
        .get("config")
        .map(|c| ConfigFacts::from_json(&c.to_string()))
        .unwrap_or_default();
    fill(&mut model, item, tree, &config, ctx.hardware);
    model.model_type = config.model_type;
    let card = generate(&mut model, item, format, ctx.index);
    let in_selection = in_selection(&model.repo_id, ctx.selection)
        || ctx
            .index
            .is_some_and(|i| i.card_for_repo(&model.repo_id).is_some());
    FoundModel {
        model,
        card,
        in_selection,
    }
}

/// What a search reads from this machine before asking the Hub.
#[derive(Debug, Clone)]
pub struct SearchInputs {
    /// The signed index kept by the last opening of the catalogue, if any.
    pub index: Option<RegistryIndex>,
    /// The organisations asked.
    pub authors: Vec<SearchAuthor>,
    /// Repositories of the editorial selection.
    pub selection: Vec<String>,
}

impl SearchInputs {
    /// Read the kept index from `cache_dir`, checked against the key compiled
    /// into this build, and the embedded selection. No request is made.
    #[must_use]
    pub fn load(cache_dir: &std::path::Path) -> Self {
        let index = match crate::model_cards::cached_index(
            cache_dir,
            crate::model_cards::compiled_public_key(),
        ) {
            Ok(index) => index,
            Err(err) => {
                event!(Level::WARN, error = %err, "llm.hf_search.index_unread");
                None
            }
        };
        let selection = match crate::recommend::FamilyManifest::embedded() {
            Ok(manifest) => selection_repos(&manifest.editorial_selection()),
            Err(err) => {
                event!(Level::ERROR, error = %err, "llm.hf_search.manifest_invalid");
                Vec::new()
            }
        };
        Self {
            authors: search_authors(index.as_ref()),
            index,
            selection,
        }
    }

    /// The context of a search on `hardware`.
    #[must_use]
    pub fn context<'a>(&'a self, hardware: Option<&'a HardwareProfile>) -> SearchContext<'a> {
        SearchContext {
            authors: &self.authors,
            index: self.index.as_ref(),
            selection: &self.selection,
            hardware,
        }
    }
}

/// The repositories of the editorial selection, GGUF and MLX.
#[must_use]
pub fn selection_repos(selection: &[crate::recommend::SelectionFamily]) -> Vec<String> {
    let mut repos: Vec<String> = Vec::new();
    for variant in selection.iter().flat_map(|f| f.variants.iter()) {
        for repo in variant.gguf_repos.iter().chain(variant.mlx_repos.iter()) {
            if !repos.iter().any(|r| r.eq_ignore_ascii_case(repo)) {
                repos.push(repo.clone());
            }
        }
    }
    repos
}

#[cfg(test)]
mod tests;
