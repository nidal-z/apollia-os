//! Fixture format for the connector replay harness.
//!
//! One file per operation, named `<operation_id>.json`, under the crate's
//! `fixtures/` directory. The point of the format is that the expensive field,
//! `response`, is the upstream body pasted verbatim: whoever holds a throwaway
//! Google or Microsoft account records a real answer and drops it in without
//! reshaping it. Everything around that field is small enough to type by hand.
//!
//! `deny_unknown_fields` is deliberate. A misspelled key in a recorded fixture
//! is a fixture that silently measures less than it claims, so it is refused
//! instead of ignored.

use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Where the fixture came from. A harness that cannot tell a recording from a
/// hand-written example reports a coverage it does not have.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Origin {
    /// Copied from a real API answer on a throwaway account.
    Capture,
    /// Written by hand from the vendor's public reference. Proves the harness
    /// runs; proves nothing about what the API actually returns.
    Example,
}

/// One recorded (or hand-written) upstream answer and the reading it must
/// produce.
/// Deserialise a field so a present `null` differs from an absent key.
fn present_even_when_null<'de, D>(deser: D) -> Result<Option<Option<serde_json::Value>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    serde::Deserialize::deserialize(deser).map(Some)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Fixture {
    /// Operation id, e.g. `"gmail.send"`. Must match the file stem.
    pub operation: String,
    /// Whether this is a recording or a hand-written example.
    pub origin: Origin,
    /// Free-text provenance: which account, which date, which reference page.
    pub note: String,
    /// HTTP status served for every body. Defaults to 200.
    #[serde(default = "default_status")]
    pub status: u16,
    /// The upstream body, pasted verbatim, for an operation that makes exactly
    /// one call. A JSON string is served as a raw body rather than as JSON, so
    /// downloads and other non-JSON answers fit here too.
    #[serde(default)]
    pub response: Option<serde_json::Value>,
    /// Bodies served in call order, for an operation that makes several calls.
    #[serde(default)]
    pub responses: Option<Vec<serde_json::Value>>,
    /// What the connector must read out of that answer: the value the client
    /// method returns, serialised.
    ///
    /// Doubly optional on purpose. `Option<Value>` alone cannot tell a missing
    /// key from an explicit `null`, serde mapping both to `None`, so a client
    /// method returning `Option<T>` had no way to assert its `None` path: the
    /// fixture that tried read as one asserting nothing at all. The outer layer
    /// is presence, the inner one is the value.
    #[serde(default, deserialize_with = "present_even_when_null")]
    pub expect: Option<Option<serde_json::Value>>,
    /// Substring the surfaced error must contain, for a fixture that records a
    /// failure answer rather than a success one.
    #[serde(default)]
    pub expect_error: Option<String>,
}

fn default_status() -> u16 {
    200
}

impl Fixture {
    /// The bodies to serve, in call order.
    pub fn bodies(&self) -> Vec<serde_json::Value> {
        match (&self.response, &self.responses) {
            (Some(one), None) => vec![one.clone()],
            (None, Some(many)) => many.clone(),
            _ => Vec::new(),
        }
    }

    /// Reject a fixture that cannot be replayed as written, naming why.
    ///
    /// This runs before the mock server starts, so a malformed fixture reports
    /// its own defect instead of failing later as a confusing mismatch.
    pub fn validate(&self, path: &Path) -> Result<(), String> {
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        let variant_of = format!("{}.", self.operation);
        if stem != self.operation && !stem.starts_with(&variant_of) {
            return Err(format!(
                "file stem {stem:?} is neither the operation {:?} it declares nor a \
                 variant of it ({variant_of}<variant>); the harness finds fixtures by \
                 file name",
                self.operation
            ));
        }
        if self.note.trim().is_empty() {
            return Err(
                "carries an empty `note`. A fixture with no provenance cannot \
                 be judged, replaced, or re-recorded"
                    .into(),
            );
        }
        match (&self.response, &self.responses) {
            (Some(_), Some(_)) => {
                return Err("carries both `response` and `responses`; use one".into())
            }
            (None, None) => {
                return Err("carries neither `response` nor `responses`, so there is \
                     nothing to serve"
                    .into())
            }
            _ => {}
        }
        if self.bodies().is_empty() {
            return Err("`responses` is empty, so there is nothing to serve".into());
        }
        match (&self.expect, &self.expect_error) {
            (Some(_), Some(_)) => {
                return Err("carries both `expect` and `expect_error`; use one".into())
            }
            (None, None) => {
                return Err(
                    "carries neither `expect` nor `expect_error`, so it asserts \
                     nothing about the reading"
                        .into(),
                )
            }
            _ => {}
        }
        if self.status == 429 || (500..=599).contains(&self.status) {
            return Err(format!(
                "status {} drives the retry-and-backoff policy, which sleeps for \
                 seconds and is already covered by the tests in `http.rs`. Record a \
                 terminal status instead",
                self.status
            ));
        }
        Ok(())
    }
}

/// The directory fixtures are read from.
pub(crate) fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures")
}

/// Every fixture on disk, sorted by operation id.
///
/// A file that does not parse is returned as an error rather than skipped: a
/// fixture the harness cannot read is a hole in the measurement, not an
/// absence of one.
pub(crate) fn load_all() -> Vec<(PathBuf, Result<Fixture, String>)> {
    let dir = fixtures_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out: Vec<(PathBuf, Result<Fixture, String>)> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "json"))
        .map(|path| {
            let parsed = std::fs::read_to_string(&path)
                .map_err(|e| format!("unreadable: {e}"))
                .and_then(|text| {
                    serde_json::from_str::<Fixture>(&text).map_err(|e| format!("invalid: {e}"))
                });
            (path, parsed)
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}
