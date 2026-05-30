//! Canonical user profile schema.
//!
//! Declares the known profile fields shared across all agents: keys, display
//! sections, sensitivity flag, field type, and i18n labels (FR/EN).  The schema
//! is the single source of truth, consumed by the Rust repository for
//! schema/extras partitioning, by the Tauri IPC layer for UI rendering, and by
//! the Python SDK to expose typed `ctx.profile.<field>` accessors.
//!
//! Keys are stored flat in the `__user__` SemanticMemory namespace (no
//! `category.` or `user.` prefix).  Adding a new canonical field requires
//! appending to [`PROFILE_SCHEMA`].  Free-form keys written by agents outside
//! this schema are tolerated and surface in the "Other entries" UI section.

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// UI section a profile field belongs to.
///
/// Sections drive layout grouping on the Settings → Profile page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileSection {
    /// Identity: name, role, goals.
    Identity,
    /// Work context: sector, team size, tech stack, daily tools, integrations.
    Work,
    /// Preferences: language, LLM, agent supervision, agent domains, triggers.
    Preferences,
    /// Constraints: sovereignty, compliance.
    Constraints,
}

impl ProfileSection {
    /// String tag for serialization to UI / SDK.
    pub fn tag(&self) -> &'static str {
        match self {
            Self::Identity => "identity",
            Self::Work => "work",
            Self::Preferences => "preferences",
            Self::Constraints => "constraints",
        }
    }
}

/// Input control type the UI should render for a profile field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileFieldType {
    /// Single-line free text.
    Text,
    /// Multi-line free text.
    LongText,
    /// Closed set of allowed values (see [`ProfileField::options`]).
    Select,
}

/// Declarative description of a canonical profile field.
#[derive(Debug, Clone, Copy)]
pub struct ProfileField {
    /// Storage key (flat, e.g. `name`, `agents.hitl`, `constraints.sovereignty`).
    pub key: &'static str,
    /// French label for UI rendering.
    pub label_fr: &'static str,
    /// English label for UI rendering.
    pub label_en: &'static str,
    /// French inline help / placeholder.
    pub help_fr: &'static str,
    /// English inline help / placeholder.
    pub help_en: &'static str,
    /// UI section the field belongs to.
    pub section: ProfileSection,
    /// `true` when changing the value warrants an "rerun onboarding" notice
    /// because it affects agent permissions / governance.
    pub sensitive: bool,
    /// Input control to render.
    pub field_type: ProfileFieldType,
    /// Allowed values when [`Self::field_type`] is
    /// [`ProfileFieldType::Select`], empty otherwise.
    pub options: &'static [&'static str],
}

// ---------------------------------------------------------------------------
// Canonical schema (~15 fields, Tier 1 + Tier 2)
// ---------------------------------------------------------------------------

