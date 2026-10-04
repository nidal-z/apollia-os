use std::sync::{Arc, Mutex};

use apollia_memory::user_memory::UserMemoryRepository;
use tempfile::TempDir;

use super::*;

fn repo(dir: &TempDir) -> Arc<Mutex<UserMemoryRepository>> {
    Arc::new(Mutex::new(
        UserMemoryRepository::new(&dir.path().join("user_memory.db")).unwrap(),
    ))
}

#[tokio::test]
async fn the_default_alias_reads_the_default_profile_with_every_field_of_the_schema() {
    // GIVEN a fresh profile store
    let dir = TempDir::new().unwrap();
    let repo = repo(&dir);

    // WHEN the default profile is read through its alias
    let detail = read_profile_by_id(ProfileStore(&repo), DEFAULT_PROFILE_ALIAS.to_owned())
        .await
        .unwrap();

    // THEN it is the default, and its fields are the six of the schema, empty
    assert!(detail.profile.is_default);
    let keys: Vec<&str> = detail.fields.iter().map(|f| f.key.as_str()).collect();
    assert_eq!(
        keys,
        [
            "name",
            "role",
            "function",
            "tools",
            "preferences",
            "free_text"
        ]
    );
    assert!(detail.fields.iter().all(|f| f.value.is_empty()));
    assert!(detail.fields.iter().any(|f| f.long_text));
}

#[tokio::test]
async fn an_update_names_only_fields_of_the_schema() {
    // GIVEN a second profile
    let dir = TempDir::new().unwrap();
    let repo = repo(&dir);
    let trainer = create_profile(ProfileStore(&repo), "Formateur".to_owned())
        .await
        .unwrap();

    // WHEN it is renamed with its role, then written with a former key
    let renamed = update_profile(
        ProfileStore(&repo),
        trainer.id.clone(),
        ProfileUpdate {
            name: Some("Formatrice".to_owned()),
            fields: BTreeMap::from([("role".to_owned(), " Trainer ".to_owned())]),
        },
    )
    .await
    .unwrap();
    let refused = update_profile(
        ProfileStore(&repo),
        trainer.id,
        ProfileUpdate {
            name: None,
            fields: BTreeMap::from([("tech.stack".to_owned(), "Rust".to_owned())]),
        },
    )
    .await;

    // THEN the name and the trimmed role are stored, the list shows the role,
    // and the former key is refused
    assert_eq!(renamed.profile.name, "Formatrice");
    assert_eq!(renamed.profile.detail.as_deref(), Some("Trainer"));
    assert!(matches!(refused, Err(ProfileError::InvalidKey(_))));
}

#[tokio::test]
async fn deleting_the_default_profile_is_refused_in_words() {
    // GIVEN the default profile only
    let dir = TempDir::new().unwrap();
    let repo = repo(&dir);

    // WHEN it is deleted
    let refused = delete_profile(ProfileStore(&repo), DEFAULT_PROFILE_ALIAS.to_owned()).await;

    // THEN the refusal names the profile and says what to do
    let message = refused.unwrap_err().to_string();
    assert!(message.contains("Mon profil"), "{message}");
    assert!(message.contains("make another profile the default first"));
}
