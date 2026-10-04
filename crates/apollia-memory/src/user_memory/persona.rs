//! Rendering of a profile for a system prompt: the persona brief.

use crate::profile_schema::FREE_TEXT_KEY;

use super::{ProfileEntry, UserMemoryError, UserMemoryRepository};

impl UserMemoryRepository {
    /// Produces a structured persona brief for LLM system-prompt injection.
    ///
    /// Reads the profile `designated` names, or the default profile when
    /// nothing is designated ([`Self::profile_or_default`]): a headline (name,
    /// role, function), the preferences, the tools, the free text, then the
    /// free-form entries as context. Empty when that profile holds nothing.
    pub fn recall_persona_brief(
        &self,
        designated: Option<&str>,
        max_entries: usize,
    ) -> Result<String, UserMemoryError> {
        let profile = self.profile_or_default(designated)?;
        let entries = self.list_all_in(&profile)?;
        if entries.is_empty() {
            return Ok(String::new());
        }

        let find = |k: &str| -> Option<String> {
            entries
                .iter()
                .find(|e| e.in_schema && e.key == k)
                .map(|e| e.value.clone())
        };

        let role = find("role");
        let preferences = find("preferences");

        let mut output = String::new();
        Self::push_headline(&mut output, find("name"), role.as_deref(), find("function"));
        if let Some(ref prefs) = preferences {
            output.push_str(&format!("Preferences: {prefs}\n"));
        }
        if let Some(tools) = find("tools") {
            output.push_str(&format!("\nTools: {tools}\n"));
        }
        if let Some(notes) = find(FREE_TEXT_KEY) {
            output.push_str("\nNotes:\n");
            output.push_str(notes.trim_end());
            output.push('\n');
        }

        Self::push_adaptation(&mut output, preferences.as_deref(), role.as_deref());
        Self::push_context(&mut output, &entries, max_entries);

        Ok(output)
    }

    /// Appends the persona headline line (name, role, function).
    fn push_headline(
        output: &mut String,
        name: Option<String>,
        role: Option<&str>,
        function: Option<String>,
    ) {
        let mut parts: Vec<String> = Vec::new();
        if let Some(n) = name {
            parts.push(n);
        }
        if let Some(r) = role {
            if parts.is_empty() {
                parts.push(format!("Role: {r}"));
            } else {
                parts.push(format!("({r})"));
            }
        }
        if let Some(f) = function {
            parts.push(format!("- {f}"));
        }
        if parts.is_empty() {
            return;
        }
        output.push_str(&parts.join(" "));
        output.push('\n');
    }

    /// Appends the "Adaptation" section based on preferences and role.
    fn push_adaptation(output: &mut String, preferences: Option<&str>, role: Option<&str>) {
        output.push_str("\nAdaptation:\n");
        if let Some(prefs) = preferences {
            output.push_str(&format!("- Preferences: {prefs}\n"));
        }
        if let Some(r) = role {
            output.push_str(Self::role_adaptation_line(&r.to_lowercase()));
        }
    }

    /// Returns the adaptation bullet matching a lowercased role string.
    fn role_adaptation_line(r_lower: &str) -> &'static str {
        const TECH: &[&str] = &["dev", "engineer", "data", "devops", "sysadmin"];
        const CREATIVE: &[&str] = &["design", "creat", "vidéo", "video"];
        const BUSINESS: &[&str] = &[
            "manager",
            "market",
            "commercial",
            "sales",
            "rh",
            "ceo",
            "cto",
            "coo",
        ];
        if TECH.iter().any(|k| r_lower.contains(k)) {
            "- Technical profile: appropriate technical vocabulary\n"
        } else if CREATIVE.iter().any(|k| r_lower.contains(k)) {
            "- Creative profile: avoid technical jargon, favour the visuals\n"
        } else if BUSINESS.iter().any(|k| r_lower.contains(k)) {
            "- Business profile: focus on outcomes, KPIs, no needless technical detail\n"
        } else {
            "- Adapt the vocabulary to the user's profile\n"
        }
    }

    /// Appends the "Context" section listing the free-form entries.
    fn push_context(output: &mut String, entries: &[ProfileEntry], max_entries: usize) {
        let remaining: Vec<&ProfileEntry> = entries
            .iter()
            .filter(|e| !e.in_schema)
            .take(max_entries)
            .collect();
        if remaining.is_empty() {
            return;
        }
        output.push_str("\nContext:\n");
        for entry in remaining {
            let stale_marker = if Self::is_stale(&entry.updated_at) {
                " (possibly stale)"
            } else {
                ""
            };
            output.push_str(&format!("- {}: {}{stale_marker}\n", entry.key, entry.value));
        }
    }
}
