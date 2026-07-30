//! Platform home and data directory resolution.
//!
//! One place, so a platform difference is fixed once rather than in every caller.
//!
//! # Why this exists
//!
//! The codebase used to read `std::env::var("HOME")` directly, in 98 places,
//! and 27 of those fell back to a literal `"/tmp"` when the variable was absent.
//! That is wrong twice over. On Windows `HOME` is simply not set by default, so
//! the profile would land in a directory named `/tmp` relative to the current
//! drive; and even on Unix, silently writing user state to a world-writable
//! temporary directory when the environment is unusual is worse than failing.
//!
//! [`std::env::home_dir`] resolves `%USERPROFILE%` on Windows and `$HOME` on
//! Unix, and no longer carries the historical Windows defect that had it
//! deprecated. Callers that genuinely cannot proceed without a home directory
//! should surface the `None`, not invent a path.

use std::path::PathBuf;

/// Name of the runtime's directory inside the user's home.
pub const DATA_DIR_NAME: &str = ".apollia";

/// The current user's home directory.
///
/// `%USERPROFILE%` on Windows, `$HOME` on Unix. Returns `None` when neither is
/// available, which is a real condition on a stripped environment and must be
/// reported rather than papered over.
pub fn home_dir() -> Option<PathBuf> {
    std::env::home_dir()
}

/// The runtime data directory: `<home>/.apollia`.
///
/// Holds the API token, the SQLite databases, models and configuration.
/// Returns `None` when the home directory cannot be resolved.
pub fn data_dir() -> Option<PathBuf> {
    home_dir().map(|h| h.join(DATA_DIR_NAME))
}

/// The home directory, falling back to the platform temporary directory.
///
/// For the call sites that cannot return an error. Prefer [`data_dir_or_err`]
/// wherever the signature allows it: a temporary directory is a place to fail
/// visibly, not a place to keep user state.
///
/// The point of routing the fallback through [`std::env::temp_dir`] rather than
/// a literal `"/tmp"` is that it resolves correctly on every platform, instead
/// of creating a directory named `/tmp` on the current drive under Windows.
pub fn home_dir_or_temp() -> PathBuf {
    home_dir().unwrap_or_else(std::env::temp_dir)
}

/// The home directory as a `String`, for call sites that compose paths by string.
///
/// Prefer [`home_dir`] and `PathBuf::join`: string composition loses the
/// platform separator and breaks on non-UTF-8 paths. This exists to migrate the
/// call sites that already worked that way, not as the recommended shape.
pub fn home_string() -> Option<String> {
    home_dir().map(|p| p.display().to_string())
}

/// The home directory as a `String`, or an error message naming the cause.
pub fn home_string_or_err() -> Result<String, String> {
    home_string().ok_or_else(|| {
        "cannot resolve the home directory (USERPROFILE on Windows, HOME on Unix)".to_string()
    })
}

/// The runtime data directory, or an error message naming the cause.
///
/// For call sites that already return a `Result<_, String>` and want the reason
/// to reach the user instead of a silent fallback.
pub fn data_dir_or_err() -> Result<PathBuf, String> {
    data_dir().ok_or_else(|| {
        "cannot resolve the home directory (USERPROFILE on Windows, HOME on Unix)".to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // GIVEN a normal environment
    // WHEN the data directory is resolved
    // THEN it sits under the home directory and carries the runtime's name,
    //      with no temporary-directory fallback anywhere in the result
    #[test]
    fn test_data_dir_is_under_home_and_named() {
        let home = home_dir().expect("a test host has a home directory");
        let data = data_dir().expect("data dir follows from home");
        assert!(data.starts_with(&home), "{data:?} should be under {home:?}");
        assert_eq!(
            data.file_name().and_then(|s| s.to_str()),
            Some(DATA_DIR_NAME)
        );
    }

    // GIVEN the fallible accessor
    // WHEN the home directory resolves
    // THEN it agrees with the optional accessor, so the two cannot drift
    #[test]
    fn test_fallible_accessor_agrees_with_optional_one() {
        assert_eq!(data_dir_or_err().ok(), data_dir());
    }
}
