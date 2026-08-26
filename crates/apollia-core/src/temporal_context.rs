//! Temporal + system context block injected at the top of every LLM system
//! prompt across all three runtime modes (Chat Libre, Chat Agent, Triggers).
//!
//! Problem this solves: LLMs have a training cutoff date and default to it
//! when they see "today" without explicit grounding. A user asking "create a
//! calendar event for today" got an event scheduled at the model's training
//! cutoff (Oct 2023) instead of the real wall-clock date (May 2026).
//!
//! Solution: prepend an authoritative, unambiguous, ISO-formatted environment
//! block to every system prompt: Chat Libre's `build_system_prompt`,
//! and the apollia-aip `ctx.llm.chat()` / `ctx.llm.complete()` Python bridge.
//! The block lives at the **top** of the prompt with an explicit override
//! instruction so the LLM treats it as ground truth, not as one fact among
//! its priors.

use chrono::{DateTime, Local, Utc};

/// Render the temporal + system environment block in a format LLMs reliably
/// honour. Designed to be the **first** content the LLM sees in the system
/// prompt.
///
/// Layout intent:
/// - ISO-8601 dates so there's zero locale ambiguity
/// - Local + UTC side by side (the LLM may reason in either)
/// - Day-of-week spelled out (helps with relative-date queries like "tomorrow")
/// - Explicit instruction NOT to fall back to training data for "today"
///
/// The block is plain text Markdown so it composes with the rest of the
/// system prompt without escaping rules.
pub fn temporal_context_block() -> String {
    let now_local: DateTime<Local> = Local::now();
    let now_utc: DateTime<Utc> = now_local.with_timezone(&Utc);
    let tz_name = current_timezone_name();
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    let cwd = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "unknown".into());
    let home = crate::paths::home_string().unwrap_or_else(|| "unknown".into());

    format!(
        "## CURRENT ENVIRONMENT (authoritative - overrides any training data cutoff)\n\
         - Date today (ISO 8601): {date_iso} ({weekday})\n\
         - Local time: {local_time} ({tz})\n\
         - UTC time: {utc_time}Z\n\
         - Unix timestamp: {ts}\n\
         - Host OS: {os} ({arch})\n\
         - Home directory: {home}\n\
         - Working directory: {cwd}\n\
         \n\
         For any operation that depends on \"today\", \"now\", \"this week\",\n\
         \"yesterday\", \"in 3 days\", etc., use the values above. Never use\n\
         your training-data cutoff as the reference date.\n",
        date_iso = now_local.format("%Y-%m-%d"),
        weekday = now_local.format("%A"),
        local_time = now_local.format("%H:%M:%S"),
        tz = tz_name,
        utc_time = now_utc.format("%Y-%m-%dT%H:%M:%S"),
        ts = now_local.timestamp(),
        os = os,
        arch = arch,
        home = home,
        cwd = cwd,
    )
}

/// Best-effort IANA timezone name.
///
/// `chrono::Local` does not surface the timezone name directly, so we fall back
/// to the offset string (e.g. `+02:00`) when the IANA name can't be read from
/// the environment. Sufficient for the LLM's needs: the offset disambiguates
/// the wall-clock display, and the IANA name when available is a stronger
/// signal for queries like "what time is it in Paris".
fn current_timezone_name() -> String {
    if let Ok(tz) = std::env::var("TZ") {
        if !tz.is_empty() {
            return tz;
        }
    }
    // macOS: `/etc/localtime` symlinks to `/var/db/timezone/zoneinfo/<IANA>`.
    // Linux: similar layout under `/usr/share/zoneinfo/<IANA>`.
    if let Ok(link) = std::fs::read_link("/etc/localtime") {
        let path = link.to_string_lossy();
        if let Some(idx) = path.find("zoneinfo/") {
            return path[idx + "zoneinfo/".len()..].to_string();
        }
    }
    // Fallback: the UTC offset (`+02:00` style) as captured by chrono::Local.
    Local::now().format("%:z").to_string()
}

/// Prepend the temporal block to an existing system prompt, separating with
/// a blank line. When the existing prompt is empty, returns only the block
/// (no trailing newlines).
pub fn prepend_temporal_context(existing_system_prompt: &str) -> String {
    let block = temporal_context_block();
    if existing_system_prompt.trim().is_empty() {
        block
    } else {
        format!("{block}\n{}", existing_system_prompt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_contains_iso_date() {
        // GIVEN the temporal context block built for the current instant
        let block = temporal_context_block();
        // YYYY-MM-DD: matches any year 20XX, any month, any day.
        // WHEN it is matched against an ISO date pattern
        let re = regex::Regex::new(r"\b20\d{2}-\d{2}-\d{2}\b").unwrap();
        // THEN the block carries a date in that form
        assert!(re.is_match(&block), "block missing ISO date: {block}");
    }

    #[test]
    fn block_contains_override_instruction() {
        // GIVEN the temporal context block
        let block = temporal_context_block();
        // WHEN its wording is inspected
        // THEN it tells the model this date outranks its training cutoff
        assert!(
            block.contains("authoritative") && block.contains("training-data cutoff"),
            "block missing the explicit override instruction"
        );
    }

    #[test]
    fn block_contains_weekday_word() {
        // GIVEN the temporal context block
        let block = temporal_context_block();
        let weekdays = [
            "Monday",
            "Tuesday",
            "Wednesday",
            "Thursday",
            "Friday",
            "Saturday",
            "Sunday",
        ];
        // WHEN it is searched for an English weekday
        let found = weekdays.iter().any(|w| block.contains(w));
        // THEN one is there, so the block states the day of week and not only the date
        assert!(found, "block missing English weekday: {block}");
    }

    #[test]
    fn prepend_keeps_existing_prompt() {
        // GIVEN a system prompt
        // WHEN the temporal context is prepended to it
        let prepended = prepend_temporal_context("You are an assistant.");
        // THEN both are present, and the block comes first
        assert!(prepended.contains("You are an assistant."));
        assert!(prepended.contains("CURRENT ENVIRONMENT"));
        let block_pos = prepended.find("CURRENT ENVIRONMENT").unwrap();
        let prompt_pos = prepended.find("You are an assistant.").unwrap();
        assert!(block_pos < prompt_pos);
    }

    #[test]
    fn prepend_handles_empty_input() {
        // GIVEN an empty prompt
        // WHEN the temporal context is prepended to it
        let prepended = prepend_temporal_context("");
        // THEN the block stands on its own
        assert!(prepended.contains("CURRENT ENVIRONMENT"));
    }

    #[test]
    fn prepend_handles_whitespace_only_input() {
        // GIVEN a prompt made only of whitespace
        // WHEN the temporal context is prepended to it
        let prepended = prepend_temporal_context("   \n  ");
        // THEN the block is there, and the blank prompt is dropped rather than appended
        assert!(prepended.contains("CURRENT ENVIRONMENT"));
        assert!(!prepended.contains("   \n  "));
    }
}
