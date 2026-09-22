//! The HTTP half of the probe: a bounded range read over a remote GGUF file.

#![cfg(feature = "cloud")]

use thiserror::Error;
use tracing::{event, Level};

use super::parse::{parse_header, GgufHeaderFacts, GgufParseError};
use super::{CONFIRM_BYTES, SCREEN_BYTES};

/// How far into the file a probe reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeDepth {
    /// Enough for the architecture, the hyperparameters and the pre-tokenizer.
    ///
    /// Cheap enough to run across a page of search results.
    Screen,
    /// Enough to clear the vocabulary and reach the chat template.
    ///
    /// Run only for the files actually offered to the operator.
    Confirm,
}

impl ProbeDepth {
    /// The byte budget this depth reads.
    #[must_use]
    pub fn bytes(self) -> u64 {
        match self {
            Self::Screen => SCREEN_BYTES,
            Self::Confirm => CONFIRM_BYTES,
        }
    }
}

/// Failure to probe a remote GGUF header.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum GgufProbeError {
    /// The request itself failed.
    #[error("probe request failed: {0}")]
    Http(#[from] reqwest::Error),

    /// The host answered with a status the probe cannot use.
    #[error("probe refused with status {0}")]
    Status(reqwest::StatusCode),

    /// The bytes came back but do not walk as a GGUF header.
    #[error("probe read a body that is not a GGUF header: {0}")]
    Parse(#[from] GgufParseError),

    /// The body stream failed partway through.
    #[error("probe body read failed: {0}")]
    Body(String),

    /// The URL is not one the probe is willing to dial.
    #[error("probe refused the url: {0}")]
    Url(String),
}

/// Read a remote GGUF header and return the facts it carries.
///
/// Issues a single `Range` request for the first [`ProbeDepth::bytes`] of the
/// file. A host that honours the range answers `206`; one that ignores it
/// answers `200` with the whole file, which is why the body is read through a
/// truncating reader rather than buffered whole.
///
/// A header that the budget did not cover comes back with
/// [`GgufHeaderFacts::truncated`] set. That is a normal result, not an error:
/// the caller decides whether the fields it needs were reached.
///
/// # Errors
/// - [`GgufProbeError::Url`] when the URL is not a public HTTP(S) address.
/// - [`GgufProbeError::Http`] or [`GgufProbeError::Status`] on a failed request.
/// - [`GgufProbeError::Parse`] when the bytes are not a GGUF header.
pub async fn probe_url(
    client: &reqwest::Client,
    url: &str,
    depth: ProbeDepth,
    auth: Option<&reqwest::header::HeaderMap>,
) -> Result<GgufHeaderFacts, GgufProbeError> {
    // The URL comes from a HuggingFace listing, which is to say from outside.
    // A probe that followed it to a loopback or link-local address would turn
    // the recommender into a request forger, so the same guard the rest of the
    // crate applies to outbound calls applies here.
    apollia_core::net::assert_public_str(url).map_err(|e| GgufProbeError::Url(e.to_string()))?;

    let budget = depth.bytes();
    let mut req = client
        .get(url)
        .header(reqwest::header::RANGE, format!("bytes=0-{}", budget - 1));
    if let Some(headers) = auth {
        req = req.headers(headers.clone());
    }

    let resp = req.send().await?;
    let status = resp.status();
    if !status.is_success() {
        return Err(GgufProbeError::Status(status));
    }
    let honoured_range = status == reqwest::StatusCode::PARTIAL_CONTENT;

    let bytes = read_header_prefix(resp, budget).await?;

    event!(
        Level::DEBUG,
        url = %url,
        depth = ?depth,
        bytes_read = bytes.len(),
        honoured_range = honoured_range,
        "gguf.probe.fetched"
    );

    let facts = parse_header(&bytes)?;

    event!(
        Level::DEBUG,
        url = %url,
        architecture = facts.architecture.as_deref().unwrap_or("unknown"),
        tokenizer_pre = facts.tokenizer_pre.as_deref().unwrap_or("unknown"),
        has_chat_template = facts.has_chat_template,
        truncated = facts.truncated,
        "gguf.probe.parsed"
    );

    Ok(facts)
}

/// First size at which the buffered prefix is tried as a complete header.
const FIRST_CHECKPOINT: usize = 256 * 1024;

/// Read the opening of a body until the GGUF header in it is complete, or until
/// `limit` bytes have arrived, whichever comes first.
///
/// The budget is a ceiling, not a target. A confirming probe budgets 24 MiB to
/// clear the largest vocabularies, but most headers end within a few; reading
/// the whole budget anyway made onboarding pull over a hundred megabytes through
/// the operator's connection before it could show a list. The prefix is parsed
/// at doubling checkpoints and the stream is dropped as soon as every declared
/// key/value pair has been walked.
///
/// Deliberately not `apollia_core::net::read_capped_bytes`, which refuses a
/// body that crosses its cap. Here crossing the cap is the expected case: the
/// file is gigabytes and the probe wants only its opening.
async fn read_header_prefix(
    mut response: reqwest::Response,
    limit: u64,
) -> Result<Vec<u8>, GgufProbeError> {
    let cap = usize::try_from(limit).unwrap_or(usize::MAX);
    let mut buf: Vec<u8> = Vec::new();
    let mut next_check = FIRST_CHECKPOINT.min(cap);
    while buf.len() < cap {
        // SAFETY: bounded read by construction. `read_capped_*` refuses a body
        // past its cap, and past the cap is the expected case here: the file
        // is gigabytes and only its opening is wanted. The loop stops at `cap`.
        let chunk = match response.chunk().await {
            Ok(Some(chunk)) => chunk,
            Ok(None) => break,
            Err(e) => return Err(GgufProbeError::Body(e.to_string())),
        };
        let room = cap - buf.len();
        buf.extend_from_slice(&chunk[..chunk.len().min(room)]);

        if buf.len() >= next_check {
            if header_is_complete(&buf) {
                break;
            }
            next_check = next_check.saturating_mul(2).min(cap);
        }
    }
    Ok(buf)
}

/// Whether `prefix` already holds every key/value pair its header declares.
///
/// A parse error is not "complete": the caller keeps reading, and the final
/// parse on the whole prefix reports the error properly.
fn header_is_complete(prefix: &[u8]) -> bool {
    parse_header(prefix).is_ok_and(|facts| facts.is_complete())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_short_prefix_is_not_mistaken_for_a_complete_header() {
        // GIVEN the first bytes of a header that declares more pairs than it holds
        let mut bytes = b"GGUF".to_vec();
        bytes.extend_from_slice(&3u32.to_le_bytes());
        bytes.extend_from_slice(&0u64.to_le_bytes());
        bytes.extend_from_slice(&5u64.to_le_bytes());

        // WHEN completeness is asked
        // THEN the answer is no, so the reader keeps going rather than stopping
        // on a header it has not finished
        assert!(!header_is_complete(&bytes));
    }

    #[test]
    fn a_header_with_every_declared_pair_is_complete() {
        // GIVEN a header that declares no pairs at all
        let mut bytes = b"GGUF".to_vec();
        bytes.extend_from_slice(&3u32.to_le_bytes());
        bytes.extend_from_slice(&0u64.to_le_bytes());
        bytes.extend_from_slice(&0u64.to_le_bytes());

        // WHEN completeness is asked
        // THEN it is complete, which is what lets the stream be dropped early
        assert!(header_is_complete(&bytes));
    }

    #[test]
    fn bytes_that_are_not_gguf_never_end_the_read_early() {
        // GIVEN a buffer that is not a GGUF header at all
        // WHEN completeness is asked
        // THEN the answer is no; the final parse is what reports the error
        assert!(!header_is_complete(b"<html>not a model</html>"));
    }
}
