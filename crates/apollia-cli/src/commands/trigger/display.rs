//! Human-readable rendering of the trigger payloads the runtime returns.

/// Format trigger list as a human-readable table.
///
/// Columns: ID, AGENT, TYPE, ENABLED, FIRES, SKIPS, LAST FIRE
pub(super) fn format_trigger_list(resp: &serde_json::Value) {
    let triggers = resp
        .get("triggers")
        .and_then(|t| t.as_array())
        .cloned()
        .unwrap_or_default();

    println!(
        "  {:<24} {:<20} {:<12} {:<8} {:<6} {:<6} LAST FIRE",
        "ID", "AGENT", "TYPE", "ENABLED", "FIRES", "SKIPS"
    );

    if triggers.is_empty() {
        println!("  (no triggers configured)");
        return;
    }

    for t in &triggers {
        let id = t.get("id").and_then(|v| v.as_str()).unwrap_or("?");
        let agent = t.get("agent").and_then(|v| v.as_str()).unwrap_or("?");
        let kind = t.get("source_kind").and_then(|v| v.as_str()).unwrap_or("?");
        let enabled = if t.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false) {
            "✔"
        } else {
            "✘"
        };
        let fires = t.get("fire_count").and_then(|v| v.as_u64()).unwrap_or(0);
        let skips = t.get("skip_count").and_then(|v| v.as_u64()).unwrap_or(0);
        let last = t
            .get("last_fired")
            .and_then(|v| v.as_str())
            .map(format_relative_time)
            .unwrap_or_else(|| "-".to_string());
        println!(
            "  {:<24} {:<20} {:<12} {:<8} {:<6} {:<6} {}",
            id, agent, kind, enabled, fires, skips, last
        );
    }
}

/// Render an RFC3339 timestamp as a compact relative duration ("3m ago").
///
/// Falls back to the raw string when parsing fails. Used by the trigger
/// list / status outputs to surface "last fired" without dumping a full
/// RFC3339 string into the table.
pub(super) fn format_relative_time(ts: &str) -> String {
    let parsed = chrono::DateTime::parse_from_rfc3339(ts).ok();
    let Some(dt) = parsed else {
        return ts.to_string();
    };
    let secs = chrono::Utc::now()
        .signed_duration_since(dt.with_timezone(&chrono::Utc))
        .num_seconds();
    if secs < 0 {
        return ts.to_string();
    }
    if secs < 60 {
        format!("{secs}s ago")
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86_400 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86_400)
    }
}

/// Format trigger detail as human-readable key-value pairs.
pub(super) fn format_trigger_detail(resp: &serde_json::Value) {
    let id = resp.get("id").and_then(|v| v.as_str()).unwrap_or("?");
    let agent = resp.get("agent").and_then(|v| v.as_str()).unwrap_or("?");
    // The runtime returns `source_type` + structured `source_config` (a JSON
    // object). Older CLI builds looked for `source_kind` / `source_detail`
    // which never existed; fix here so `trigger status` shows the real
    // kind + the kind-specific config slot.
    let kind = resp
        .get("source_type")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    let detail = trigger_detail_from_config(kind, resp.get("source_config"));
    let on_busy = resp.get("on_busy").and_then(|v| v.as_str()).unwrap_or("?");
    let enabled = resp
        .get("enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let fires = resp.get("fire_count").and_then(|v| v.as_u64()).unwrap_or(0);
    let skips = resp.get("skip_count").and_then(|v| v.as_u64()).unwrap_or(0);

    let type_display = if detail.is_empty() {
        kind.to_string()
    } else {
        format!("{kind} ({detail})")
    };

    println!("  Trigger   : {id}");
    println!("  Agent     : {agent}");
    println!("  Type      : {type_display}");
    println!("  On busy   : {on_busy}");
    println!("  Enabled   : {enabled}");
    println!("  Fires     : {fires} total, {skips} skipped");
}

/// Extract the human-readable detail string from a `source_config` JSON
/// object, picking the right field per `source_type`. Webhook intentionally
/// renders as `(secret hidden)` so we never print a shared secret in the
/// status output.
pub(super) fn trigger_detail_from_config(kind: &str, config: Option<&serde_json::Value>) -> String {
    let Some(cfg) = config else {
        return String::new();
    };
    let pick = |k: &str| {
        cfg.get(k)
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .unwrap_or_default()
    };
    match kind {
        "cron" => pick("schedule"),
        "interval" => pick("every"),
        "oneshot" => pick("fire_at"),
        "file_watch" => pick("path"),
        "webhook" => "secret hidden".to_string(),
        _ => String::new(),
    }
}

/// Format trigger logs as human-readable rows.
///
/// Columns: date, status, task_id (or a dash placeholder), reason (or a dash placeholder).
pub(super) fn format_trigger_logs(resp: &serde_json::Value) {
    let entries = resp
        .get("entries")
        .and_then(|e| e.as_array())
        .cloned()
        .unwrap_or_default();

    if entries.is_empty() {
        println!("  (no history)");
        return;
    }

    for entry in &entries {
        let fired_at = entry
            .get("fired_at")
            .and_then(|v| v.as_str())
            .unwrap_or("?");
        // Truncate RFC3339 to "YYYY-MM-DD HH:MM:SS"
        let date_display = if fired_at.len() >= 19 {
            fired_at[..19].replace('T', " ")
        } else {
            fired_at.to_string()
        };
        let status = entry.get("status").and_then(|v| v.as_str()).unwrap_or("?");
        let task_id = entry.get("task_id").and_then(|v| v.as_str()).unwrap_or("-");
        let reason = entry.get("reason").and_then(|v| v.as_str()).unwrap_or("-");
        println!("  {date_display}  {status:<8}  {task_id:<36}  {reason}");
    }
}
