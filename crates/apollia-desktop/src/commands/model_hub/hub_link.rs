//! The reading side of the Model Hub on either arm of the seam: the hardware,
//! the search, a model's card, the installed models and the recommendation.
//!
//! Embedded, each runs in this process. Attached, each is asked of the daemon,
//! whose routes run the same code on the same machine, and a refusal of the
//! daemon reaches the interface in the daemon's own words.

use crate::attached::{segment, RuntimeLink};

use super::HfSearchParams;

/// The hardware profile of this machine.
pub(crate) async fn hardware_on(link: &RuntimeLink) -> Result<serde_json::Value, String> {
    match link {
        RuntimeLink::Embedded(_) => super::local_hardware_profile().await,
        RuntimeLink::Attached(remote) => Ok(remote.get("/api/v1/llm/hardware").await?),
    }
}

/// The bounded search of the official publishers and recognised converters.
pub(crate) async fn search_on(
    link: &RuntimeLink,
    params: HfSearchParams,
) -> Result<serde_json::Value, String> {
    let remote = match link {
        RuntimeLink::Embedded(_) => return super::local_search(params).await,
        RuntimeLink::Attached(remote) => remote,
    };
    let format = serde_json::to_value(params.format)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default();
    let mut query = vec![("q", params.query), ("format", format)];
    if let Some(sort) = params.sort {
        query.push(("sort", sort));
    }
    if let Some(limit) = params.limit {
        query.push(("limit", limit.to_string()));
    }
    if params.fits_only {
        query.push(("fits_only", "true".to_owned()));
    }
    if let Some(token) = params.hf_token {
        query.push(("hf_token", token));
    }
    Ok(remote
        .get_from_hub("/api/v1/llm/registry/search", &query)
        .await?)
}

/// The card of the repository `repo_id` (`org/name`).
pub(crate) async fn model_on(
    link: &RuntimeLink,
    cache: &apollia_llm::HfModelTypeCache,
    repo_id: String,
    hf_token: Option<String>,
) -> Result<serde_json::Value, String> {
    let remote = match link {
        RuntimeLink::Embedded(_) => return super::local_model(cache, repo_id, hf_token).await,
        RuntimeLink::Attached(remote) => remote,
    };
    let Some((org, repo)) = repo_id.split_once('/') else {
        return Err(format!(
            "failed to fetch model {repo_id}: not a repository id (org/name)"
        ));
    };
    let path = format!(
        "/api/v1/llm/registry/model/{}/{}",
        segment(org),
        segment(repo)
    );
    let query: Vec<(&str, String)> = hf_token.map(|t| ("hf_token", t)).into_iter().collect();
    Ok(remote.get_from_hub(&path, &query).await?)
}

/// The models installed on this machine.
pub(crate) async fn installed_on(link: &RuntimeLink) -> Result<serde_json::Value, String> {
    match link {
        RuntimeLink::Embedded(_) => super::local_installed_models().await,
        RuntimeLink::Attached(remote) => Ok(remote.get("/api/v1/llm/installed-models").await?),
    }
}

/// The recommendation outcome for this machine.
pub(crate) async fn recommend_on(
    link: &RuntimeLink,
    params: apollia_runtime::recommendation::RecommendParams,
) -> Result<serde_json::Value, String> {
    let remote = match link {
        RuntimeLink::Embedded(_) => {
            let cards_dir = apollia_llm::model_cards::default_cache_dir(
                &crate::commands::model_cards::data_dir(),
            );
            let outcome = apollia_runtime::recommendation::recommend(params, &cards_dir)
                .await
                .map_err(|e| e.to_string())?;
            return serde_json::to_value(outcome).map_err(|e| format!("serialization error: {e}"));
        }
        RuntimeLink::Attached(remote) => remote,
    };
    let mut query = Vec::new();
    if let Some(limit) = params.limit {
        query.push(("limit", limit.to_string()));
    }
    if let Some(n_ctx) = params.n_ctx {
        query.push(("n_ctx", n_ctx.to_string()));
    }
    if params.fits_only {
        query.push(("fits_only", "true".to_owned()));
    }
    if let Some(token) = params.hf_token {
        query.push(("hf_token", token));
    }
    Ok(remote
        .get_from_hub("/api/v1/llm/recommendations", &query)
        .await?)
}

#[cfg(test)]
mod tests;
