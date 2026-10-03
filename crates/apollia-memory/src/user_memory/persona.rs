//! Renderings of the default profile for a system prompt: the sectioned block
//! and the persona brief.

use crate::profile_schema::{field_for, ProfileSection, FREE_TEXT_KEY};

use super::{ProfileEntry, UserMemoryError, UserMemoryRepository};

impl UserMemoryRepository {
    /// Produces a text block suitable for LLM system-prompt injection.
    ///
    /// Output groups the fields of the default profile by schema section, then
    /// the free-form entries under `other`:
    /// ```text
    /// Section: identity
    /// - name: Nidal
    /// - role: CTO
    /// Section: preferences
    /// - preferences: fr
    /// ```
    pub fn recall_all_for_injection(&self, max_entries: usize) -> Result<String, UserMemoryError> {
        let entries = self.list_all()?;
        let mut output = String::new();
        let mut total = 0usize;

        let sections = [
            ProfileSection::Identity,
            ProfileSection::Work,
            ProfileSection::Preferences,
            ProfileSection::Notes,
        ];

        for section in sections {
            let in_section: Vec<&ProfileEntry> = entries
                .iter()
                .filter(|e| field_for(&e.key).map(|f| f.section) == Some(section))
                .collect();
            Self::append_injection_section(
                &mut output,
                section.tag(),
                &in_section,
                &mut total,
                max_entries,
            );
            if total >= max_entries {
                break;
            }
        }

        let extras: Vec<&ProfileEntry> = entries.iter().filter(|e| !e.in_schema).collect();
        if total < max_entries {
            Self::append_injection_section(&mut output, "other", &extras, &mut total, max_entries);
        }

        Ok(output)
    }

    /// Appends a `Section: <tag>` block for `group` to `output`, respecting the
    /// `max_entries` budget tracked by `total`.  No-op when `group` is empty.
    fn append_injection_section(
        output: &mut String,
        tag: &str,
        group: &[&ProfileEntry],
        total: &mut usize,
        max_entries: usize,
    ) {
        if group.is_empty() {
            return;
        }
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str(&format!("Section: {tag}\n"));
        for entry in group {
            if *total >= max_entries {
                break;
            }
            output.push_str(&format!("- {}: {}\n", entry.key, entry.value));
            *total += 1;
        }
    }

    /// Produces a structured persona brief for LLM system-prompt injection.
    ///
    /// Reads the fields of the default profile: a headline (name, role,
    /// function), the preferences, the tools, the free text, then the
    /// free-form entries as context.
    pub fn recall_persona_brief(&self, max_entries: usize) -> Result<String, UserMemoryError> {
        let entries = self.list_all()?;
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
