//! A conversation designates its profile: empty, it follows the current
//! default; designated, it keeps its profile across a restart; the deletion of
//! the profile returns it to empty. Its profile switch is on until turned
//! off, survives a restart, and the gauge of the conversation follows it.

use std::sync::{Arc, Mutex};

use apollia_core::StepBudgetConfig;
use apollia_mcp::session::LoadingMode;
use apollia_memory::user_memory::UserMemoryRepository;
use apollia_tools::ToolRegistryHandle;

use crate::chat::manager::{ChatSessionManagerHandle, CreateSessionParams};
use crate::chat::types::{ChatMode, SessionConfigUpdate};
use crate::profile::{delete_profile_and_release, profile_for_execution, ProfileStore};

fn spawn(dir: &tempfile::TempDir) -> ChatSessionManagerHandle {
    let (event_tx, _) = tokio::sync::broadcast::channel(128);
    ChatSessionManagerHandle::spawn(
        &dir.path().join("chat.db"),
        Some(Arc::new(apollia_llm::LlmRouter::empty())),
        ToolRegistryHandle::start(),
        None,
        event_tx.clone(),
        StepBudgetConfig::default(),
        None,
        crate::registry::AgentRegistry::spawn(event_tx),
        None,
        None,
        None,
        LoadingMode::Eager,
        20,
        None,
        false,
    )
    .expect("spawn manager")
}

fn profiles(dir: &tempfile::TempDir) -> Arc<Mutex<UserMemoryRepository>> {
    Arc::new(Mutex::new(
        UserMemoryRepository::new(&apollia_core::paths::DataFile::UserMemory.path(dir.path()))
            .expect("user_memory.db opens"),
    ))
}

async fn new_session(handle: &ChatSessionManagerHandle) -> String {
    handle
        .create_session(CreateSessionParams {
            mode: ChatMode::Libre,
            agent_name: None,
            system_prompt: None,
            tools: vec![],
            model: None,
        })
        .await
        .expect("session created")
        .id
}

async fn profile_of(handle: &ChatSessionManagerHandle, id: &str) -> Option<String> {
    handle
        .get_session(id.to_owned())
        .await
        .expect("session exists")
        .session
        .profile_id
}

async fn designate(handle: &ChatSessionManagerHandle, id: &str, profile: Option<&str>) {
    handle
        .update_session(
            id.to_owned(),
            SessionConfigUpdate {
                profile_id: Some(profile.map(str::to_owned)),
                ..SessionConfigUpdate::default()
            },
        )
        .await
        .expect("profile designated");
}

#[tokio::test]
async fn a_conversation_without_a_choice_follows_the_current_default() {
    // GIVEN a conversation with no profile, a default profile and a "Juriste" profile
    let dir = tempfile::tempdir().expect("tempdir");
    let handle = spawn(&dir);
    let store = profiles(&dir);
    let (first_default, juriste) = {
        let repo = store.lock().expect("lock");
        (
            repo.default_profile().expect("default"),
            repo.create_profile("Juriste").expect("created"),
        )
    };
    let b = new_session(&handle).await;

    // WHEN its profile is resolved, then resolved again once "Juriste" becomes the default
    let designated = profile_of(&handle, &b).await;
    let repo = store.lock().expect("lock");
    let before = profile_for_execution(&repo, dir.path(), "apollia", designated.as_deref())
        .expect("resolved");
    repo.set_default_profile(&juriste.id).expect("new default");
    let after = profile_for_execution(&repo, dir.path(), "apollia", designated.as_deref())
        .expect("resolved");

    // THEN it designates nothing and follows the default of the moment
    assert_eq!(designated, None);
    assert_eq!(before.id, first_default.id);
    assert_eq!(after.id, juriste.id);
}

#[tokio::test]
async fn a_designated_profile_survives_a_restart_and_the_other_stays_empty() {
    // GIVEN conversation A on "Formateur" and conversation B without a choice
    let dir = tempfile::tempdir().expect("tempdir");
    let store = profiles(&dir);
    let formateur = store
        .lock()
        .expect("lock")
        .create_profile("Formateur")
        .expect("created");
    let handle = spawn(&dir);
    let a = new_session(&handle).await;
    let b = new_session(&handle).await;
    designate(&handle, &a, Some(&formateur.id)).await;

    // WHEN the chat restarts on the same database
    handle.shutdown().await;
    let reopened = spawn(&dir);

    // THEN A is still on "Formateur" and B still follows the default
    assert_eq!(
        profile_of(&reopened, &a).await.as_deref(),
        Some(formateur.id.as_str())
    );
    assert_eq!(profile_of(&reopened, &b).await, None);
    // AND returning A to the default empties its designation
    designate(&reopened, &a, None).await;
    assert_eq!(profile_of(&reopened, &a).await, None);
}

