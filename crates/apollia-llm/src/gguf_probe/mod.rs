//! Read a remote GGUF file's header without downloading the weights.
//!
//! Onboarding has to answer, before spending several gigabytes of a user's
//! connection, whether the embedded `llama-server` will load a candidate file
//! and whether it can call tools once loaded. Both answers live in the GGUF
//! key/value header, which sits at the front of the file, so an HTTP range
//! request over the opening megabytes is enough.
//!
//! The module is in two halves. [`parse`] is pure and offline: it walks a byte
//! buffer that may stop anywhere and reports what it found. This module adds
//! the fetch, in two passes:
//!
//! - a **screening** pass over [`SCREEN_BYTES`], which reaches the architecture,
//!   the hyperparameters and `tokenizer.ggml.pre`, because the conversion script
//!   writes those ahead of the vocabulary;
//! - a **confirm** pass over [`CONFIRM_BYTES`], run only for the handful of
//!   files actually offered, which reaches past the vocabulary to
//!   `tokenizer.chat_template` and `general.file_type`.
//!
//! Splitting them matters: a vocabulary runs to several megabytes, and paying
//! that for every search result would cost more than the listing itself.

pub mod parse;

#[cfg(test)]
mod builder;
#[cfg(test)]
mod tests;

pub use parse::{parse_header, GgufHeaderFacts, GgufParseError};

/// Read the header of a GGUF file already on disk.
///
/// The network probe exists to decide whether to download a file; this one
/// exists to decide how to launch one that is already here. Same parser, same
/// budgets, no request.
///
/// # Errors
/// Propagates the read failure, or [`GgufParseError`] when the bytes are not a
/// GGUF header.
pub fn probe_file(
    path: &std::path::Path,
    budget_bytes: u64,
) -> Result<GgufHeaderFacts, std::io::Error> {
    use std::io::Read;

    let file = std::fs::File::open(path)?;
    let mut buf = Vec::new();
    file.take(budget_bytes).read_to_end(&mut buf)?;
    parse_header(&buf).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

#[cfg(feature = "cloud")]
mod fetch;

#[cfg(feature = "cloud")]
pub use fetch::{probe_url, GgufProbeError, ProbeDepth};

/// Bytes read by a screening probe.
///
/// One mebibyte clears the preamble, every `general.*` key, the architecture
/// hyperparameters and `tokenizer.ggml.pre` on every file examined while this
/// was written. A file that manages to push the pre-tokenizer past it comes
/// back truncated, which the verdict treats as "unknown", not as "absent".
pub const SCREEN_BYTES: u64 = 1024 * 1024;

/// Bytes read by a confirming probe.
///
/// Sized to clear a 150000-entry vocabulary plus its merge table, which is what
/// separates `tokenizer.ggml.pre` from `tokenizer.chat_template`. Twenty-four
/// mebibytes covers the largest vocabularies shipped today with room to spare,
/// and is still three orders of magnitude below the weights themselves.
pub const CONFIRM_BYTES: u64 = 24 * 1024 * 1024;
