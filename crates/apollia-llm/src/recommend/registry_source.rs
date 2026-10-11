//! The recommendation's candidates, taken from the model registry.
//!
//! The table of `families.toml` orders ten generations by a benchmark score.
//! The registry holds every model Apollia describes, and a first launch should
//! suggest from all of them. A card carries no score, so the order here is a
//! rule, stated once and applied to every card alike:
//!
//! - only a card that calls tools, is meant for general use, is not a draft
//!   model and is published under an open licence is suggested unasked;
//! - a larger model is worth more than a smaller one, by a fixed amount for
//!   each doubling of its parameter count;
//! - a model loses a fixed amount for each month between its release and the
//!   newest release in the registry, so a recent generation passes an older
//!   one of the same size and an older one has to be clearly larger to stay
//!   ahead.
//!
//! Age is counted from the newest card rather than from today, which keeps the
//! order a property of the registry: the same index gives the same order on
//! any day.
//!
//! The result is a [`FamilyManifest`], so the rest of the recommendation (what
//! fits, where the layers run, how fast, which quantisation) is the one path
//! every candidate has always gone through.

use std::collections::BTreeMap;

use crate::model_cards::{CardKind, CardUsage, ModelCard};

use super::manifest::{Family, FamilyManifest, ManifestError, ToolCalling, Variant};

/// Licences a model may carry to be suggested unasked: the ones that leave
/// commercial use free of any condition to read first.
const OPEN_LICENSES: &[&str] = &["apache-2.0", "mit"];

/// Rule score of a model of one billion parameters released with the newest.
const SIZE_BASE: f64 = 40.0;

/// Points each doubling of the parameter count is worth.
const POINTS_PER_DOUBLING: f64 = 12.0;

/// Bounds of the size term, so a very small or very large model does not
/// leave the range the machine terms of the ranking are scaled for.
const SIZE_FLOOR: f64 = 10.0;
const SIZE_CEILING: f64 = 100.0;

/// Points a model loses for each month it is older than the newest card.
///
/// Eight months cost one doubling: a model has to be twice the size of one
/// released eight months later to rank level with it.
const POINTS_PER_MONTH: f64 = 1.5;

/// Most the age term can take away.
const AGE_CEILING: f64 = 45.0;

/// Whether a card may be suggested to someone who has not chosen a model.
#[must_use]
pub fn is_suggested(card: &ModelCard) -> bool {
    card.kind == CardKind::Language
        && card.draft.is_none()
        && card.capabilities.tools
        && card.usage == Some(CardUsage::General)
        && OPEN_LICENSES.contains(&card.license.id.as_str())
        && card.params_b.is_some_and(|p| p > 0.0)
        && card.released.as_deref().and_then(months_of).is_some()
        && card.repos.iter().any(|r| is_gguf_repo(r))
}

/// The rule score of a model of `params_b` billion parameters, `age_months`
/// older than the newest card.
#[must_use]
pub fn rule_score(params_b: f64, age_months: f64) -> f64 {
    let size = (SIZE_BASE + POINTS_PER_DOUBLING * params_b.log2()).clamp(SIZE_FLOOR, SIZE_CEILING);
    let age = (age_months.max(0.0) * POINTS_PER_MONTH).min(AGE_CEILING);
    size - age
}

/// The suggested cards of a registry, as the families the recommendation
/// orders: one family for each group of cards, one variant for each card.
///
/// # Errors
/// [`ManifestError::DuplicateId`] when two groups share an identifier, which
/// the grouping rules out.
pub fn manifest_from_cards(cards: &[ModelCard]) -> Result<FamilyManifest, ManifestError> {
    let suggested: Vec<&ModelCard> = cards.iter().filter(|c| is_suggested(c)).collect();
    let newest = suggested
        .iter()
        .filter_map(|c| c.released.as_deref().and_then(months_of))
        .fold(f64::MIN, f64::max);

    let mut groups: BTreeMap<&str, Vec<&ModelCard>> = BTreeMap::new();
    for card in suggested {
        groups.entry(group_of(&card.id)).or_default().push(card);
    }

    let families = groups
        .into_iter()
        .filter_map(|(group, cards)| family_of(group, &cards, newest))
        .collect();
    FamilyManifest::from_families(families)
}

/// The group a card belongs to: what comes before the first `/` of its id.
fn group_of(id: &str) -> &str {
    id.split_once('/').map_or(id, |(group, _)| group)
}

fn family_of(group: &str, cards: &[&ModelCard], newest: f64) -> Option<Family> {
    let first = cards.first()?;
    let variants = cards
        .iter()
        .filter_map(|card| {
            let params_b = card.params_b?;
            let released = card.released.as_deref().and_then(months_of)?;
            Some(Variant {
                params_b,
                hf_name: repo_name(&card.source_url).to_owned(),
                agentic: None,
                reasoning: None,
                source: Some(card.source_url.clone()),
                mlx_repos: repos_where(card, is_mlx_repo),
                gguf_repos: repos_where(card, is_gguf_repo),
                rule_score: Some(rule_score(params_b, newest - released)),
                label: Some(card.label.clone()),
            })
        })
        .collect();
    let released = cards
        .iter()
        .filter_map(|c| c.released.clone())
        .max()
        .unwrap_or_default();
    Some(Family {
        id: group.to_owned(),
        label: common_label(cards),
        publisher: first.publisher.clone(),
        released,
        architectures: Vec::new(),
        supersedes: Vec::new(),
        tool_calling: ToolCalling::Native,
        license: first.license.id.clone(),
        repo_patterns: Vec::new(),
        notes: None,
        variants,
    })
}

/// The name the cards of a group share: the words their labels begin with in
/// common, the first label when they share none.
fn common_label(cards: &[&ModelCard]) -> String {
    let Some(first) = cards.first() else {
        return String::new();
    };
    let mut shared: Vec<&str> = first.label.split(' ').collect();
    for card in cards.iter().skip(1) {
        let words: Vec<&str> = card.label.split(' ').collect();
        let same = shared
            .iter()
            .zip(&words)
            .take_while(|(a, b)| a == b)
            .count();
        shared.truncate(same);
    }
    if cards.len() > 1 && !shared.is_empty() {
        shared.join(" ")
    } else {
        first.label.clone()
    }
}

/// The repository name of a card's source address.
fn repo_name(source_url: &str) -> &str {
    source_url
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(source_url)
}

fn repos_where(card: &ModelCard, keep: fn(&str) -> bool) -> Vec<String> {
    card.repos.iter().filter(|r| keep(r)).cloned().collect()
}

fn is_gguf_repo(repo: &str) -> bool {
    repo.to_ascii_lowercase().contains("gguf")
}

fn is_mlx_repo(repo: &str) -> bool {
    let lower = repo.to_ascii_lowercase();
    lower.starts_with("mlx-community/") || lower.contains("-mlx")
}

/// An ISO date as a count of months, the day as a fraction of one.
fn months_of(date: &str) -> Option<f64> {
    let mut parts = date.splitn(3, '-');
    let year: f64 = parts.next()?.parse().ok()?;
    let month: f64 = parts.next()?.parse().ok()?;
    let day: f64 = parts.next()?.parse().ok()?;
    ((1.0..=12.0).contains(&month) && (1.0..=31.0).contains(&day))
        .then(|| year * 12.0 + (month - 1.0) + (day - 1.0) / 31.0)
}

#[cfg(test)]
mod tests;