#[tokio::test]
async fn deleting_a_profile_empties_the_conversations_designating_it() {
    // GIVEN two conversations on "Formateur" and one on "Juriste"
    let dir = tempfile::tempdir().expect("tempdir");
    let store = profiles(&dir);
    let (formateur, juriste) = {
        let repo = store.lock().expect("lock");
        (
            repo.create_profile("Formateur").expect("created"),
            repo.create_profile("Juriste").expect("created"),
        )
    };
    let handle = spawn(&dir);
    let a1 = new_session(&handle).await;
    let a2 = new_session(&handle).await;
    let c = new_session(&handle).await;
    designate(&handle, &a1, Some(&formateur.id)).await;
    designate(&handle, &a2, Some(&formateur.id)).await;
    designate(&handle, &c, Some(&juriste.id)).await;

    // WHEN "Formateur" is deleted
    delete_profile_and_release(ProfileStore(&store), Some(&handle), formateur.id.clone())
        .await
        .expect("deleted");

    // THEN its conversations are empty, in memory and after a restart
    assert_eq!(profile_of(&handle, &a1).await, None);
    assert_eq!(profile_of(&handle, &a2).await, None);
    handle.shutdown().await;
    let reopened = spawn(&dir);
    assert_eq!(profile_of(&reopened, &a1).await, None);
    assert_eq!(profile_of(&reopened, &a2).await, None);
    // AND the conversation on "Juriste" keeps it
    assert_eq!(
        profile_of(&reopened, &c).await.as_deref(),
        Some(juriste.id.as_str())
    );
}

async fn switch(handle: &ChatSessionManagerHandle, id: &str, on: bool) {
    handle
        .update_session(
            id.to_owned(),
            SessionConfigUpdate {
                use_profile: Some(on),
                ..SessionConfigUpdate::default()
            },
        )
        .await
        .expect("switch written");
}

async fn switch_of(handle: &ChatSessionManagerHandle, id: &str) -> bool {
    handle
        .get_session(id.to_owned())
        .await
        .expect("session exists")
        .session
        .use_profile
}

#[tokio::test]
async fn the_profile_switch_is_on_at_birth_and_survives_a_restart_once_turned_off() {
    // GIVEN two new conversations
    let dir = tempfile::tempdir().expect("tempdir");
    let handle = spawn(&dir);
    let a = new_session(&handle).await;
    let b = new_session(&handle).await;
    assert!(switch_of(&handle, &a).await);

    // WHEN the switch of A is turned off and the chat restarts
    switch(&handle, &a, false).await;
    assert!(!switch_of(&handle, &a).await);
    handle.shutdown().await;
    let reopened = spawn(&dir);

    // THEN A is still off, B still on, and A can be turned back on
    assert!(!switch_of(&reopened, &a).await);
    assert!(switch_of(&reopened, &b).await);
    switch(&reopened, &a, true).await;
    assert!(switch_of(&reopened, &a).await);
}

#[tokio::test]
async fn the_gauge_says_injected_only_while_the_switch_is_on_and_the_profile_filled() {
    // GIVEN a conversation on "Formateur", whose role is filled
    let dir = tempfile::tempdir().expect("tempdir");
    let store = profiles(&dir);
    let formateur = {
        let repo = store.lock().expect("lock");
        let profile = repo.create_profile("Formateur").expect("created");
        repo.set_profile_field(&profile.id, "role", Some("formateur"))
            .expect("role written");
        profile
    };
    let handle = spawn(&dir);
    let a = new_session(&handle).await;
    let stats = |id: String| async {
        crate::conversation_stats::conversation_stats(Some(&handle), Some(&store), id)
            .await
            .expect("stats")
            .user_memory_injected
    };

    // WHEN the gauge is read before a profile is designated, once it is, and
    // once the switch is off
    let on_blank_default = stats(a.clone()).await;
    designate(&handle, &a, Some(&formateur.id)).await;
    let on_filled = stats(a.clone()).await;
    switch(&handle, &a, false).await;
    let off = stats(a.clone()).await;

    // THEN it is true only while the switch is on and the profile holds something
    assert!(!on_blank_default);
    assert!(on_filled);
    assert!(!off);
}
