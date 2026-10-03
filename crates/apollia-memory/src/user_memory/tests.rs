use super::*;
use crate::profile_schema::FREE_TEXT_KEY;

fn setup() -> (UserMemoryRepository, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("apollia_user_mem_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("user_memory.db");
    let repo = UserMemoryRepository::new(&path).unwrap();
    (repo, path)
}

#[test]
fn set_and_get_roundtrip() {
    // GIVEN an empty user-memory repository
    let (repo, _) = setup();
    // WHEN a schema key is written, then read back
    repo.set("name", "Nidal", WrittenBy::Onboarding).unwrap();
    let entry = repo.get("name").unwrap().expect("entry should exist");
    // THEN the value lands in the default profile, as a schema field owned by the user
    assert_eq!(entry.value, "Nidal");
    assert_eq!(entry.written_by, WrittenBy::User);
    assert!(entry.in_schema);
    assert_eq!(
        repo.default_profile().unwrap().person_name.as_deref(),
        Some("Nidal")
    );
}

#[test]
fn list_schema_and_extras_are_partitioned() {
    // GIVEN one schema key and one key outside the schema
    let (repo, _) = setup();
    repo.set("name", "Nidal", WrittenBy::Onboarding).unwrap();
    repo.set("favorite_color", "blue", WrittenBy::User).unwrap();

    // WHEN the two listings are read
    let schema = repo.list_schema().unwrap();
    let extras = repo.list_extras().unwrap();
    // THEN each key appears in exactly one of them
    assert!(schema.iter().any(|e| e.key == "name"));
    assert!(!schema.iter().any(|e| e.key == "favorite_color"));
    assert!(extras.iter().any(|e| e.key == "favorite_color"));
    assert!(!extras.iter().any(|e| e.key == "name"));
}

#[test]
fn set_rejects_internal_keys() {
    // GIVEN an empty user-memory repository
    let (repo, _) = setup();
    // WHEN a key reserved for internal state is written
    let err = repo
        .set("__sneaky", "x", WrittenBy::User)
        .expect_err("internal keys must be rejected");
    // THEN the write is refused as an invalid key
    assert!(matches!(err, UserMemoryError::InvalidKey(_)));
}

#[test]
fn reset_preserves_internal_state() {
    // GIVEN one visible entry, one covered topic and the skip marker
    let (repo, _) = setup();
    repo.set("name", "Nidal", WrittenBy::User).unwrap();
    repo.mark_topic_covered("identity").unwrap();
    repo.set_onboarding_skipped(true).unwrap();

    // WHEN the repository is reset
    let removed = repo.reset().unwrap();
    // THEN the visible entry alone is dropped, and the internal state stands
    assert_eq!(removed, 1, "only the user-visible entry should be removed");
    assert!(repo.get("name").unwrap().is_none());
    assert!(repo.get_onboarding_skipped().unwrap());
    assert_eq!(repo.get_covered_topics().unwrap(), vec!["identity"]);
}

#[test]
fn list_orders_schema_then_extras() {
    // GIVEN two schema keys and two keys outside it, written out of order
    let (repo, _) = setup();
    repo.set("zzz_extra", "x", WrittenBy::User).unwrap();
    repo.set("role", "CTO", WrittenBy::User).unwrap();
    repo.set("aaa_extra", "y", WrittenBy::User).unwrap();
    repo.set("name", "Nidal", WrittenBy::Onboarding).unwrap();

    // WHEN everything is listed
    let all = repo.list_all().unwrap();
    // THEN the schema keys come first, and the extras follow in alphabetical order
    assert_eq!(all[0].key, "name");
    assert_eq!(all[1].key, "role");
    let extras_keys: Vec<&str> = all
        .iter()
        .filter(|e| !e.in_schema)
        .map(|e| e.key.as_str())
        .collect();
    assert_eq!(extras_keys, vec!["aaa_extra", "zzz_extra"]);
}

#[test]
fn forget_returns_not_found_when_missing() {
    // GIVEN an empty user-memory repository
    let (repo, _) = setup();
    // WHEN a key that was never written is forgotten
    let err = repo.forget("nonexistent").expect_err("must fail");
    // THEN the call reports it as not found, rather than succeeding silently
    assert!(matches!(err, UserMemoryError::NotFound(_)));
}

#[test]
fn forget_removes_entry() {
    // GIVEN a repository holding one entry
    let (repo, _) = setup();
    repo.set("name", "Nidal", WrittenBy::User).unwrap();
    // WHEN that key is forgotten
    repo.forget("name").unwrap();
    // THEN the entry is gone
    assert!(repo.get("name").unwrap().is_none());
}

#[test]
fn update_preserves_provenance() {
    // GIVEN a free-form entry written by an agent
    let (repo, _) = setup();
    let agent = WrittenBy::Agent("veille-ia".to_owned());
    repo.set("favorite_editor", "vim", agent.clone()).unwrap();
    // WHEN its value is updated
    repo.update("favorite_editor", "helix").unwrap();
    let entry = repo.get("favorite_editor").unwrap().unwrap();
    // THEN the new value is stored and the original provenance is kept
    assert_eq!(entry.value, "helix");
    assert_eq!(entry.written_by, agent);
}

#[test]
fn recall_persona_brief_reflects_the_profile_fields() {
    // GIVEN the five fields, the free text and one free-form entry, all written
    let (repo, _) = setup();
    repo.set("name", "Nidal", WrittenBy::User).unwrap();
    repo.set("role", "CTO", WrittenBy::User).unwrap();
    repo.set("function", "fintech", WrittenBy::User).unwrap();
    repo.set("tools", "Notion, tableur", WrittenBy::User)
        .unwrap();
    repo.set("preferences", "short answers", WrittenBy::User)
        .unwrap();
    repo.set(FREE_TEXT_KEY, "tech.proficiency: expert", WrittenBy::User)
        .unwrap();
    repo.set(
        "timezone",
        "Europe/Paris",
        WrittenBy::Agent("planner".to_owned()),
    )
    .unwrap();

    // WHEN the persona brief is recalled
    let brief = repo.recall_persona_brief(20).unwrap();
    // THEN each value is rendered into it
    assert!(brief.contains("Nidal (CTO) - fintech"), "{brief}");
    assert!(brief.contains("Preferences: short answers"), "{brief}");
    assert!(brief.contains("Tools: Notion, tableur"), "{brief}");
    assert!(
        brief.contains("Notes:\ntech.proficiency: expert"),
        "{brief}"
    );
    assert!(brief.contains("- timezone: Europe/Paris"), "{brief}");
}

#[test]
fn is_empty_on_fresh_and_after_set() {
    // GIVEN a fresh repository
    let (repo, _) = setup();
    // WHEN emptiness is asked, then the skip marker set, then a visible entry written
    // THEN only the visible entry makes it non-empty
    assert!(repo.is_empty().unwrap());
    repo.set_onboarding_skipped(true).unwrap();
    assert!(
        repo.is_empty().unwrap(),
        "internal markers should not count as 'visible' entries"
    );
    repo.set("name", "Nidal", WrittenBy::User).unwrap();
    assert!(!repo.is_empty().unwrap());
}

#[test]
fn search_returns_visible_entries() {
    // GIVEN two entries, one of them holding the searched term
    let (repo, _) = setup();
    repo.set("name", "Nidal", WrittenBy::Onboarding).unwrap();
    repo.set("preferences", "francais", WrittenBy::User)
        .unwrap();
    repo.set("favorite_language", "francais too", WrittenBy::User)
        .unwrap();
    // WHEN that term is searched for
    let results = repo.search("francais", 10).unwrap();
    // THEN the profile field and the free-form entry holding it come back
    assert!(results.iter().any(|e| e.key == "preferences"));
    assert!(results.iter().any(|e| e.key == "favorite_language"));
}

#[test]
fn onboarding_topics_round_trip() {
    // GIVEN a fresh repository
    let (repo, _) = setup();
    // WHEN two onboarding topics are marked covered, then read back
    repo.mark_topic_covered("identity").unwrap();
    repo.mark_topic_covered("preferences").unwrap();
    let topics = repo.get_covered_topics().unwrap();
    // THEN both come back
    assert_eq!(topics.len(), 2);
    assert!(topics.contains(&"identity".to_string()));
    assert!(topics.contains(&"preferences".to_string()));
}

#[test]
fn onboarding_session_round_trip() {
    // GIVEN a fresh repository, carrying no onboarding session
    let (repo, _) = setup();
    assert!(repo.get_last_onboarding_session().unwrap().is_none());
    // WHEN a session timestamp is stored, then read back
    repo.set_last_onboarding_session("2026-05-11T10:00:00Z")
        .unwrap();
    // THEN the stored timestamp comes back unchanged
    assert_eq!(
        repo.get_last_onboarding_session().unwrap().as_deref(),
        Some("2026-05-11T10:00:00Z")
    );
}

#[test]
fn written_by_tag_round_trip() {
    // GIVEN the three provenance kinds an entry can carry
    // WHEN each is rendered as its stored tag
    // THEN the tags are the persisted forms, the agent one carrying its name
    assert_eq!(WrittenBy::Onboarding.tag(), "onboarding");
    assert_eq!(WrittenBy::User.tag(), "user");
    assert_eq!(
        WrittenBy::Agent("veille-ia".to_owned()).tag(),
        "agent:veille-ia"
    );
    assert_eq!(WrittenBy::from_tag("onboarding"), WrittenBy::Onboarding);
    assert_eq!(WrittenBy::from_tag("user"), WrittenBy::User);
    assert_eq!(
        WrittenBy::from_tag("agent:veille-ia"),
        WrittenBy::Agent("veille-ia".to_owned())
    );
}

#[test]
fn marking_a_second_profile_default_clears_the_first() {
    // GIVEN the default profile and a second one
    let (repo, _) = setup();
    let first = repo.default_profile().unwrap();
    let second = repo.create_profile("Travail").unwrap();
    assert!(!second.is_default);

    // WHEN the second is marked default
    repo.set_default_profile(&second.id).unwrap();

    // THEN it is the only default, and the first no longer is
    let profiles = repo.list_profiles().unwrap();
    let defaults: Vec<&str> = profiles
        .iter()
        .filter(|p| p.is_default)
        .map(|p| p.id.as_str())
        .collect();
    assert_eq!(defaults, vec![second.id.as_str()]);
    assert!(profiles.iter().any(|p| p.id == first.id && !p.is_default));
    assert_eq!(repo.default_profile().unwrap().id, second.id);
}

#[test]
fn the_database_refuses_a_second_default() {
    // GIVEN a database whose default profile exists
    let (_repo, path) = setup();
    let conn = rusqlite::Connection::open(&path).unwrap();

    // WHEN a second row marked default is inserted behind the repository
    let err = conn.execute(
        "INSERT INTO profiles (id, name, is_default, created_at, updated_at)
         VALUES ('other', 'Other', 1, '2026-10-03T00:00:00Z', '2026-10-03T00:00:00Z')",
        [],
    );

    // THEN the unique index refuses it
    assert!(
        err.is_err(),
        "a second default must be refused by the database"
    );
}

#[test]
fn setting_an_unknown_profile_default_is_not_found() {
    // GIVEN the default profile alone
    let (repo, _) = setup();
    let before = repo.default_profile().unwrap();
    // WHEN an identifier that names no profile is marked default
    let err = repo.set_default_profile("missing").unwrap_err();
    // THEN the call reports it, and the default is unchanged
    assert!(matches!(err, UserMemoryError::NotFound(_)));
    assert_eq!(repo.default_profile().unwrap().id, before.id);
}

#[test]
fn schema_writes_go_to_the_default_profile_only() {
    // GIVEN a second profile made default
    let (repo, _) = setup();
    let first = repo.default_profile().unwrap();
    let second = repo.create_profile("Travail").unwrap();
    repo.set_default_profile(&second.id).unwrap();

    // WHEN a field is written through the key-value view
    repo.set("role", "Juriste", WrittenBy::User).unwrap();

    // THEN the new default carries it and the former one does not
    let profiles = repo.list_profiles().unwrap();
    let role_of = |id: &str| profiles.iter().find(|p| p.id == id).unwrap().role.clone();
    assert_eq!(role_of(&second.id).as_deref(), Some("Juriste"));
    assert_eq!(role_of(&first.id), None);
}

#[test]
fn forgetting_and_resetting_a_field_empties_it() {
    // GIVEN two fields and one free-form entry
    let (repo, _) = setup();
    repo.set("name", "Ada", WrittenBy::User).unwrap();
    repo.set(FREE_TEXT_KEY, "notes", WrittenBy::User).unwrap();
    repo.set("favorite_editor", "vim", WrittenBy::User).unwrap();

    // WHEN one field is forgotten, then everything reset
    repo.forget("name").unwrap();
    let after_forget = repo.get("name").unwrap();
    let removed = repo.reset().unwrap();

    // THEN the field read empty, the reset counted the two others, and the profile is blank
    assert!(after_forget.is_none());
    assert_eq!(removed, 2);
    assert!(repo.default_profile().unwrap().is_blank());
    assert!(repo.is_empty().unwrap());
}

#[test]
fn deleting_the_default_profile_is_refused() {
    // GIVEN the default profile and a second one
    let (repo, _) = setup();
    let default = repo.default_profile().unwrap();
    repo.create_profile("Formateur").unwrap();

    // WHEN the default profile is deleted
    let err = repo.delete_profile(&default.id).unwrap_err();

    // THEN the deletion is refused with a message naming the way out, and both profiles remain
    assert!(matches!(err, UserMemoryError::DefaultProfile(_)));
    assert!(err.to_string().contains("make another profile the default"));
    assert_eq!(repo.list_profiles().unwrap().len(), 2);
}

#[test]
fn deleting_another_profile_removes_it_and_its_designation_reads_as_default() {
    // GIVEN a second profile that a caller designates
    let (repo, _) = setup();
    let default = repo.default_profile().unwrap();
    let other = repo.create_profile("Formateur").unwrap();
    assert_eq!(
        repo.profile_or_default(Some(&other.id)).unwrap().id,
        other.id
    );

    // WHEN that profile is deleted
    repo.delete_profile(&other.id).unwrap();

    // THEN it is gone, and the designation left behind reads as the default
    assert!(repo.profile(&other.id).unwrap().is_none());
    assert_eq!(
        repo.profile_or_default(Some(&other.id)).unwrap().id,
        default.id
    );
    assert_eq!(repo.profile_or_default(None).unwrap().id, default.id);
    assert!(matches!(
        repo.delete_profile(&other.id).unwrap_err(),
        UserMemoryError::NotFound(_)
    ));
}

#[test]
fn a_field_written_by_identifier_lands_in_that_profile_only() {
    // GIVEN the default profile and a second one
    let (repo, _) = setup();
    let default = repo.default_profile().unwrap();
    let other = repo.create_profile("Responsable SI").unwrap();

    // WHEN a field is written on the second, then a key outside the schema
    repo.set_profile_field(&other.id, "role", Some("DSI"))
        .unwrap();
    let outside = repo.set_profile_field(&other.id, "monthly_budget", Some("10"));

    // THEN the second carries it, the default does not, and the outside key is refused
    let reread = repo.profile(&other.id).unwrap().unwrap();
    assert_eq!(reread.role.as_deref(), Some("DSI"));
    assert_eq!(repo.profile(&default.id).unwrap().unwrap().role, None);
    assert!(matches!(outside, Err(UserMemoryError::InvalidKey(_))));
    assert_eq!(
        repo.get_in(&reread, "role").unwrap().map(|e| e.value),
        Some("DSI".to_owned())
    );
}

#[test]
fn a_profile_is_renamed_and_an_empty_name_is_refused() {
    // GIVEN a second profile
    let (repo, _) = setup();
    let other = repo.create_profile("Formateur").unwrap();

    // WHEN it is renamed, then renamed to blanks
    repo.rename_profile(&other.id, "  Formatrice ").unwrap();
    let blank = repo.rename_profile(&other.id, "   ");

    // THEN the trimmed name is kept and the blank one refused
    assert_eq!(repo.profile(&other.id).unwrap().unwrap().name, "Formatrice");
    assert!(matches!(blank, Err(UserMemoryError::InvalidKey(_))));
}
