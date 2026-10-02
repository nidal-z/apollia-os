//! The README of a repository, read on demand and never kept.
//!
//! A README is a text its author holds the rights to. It is fetched from the
//! repository itself when the user asks for it, returned as text with the
//! repository, its owner and its declared licence, and dropped once returned:
//! nothing of it is written to disk or embedded in a build.

use std::time::Duration;

use serde::Serialize;

/// Largest README accepted, in bytes.
pub const MAX_README_BYTES: u64 = 512 * 1024;

/// The HuggingFace host the README is read from.
pub const HUGGINGFACE_BASE: &str = "https://huggingface.co";

const TIMEOUT: Duration = Duration::from_secs(20);

/// A README, as text, with what identifies it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReadmeText {
    /// The repository, `owner/name`.
    pub repo: String,
    /// Its owner.
    pub author: String,
    /// The licence its front matter declares, when it declares one.
    pub license: Option<String>,
    /// The page of the repository.
    pub source_url: String,
    /// The text.
    pub text: String,
}

/// Why no README was returned.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ReadmeError {
    /// The repository identifier is not `owner/name`.
    #[error("invalid repository identifier: {0}")]
    InvalidRepo(String),
    /// The host could not be reached.
    #[error("README unavailable offline")]
    Offline,
    /// The repository has no README.
    #[error("no README in repository {0}")]
    NotFound(String),
    /// The README is larger than [`MAX_README_BYTES`].
    #[error("README larger than {limit} bytes, refused")]
    TooLarge {
        /// The cap.
        limit: u64,
    },
    /// The host answered with an error.
    #[error("README request answered {0}")]
    Status(u16),
    /// The HTTP client could not be built.
    #[error("the http client could not be built")]
    Client(#[source] reqwest::Error),
}

/// Read the README of `repo_id` from the host at `base`.
///
/// # Errors
///
/// [`ReadmeError::InvalidRepo`] for an identifier that is not `owner/name`,
/// [`ReadmeError::Offline`] when the host cannot be reached,
/// [`ReadmeError::NotFound`] on a 404, [`ReadmeError::TooLarge`] over the cap,
/// and [`ReadmeError::Status`] for another error status.
pub async fn fetch_readme(base: &str, repo_id: &str) -> Result<ReadmeText, ReadmeError> {
    let (owner, name) = split_repo(repo_id)?;
    let base = base.trim_end_matches('/');
    let url = format!("{base}/{owner}/{name}/resolve/main/README.md");
    let client = apollia_core::net::safe_client_builder()
        .timeout(TIMEOUT)
        .build()
        .map_err(ReadmeError::Client)?;
    let response = client.get(&url).send().await.map_err(|error| {
        tracing::event!(
            tracing::Level::INFO,
            repo = %repo_id,
            error = %error,
            "llm.model_cards.readme_unreachable"
        );
        ReadmeError::Offline
    })?;
    let status = response.status();
    if status == reqwest::StatusCode::NOT_FOUND {
        return Err(ReadmeError::NotFound(repo_id.to_owned()));
    }
    if !status.is_success() {
        return Err(ReadmeError::Status(status.as_u16()));
    }
    let text = apollia_core::net::read_capped_text(response, MAX_README_BYTES)
        .await
        .map_err(|error| match error {
            apollia_core::net::ReadCappedError::TooLarge { limit, .. } => {
                ReadmeError::TooLarge { limit }
            }
            _ => ReadmeError::Offline,
        })?;
    Ok(ReadmeText {
        repo: format!("{owner}/{name}"),
        author: owner.to_owned(),
        license: front_matter_license(&text),
        source_url: format!("{HUGGINGFACE_BASE}/{owner}/{name}"),
        text,
    })
}

fn split_repo(repo_id: &str) -> Result<(&str, &str), ReadmeError> {
    let invalid = || ReadmeError::InvalidRepo(repo_id.to_owned());
    let (owner, name) = repo_id.split_once('/').ok_or_else(invalid)?;
    let valid = |part: &str| {
        !part.is_empty()
            && part != "."
            && part != ".."
            && part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    };
    if valid(owner) && valid(name) {
        Ok((owner, name))
    } else {
        Err(invalid())
    }
}

/// The `license:` key of the YAML front matter that opens a model card.
fn front_matter_license(text: &str) -> Option<String> {
    let mut lines = text.lines();
    if lines.next()?.trim() != "---" {
        return None;
    }
    for line in lines {
        let line = line.trim_end();
        if line.trim() == "---" {
            return None;
        }
        if let Some(value) = line.strip_prefix("license:") {
            let value = value.trim().trim_matches(['"', '\'']);
            return (!value.is_empty()).then(|| value.to_owned());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_licence_is_read_from_the_front_matter_only() {
        // GIVEN a README whose front matter declares a licence, and one that
        // mentions a licence only in its body
        let with = "---\nlibrary_name: transformers\nlicense: apache-2.0\n---\n# Model\n";
        let without = "# Model\nlicense: mit\n";

        // WHEN the licence is read
        let declared = front_matter_license(with);
        let body_only = front_matter_license(without);

        // THEN only the front matter counts
        assert_eq!(declared.as_deref(), Some("apache-2.0"));
        assert_eq!(body_only, None);
    }

    #[test]
    fn a_repository_identifier_is_owner_and_name_only() {
        // GIVEN identifiers with a path escape, a missing part and a valid one
        // WHEN they are split
        // THEN only `owner/name` made of the usual characters passes
        assert!(split_repo("../etc").is_err());
        assert!(split_repo("Qwen/").is_err());
        assert!(split_repo("Qwen/Qwen3-8B/extra").is_err());
        assert_eq!(split_repo("Qwen/Qwen3-8B").ok(), Some(("Qwen", "Qwen3-8B")));
    }
}
