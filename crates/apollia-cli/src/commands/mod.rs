//! CLI command modules.

/// Truncates `s` for single-line display, on a UTF-8 char boundary.
///
/// When `s` is longer than `over` bytes, keeps the first `keep` bytes (floored
/// to the nearest char boundary so a multibyte codepoint is never split) and
/// appends `ellipsis`. Shorter strings are returned unchanged. For ASCII input
/// this reproduces a plain `&s[..keep]` slice, but it never panics on
/// multibyte text.
pub(crate) fn truncate_for_display(s: &str, over: usize, keep: usize, ellipsis: &str) -> String {
    if s.len() <= over {
        return s.to_string();
    }
    let cut = apollia_core::floor_char_boundary(s, keep);
    format!("{}{ellipsis}", &s[..cut])
}

#[cfg(test)]
mod truncate_tests {
    use super::truncate_for_display;

    // GIVEN a run of multibyte chars whose byte length exceeds the cut and whose
    // `keep`-th byte lands adjacent to a multibyte boundary
    // WHEN truncating for display
    // THEN it does not panic and keeps only whole chars before the ellipsis
    #[test]
    fn test_truncate_multibyte_does_not_panic() {
        let s = "é".repeat(40); // 80 bytes
        let out = truncate_for_display(&s, 60, 57, "...");
        assert!(out.ends_with("..."));
        let body = out.trim_end_matches("...");
        assert!(body.chars().all(|c| c == 'é'), "only whole chars kept");
        assert!(body.len() <= 57);
    }

    // GIVEN an ASCII string longer than the cut
    // WHEN truncating
    // THEN it matches the exact byte slice the old code produced
    #[test]
    fn test_truncate_ascii_is_byte_identical() {
        let s = "abcdefghijklmnopqrstuvwxyz"; // 26 ASCII bytes
        assert_eq!(truncate_for_display(s, 10, 10, "..."), "abcdefghij...");
    }

    // GIVEN a string within the trigger threshold
    // WHEN truncating
    // THEN it is returned unchanged (no ellipsis)
    #[test]
    fn test_truncate_short_unchanged() {
        assert_eq!(truncate_for_display("hi", 10, 7, "…"), "hi");
    }
}

pub mod a2a;
pub mod agent;
pub mod audit;
pub mod chat;
pub mod chat_config;
pub mod chat_stream;
pub mod completions;
pub mod config;
pub mod connector;
pub mod digest;
pub mod do_cmd;
pub mod doctor;
pub mod eval;
pub mod explain;
pub mod fuzzy;
pub mod guide;
pub mod hooks;
pub mod inspect;
pub mod llm;
pub mod logs;
pub mod mcp;
pub mod mcp_oauth;
pub mod mcp_server;
pub mod memory;
pub mod model;
pub mod notify;
pub mod onboard;
pub mod permissions;
pub mod plan_cache;
pub mod project;
pub mod resilience;
pub mod review;
pub mod run;
pub mod start;
pub mod status;
pub mod stop;
pub mod stt;
pub mod suggest;
pub mod task;
pub mod tools;
pub mod trace;
pub mod trigger;
pub mod update;
pub mod user_memory;
pub mod version;
pub mod workspace;