/// The canonical set of profile fields, ordered for stable UI rendering.
///
/// Tier 1 (`name`, `role`, `agents.hitl`, `constraints.sovereignty`) is what
/// the onboarding agent writes by default.  Tier 2 fields surface as empty
/// inputs in the UI until populated by the user or by an agent observation.
pub const PROFILE_SCHEMA: &[ProfileField] = &[
    // ─── Identity ────────────────────────────────────────────────────────
    ProfileField {
        key: "name",
        label_fr: "Prénom",
        label_en: "First name",
        help_fr: "Comment les agents doivent vous appeler.",
        help_en: "How agents should address you.",
        section: ProfileSection::Identity,
        sensitive: false,
        field_type: ProfileFieldType::Text,
        options: &[],
    },
    ProfileField {
        key: "role",
        label_fr: "Rôle",
        label_en: "Role",
        help_fr: "Votre métier ou fonction principale.",
        help_en: "Your main role or job title.",
        section: ProfileSection::Identity,
        sensitive: false,
        field_type: ProfileFieldType::Text,
        options: &[],
    },
    ProfileField {
        key: "goals",
        label_fr: "Objectifs",
        label_en: "Goals",
        help_fr: "Ce que vous cherchez à accomplir avec Apollia.",
        help_en: "What you want to accomplish with Apollia.",
        section: ProfileSection::Identity,
        sensitive: false,
        field_type: ProfileFieldType::LongText,
        options: &[],
    },
    // ─── Work ────────────────────────────────────────────────────────────
    ProfileField {
        key: "domain.sector",
        label_fr: "Secteur d'activité",
        label_en: "Industry",
        help_fr: "ex : fintech, santé, e-commerce, R&D.",
        help_en: "e.g. fintech, healthcare, e-commerce, R&D.",
        section: ProfileSection::Work,
        sensitive: false,
        field_type: ProfileFieldType::Text,
        options: &[],
    },
    ProfileField {
        key: "domain.team_size",
        label_fr: "Taille d'équipe",
        label_en: "Team size",
        help_fr: "Solo, petite équipe, grande structure...",
        help_en: "Solo, small team, large organization...",
        section: ProfileSection::Work,
        sensitive: false,
        field_type: ProfileFieldType::Text,
        options: &[],
    },
    ProfileField {
        key: "tech.stack",
        label_fr: "Stack technique",
        label_en: "Tech stack",
        help_fr: "Langages, frameworks et outils principaux.",
        help_en: "Main languages, frameworks and tools.",
        section: ProfileSection::Work,
        sensitive: false,
        field_type: ProfileFieldType::LongText,
        options: &[],
    },
    ProfileField {
        key: "tech.proficiency",
        label_fr: "Niveau technique",
        label_en: "Tech proficiency",
        help_fr: "Profondeur souhaitée des explications techniques.",
        help_en: "Depth of technical explanations you expect.",
        section: ProfileSection::Work,
        sensitive: false,
        field_type: ProfileFieldType::Select,
        options: &["debutant", "intermediaire", "avance", "expert"],
    },
    ProfileField {
        key: "tools.daily",
        label_fr: "Outils quotidiens",
        label_en: "Daily tools",
        help_fr: "Outils que vous utilisez tous les jours (Git, Notion, Slack...).",
        help_en: "Tools you use daily (Git, Notion, Slack...).",
        section: ProfileSection::Work,
        sensitive: false,
        field_type: ProfileFieldType::LongText,
        options: &[],
    },
    ProfileField {
        key: "tech.integrations",
        label_fr: "Intégrations connectées",
        label_en: "Connected integrations",
        help_fr: "Services connectés (GitHub, GitLab, Gmail...). Sensible : modifier ce champ peut nécessiter un nouvel onboarding.",
        help_en: "Connected services (GitHub, GitLab, Gmail...). Sensitive: editing this may require re-running onboarding.",
        section: ProfileSection::Work,
        sensitive: true,
        field_type: ProfileFieldType::LongText,
        options: &[],
    },
    // ─── Preferences ─────────────────────────────────────────────────────
    ProfileField {
        key: "preferences.language",
        label_fr: "Langue préférée",
        label_en: "Preferred language",
        help_fr: "Langue dans laquelle les agents s'adressent à vous.",
        help_en: "Language agents should use when addressing you.",
        section: ProfileSection::Preferences,
        sensitive: false,
        field_type: ProfileFieldType::Select,
        options: &["fr", "en"],
    },
    ProfileField {
        key: "preferences.llm",
        label_fr: "Modèle LLM préféré",
        label_en: "Preferred LLM",
        help_fr: "Modèle à privilégier quand plusieurs sont disponibles.",
        help_en: "Which model to favor when several are available.",
        section: ProfileSection::Preferences,
        sensitive: false,
        field_type: ProfileFieldType::Text,
        options: &[],
    },
    ProfileField {
        key: "agents.hitl",
        label_fr: "Supervision des agents",
        label_en: "Agent supervision",
        help_fr: "Quand devons-nous vous demander confirmation. Sensible : impacte directement les règles de permissions.",
        help_en: "When should we ask for your confirmation. Sensitive: directly impacts permission rules.",
        section: ProfileSection::Preferences,
        sensitive: true,
        field_type: ProfileFieldType::Select,
        options: &["always", "critical-only", "never"],
    },
    ProfileField {
        key: "agents.domains",
        label_fr: "Domaines des agents",
        label_en: "Agent domains",
        help_fr: "Domaines fonctionnels couverts (veille, code review, RSE...).",
        help_en: "Functional domains covered (research, code review, ESG...).",
        section: ProfileSection::Preferences,
        sensitive: false,
        field_type: ProfileFieldType::LongText,
        options: &[],
    },
    ProfileField {
        key: "agents.trigger",
        label_fr: "Déclenchement des agents",
        label_en: "Agent triggering",
        help_fr: "Comment et quand les agents doivent s'activer (manuel, cron, événement).",
        help_en: "How and when agents should run (manual, cron, event).",
        section: ProfileSection::Preferences,
        sensitive: false,
        field_type: ProfileFieldType::Text,
        options: &[],
    },
    // ─── Constraints ─────────────────────────────────────────────────────
    ProfileField {
        key: "constraints.sovereignty",
        label_fr: "Souveraineté des données",
        label_en: "Data sovereignty",
        help_fr: "Niveau d'autonomie locale exigé. Sensible : impacte les règles réseau et LLM.",
        help_en: "Required level of local autonomy. Sensitive: impacts network and LLM rules.",
        section: ProfileSection::Constraints,
        sensitive: true,
        field_type: ProfileFieldType::Select,
        options: &["local-only", "local-preferred", "cloud-ok"],
    },
    ProfileField {
        key: "constraints.compliance",
        label_fr: "Exigences de conformité",
        label_en: "Compliance requirements",
        help_fr: "Cadres réglementaires applicables (RGPD, HDS, SOC2...). Sensible.",
        help_en: "Regulatory frameworks (GDPR, HIPAA, SOC2...). Sensitive.",
        section: ProfileSection::Constraints,
        sensitive: true,
        field_type: ProfileFieldType::Text,
        options: &[],
    },
];

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Looks up a canonical field by its storage key.
pub fn field_for(key: &str) -> Option<&'static ProfileField> {
    PROFILE_SCHEMA.iter().find(|f| f.key == key)
}

