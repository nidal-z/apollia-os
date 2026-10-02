//! `apollia-os policy show`: the policy the runtime reads at startup.
//!
//! The folder and the rights required are those of the runtime
//! (`policy_location`), and the verdict is the one `load_policy` gives it, so
//! what is shown is what a runtime started now would apply.

use apollia_permissions::load_policy;
use apollia_permissions::org_policy::{
    policy_location, DenyOnly, ExportDetail, OrgPolicy, PolicyVerdict,
};

use crate::exit_codes;

pub(super) fn run(json: bool) -> i32 {
    let location = policy_location();
    let verdict = load_policy(&location.dir, location.owner);
    let dir = location.dir.display().to_string();
    if json {
        let doc = match &verdict {
            PolicyVerdict::Absent => serde_json::json!({"state": "none", "dir": dir}),
            PolicyVerdict::Invalid(invalid) => serde_json::json!({
                "state": "invalid",
                "dir": dir,
                "reason": invalid.reason.as_str(),
                "detail": invalid.detail,
            }),
            PolicyVerdict::Valid(valid) => serde_json::json!({
                "state": "active",
                "dir": dir,
                "key_fingerprint": valid.key_fingerprint,
                "publisher": valid.publisher,
                "signed_at": valid.signed_at.to_string(),
                "policy": policy_json(&valid.policy),
            }),
        };
        println!("{}", serde_json::to_string_pretty(&doc).unwrap_or_default());
        return exit_codes::SUCCESS;
    }
    match &verdict {
        PolicyVerdict::Absent => {
            println!("none");
            crate::note!("  No organisation policy in {dir}.");
        }
        PolicyVerdict::Invalid(invalid) => {
            println!("invalid ({})", invalid.reason);
            crate::note!("  {}", invalid.detail);
            crate::note!(
                "  The runtime refuses sensitive actions while the policy cannot be read."
            );
        }
        PolicyVerdict::Valid(valid) => {
            println!("active");
            crate::note!("  folder     {dir}");
            crate::note!("  key        {}", valid.key_fingerprint);
            crate::note!("  signed by  {} on {}", valid.publisher, valid.signed_at);
            let rules = rule_lines(&valid.policy);
            if rules.is_empty() {
                println!("No rule imposed.");
            }
            for line in rules {
                println!("{line}");
            }
        }
    }
    exit_codes::SUCCESS
}

/// The policy as JSON, with `signed_at` as its TOML text.
fn policy_json(policy: &OrgPolicy) -> serde_json::Value {
    let mut value = serde_json::to_value(policy).unwrap_or(serde_json::Value::Null);
    if let Some(object) = value.as_object_mut() {
        object.insert(
            "signed_at".to_owned(),
            serde_json::Value::String(policy.signed_at.to_string()),
        );
    }
    value
}

fn list(items: &[String]) -> String {
    items.join(", ")
}

/// The rules of `policy`, one plain sentence each; a rule that imposes
/// nothing yields no line.
pub(super) fn rule_lines(policy: &OrgPolicy) -> Vec<String> {
    let mut lines = Vec::new();
    let c = &policy.connections;
    if !c.allow.is_empty() {
        lines.push(format!(
            "Connections allowed, all others refused: {}",
            list(&c.allow)
        ));
    }
    if !c.deny.is_empty() {
        lines.push(format!("Connections refused: {}", list(&c.deny)));
    }
    if c.remote == Some(DenyOnly::Deny) {
        lines.push("Remote servers refused.".to_owned());
    }
    if !policy.families.deny.is_empty() {
        let names: Vec<String> = policy
            .families
            .deny
            .iter()
            .map(|f| {
                serde_json::to_value(f)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_owned))
                    .unwrap_or_default()
            })
            .collect();
        lines.push(format!("Action families refused: {}", list(&names)));
    }
    let i = &policy.inference;
    if !i.allowed_servers.is_empty() {
        lines.push(format!(
            "Inference servers allowed, all others refused: {}",
            list(&i.allowed_servers)
        ));
    }
    if i.remote_endpoints == Some(DenyOnly::Deny) {
        lines.push("Cloud inference endpoints refused.".to_owned());
    }
    if i.require_model_hash {
        lines.push("Models without a known digest refused.".to_owned());
    }
    for (key, value) in &policy.settings {
        lines.push(format!("Setting fixed: {key} = {value}"));
    }
    let p = &policy.packages;
    if p.require_signature {
        lines.push("Agent packages must be signed.".to_owned());
    }
    if !p.trusted_publishers.is_empty() {
        lines.push(format!(
            "Trusted publishers: {}",
            list(&p.trusted_publishers)
        ));
    }
    let j = &policy.journal;
    if j.required {
        lines.push("The journal is required for sensitive actions.".to_owned());
    }
    if let Some(days) = j.retention_days {
        lines.push(format!("Journal entries kept {days} days."));
    }
    match j.export.detail {
        Some(ExportDetail::Forbid) => {
            lines.push("Journal exports never carry the detail.".to_owned())
        }
        Some(ExportDetail::Require) => {
            lines.push("Journal exports always carry the detail.".to_owned())
        }
        None => {}
    }
    if let Some(dir) = &j.export.daily_summary_dir {
        lines.push(format!("Daily journal summary exported to {dir}."));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rule_lines_of_an_empty_policy_are_none() {
        // GIVEN a policy that imposes nothing
        let policy = apollia_permissions::parse_policy(
            "schema_version = 1\nsigned_at = 2026-10-02T09:30:00Z\n",
        )
        .unwrap();
        // WHEN its rules are rendered
        let lines = rule_lines(&policy);
        // THEN there is no line
        assert!(lines.is_empty());
    }

    #[test]
    fn test_rule_lines_name_each_rule_in_plain_words() {
        // GIVEN a policy refusing remote servers and a family, and fixing a setting
        let policy = apollia_permissions::parse_policy(
            "schema_version = 1\nsigned_at = 2026-10-02T09:30:00Z\n\
             [connections]\nremote = \"deny\"\n[families]\ndeny = [\"execute_code\"]\n\
             [settings]\n\"chat.max_steps\" = 20\n",
        )
        .unwrap();
        // WHEN its rules are rendered
        let lines = rule_lines(&policy);
        // THEN each rule is one sentence
        assert_eq!(
            lines,
            vec![
                "Remote servers refused.".to_owned(),
                "Action families refused: execute_code".to_owned(),
                "Setting fixed: chat.max_steps = 20".to_owned(),
            ]
        );
    }

    #[test]
    fn test_policy_json_carries_signed_at_as_text() {
        // GIVEN a parsed policy
        let policy = apollia_permissions::parse_policy(
            "schema_version = 1\nsigned_at = 2026-10-02T09:30:00Z\n",
        )
        .unwrap();
        // WHEN it is turned into JSON
        let value = policy_json(&policy);
        // THEN signed_at is its TOML text and the sections are present
        assert_eq!(value["signed_at"], "2026-10-02T09:30:00Z");
        assert_eq!(value["schema_version"], 1);
        assert!(value["journal"].is_object());
    }
}
