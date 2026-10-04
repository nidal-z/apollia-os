//! Canonical user profile schema.
//!
//! Declares the fields a profile carries: five simple fields and one free
//! text, all optional. The schema is the single source of truth, consumed by
//! the Rust repository to tell a profile field from a free-form entry, by the
//! interface layers to draw the profile form ([`profile_fields`]), and by the
//! Python SDK to expose `ctx.profile` keys.
//!
//! A profile is a row of the `profiles` table of `user_memory.db`; each key
//! below names one of its columns. Free-form keys written by agents outside
//! this schema are tolerated and stay in the `__user__` semantic namespace.

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Section a profile field belongs to, for grouping in a form or a prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileSection {
    /// Who the person is: name, role, function.
    Identity,
    /// How the person works: the tools they use.
    Work,
    /// What the person prefers.
    Preferences,
    /// The free text.
    Notes,
}

impl ProfileSection {
    /// String tag for serialization to UI / SDK.
    pub fn tag(&self) -> &'static str {
        match self {
            Self::Identity => "identity",
            Self::Work => "work",
            Self::Preferences => "preferences",
            Self::Notes => "notes",
        }
    }
}

/// Input control a form should render for a profile field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileFieldType {
    /// Single-line free text.
    Text,
    /// Multi-line free text.
    LongText,
}

/// Declarative description of a profile field. Every field is optional.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct ProfileField {
    /// Storage key (`name`, `role`, `function`, `tools`, `preferences`,
    /// `free_text`).
    pub key: &'static str,
    /// French label for UI rendering.
    pub label_fr: &'static str,
    /// English label for UI rendering.
    pub label_en: &'static str,
    /// French inline help / placeholder.
    pub help_fr: &'static str,
    /// English inline help / placeholder.
    pub help_en: &'static str,
    /// Section the field belongs to.
    pub section: ProfileSection,
    /// Input control to render.
    pub field_type: ProfileFieldType,
}

// ---------------------------------------------------------------------------
// Canonical schema: five fields and a free text
// ---------------------------------------------------------------------------

/// Key of the free text of a profile.
pub const FREE_TEXT_KEY: &str = "free_text";

/// The profile fields, ordered for stable form rendering.
pub const PROFILE_SCHEMA: &[ProfileField] = &[
    ProfileField {
        key: "name",
        label_fr: "Nom",
        label_en: "Name",
        help_fr: "Comment les agents doivent vous appeler.",
        help_en: "How agents should address you.",
        section: ProfileSection::Identity,
        field_type: ProfileFieldType::Text,
    },
    ProfileField {
        key: "role",
        label_fr: "Rôle",
        label_en: "Role",
        help_fr: "Votre métier ou votre poste.",
        help_en: "Your job or position.",
        section: ProfileSection::Identity,
        field_type: ProfileFieldType::Text,
    },
    ProfileField {
        key: "function",
        label_fr: "Fonction",
        label_en: "Function",
        help_fr: "Le service ou le domaine dans lequel vous travaillez.",
        help_en: "The department or field you work in.",
        section: ProfileSection::Identity,
        field_type: ProfileFieldType::Text,
    },
    ProfileField {
        key: "tools",
        label_fr: "Outils utilisés",
        label_en: "Tools you use",
        help_fr: "Les outils de votre quotidien (tableur, messagerie, Notion...).",
        help_en: "The tools of your day (spreadsheet, email, Notion...).",
        section: ProfileSection::Work,
        field_type: ProfileFieldType::LongText,
    },
    ProfileField {
        key: "preferences",
        label_fr: "Préférences",
        label_en: "Preferences",
        help_fr: "Langue, ton, format de réponse que vous préférez.",
        help_en: "The language, tone and answer format you prefer.",
        section: ProfileSection::Preferences,
        field_type: ProfileFieldType::LongText,
    },
    ProfileField {
        key: FREE_TEXT_KEY,
        label_fr: "Ce que vos agents doivent savoir",
        label_en: "What your agents should know",
        help_fr: "Tout ce que vous voulez que les agents sachent de vous.",
        help_en: "Anything you want agents to know about you.",
        section: ProfileSection::Notes,
        field_type: ProfileFieldType::LongText,
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

/// Returns the profile fields a form draws, in display order.
pub fn profile_fields() -> &'static [ProfileField] {
    PROFILE_SCHEMA
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
        // GIVEN the declared profile schema
        let mut seen = HashSet::new();
        // WHEN its keys are collected one by one
        for f in PROFILE_SCHEMA {
            // THEN no key appears twice
            assert!(seen.insert(f.key), "duplicate key in schema: {}", f.key);
        }
    }

    #[test]
    fn profile_fields_are_the_five_fields_and_the_free_text() {
        // GIVEN the schema a form reads
        // WHEN the public field list is asked for
        let keys: Vec<&str> = profile_fields().iter().map(|f| f.key).collect();
        // THEN it names the five fields then the free text, nothing else
        assert_eq!(
            keys,
            vec![
                "name",
                "role",
                "function",
                "tools",
                "preferences",
                "free_text"
            ]
        );
    }

    #[test]
    fn profile_fields_serialize_for_a_form() {
        // GIVEN the field list
        // WHEN it is serialized the way an interface layer sends it
        let json = serde_json::to_value(profile_fields()).unwrap();
        // THEN each field carries its key, labels, section and control
        assert_eq!(json[0]["key"], "name");
        assert_eq!(json[0]["section"], "identity");
        assert_eq!(json[5]["field_type"], "long_text");
        assert_eq!(json[5]["section"], "notes");
    }

    #[test]
    fn is_canonical_matches() {
        // GIVEN two schema keys and one that is not in the schema
        // WHEN each is tested for canonicity
        // THEN the two schema keys are canonical and the third is not
        assert!(is_canonical("name"));
        assert!(is_canonical("free_text"));
        assert!(!is_canonical("random_key"));
    }
}