/// Returns `true` if the key matches a canonical schema entry.
pub fn is_canonical(key: &str) -> bool {
    field_for(key).is_some()
}

/// Returns the set of all canonical keys, in schema order.
pub fn canonical_keys() -> Vec<&'static str> {
    PROFILE_SCHEMA.iter().map(|f| f.key).collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn schema_keys_are_unique() {
        let mut seen = HashSet::new();
        for f in PROFILE_SCHEMA {
            assert!(seen.insert(f.key), "duplicate key in schema: {}", f.key);
        }
    }

    #[test]
    fn schema_select_fields_have_options() {
        for f in PROFILE_SCHEMA {
            if f.field_type == ProfileFieldType::Select {
                assert!(
                    !f.options.is_empty(),
                    "select field {} must have options",
                    f.key
                );
            } else {
                assert!(
                    f.options.is_empty(),
                    "non-select field {} must not have options",
                    f.key
                );
            }
        }
    }

    #[test]
    fn schema_includes_tier_1_keys() {
        let keys: HashSet<&str> = canonical_keys().into_iter().collect();
        for required in &["name", "role", "agents.hitl", "constraints.sovereignty"] {
            assert!(keys.contains(required), "tier 1 key missing: {}", required);
        }
    }

    #[test]
    fn is_canonical_matches() {
        assert!(is_canonical("name"));
        assert!(is_canonical("agents.hitl"));
        assert!(!is_canonical("random_key"));
    }
}
