//! The profile block of the built-in assistant.
//!
//! The assistant behind the chat window receives the profile of its
//! conversation in its system prompt on every turn, whatever its autonomy
//! tier, while the profile switch of the conversation is on. No other agent
//! receives it. The prompt builder and the conversation gauge both answer
//! from this module, so the gauge says what the prompt carries.

use std::sync::Mutex;

use apollia_memory::user_memory::UserMemoryRepository;
use tracing::warn;

use super::types::{ChatMode, ChatSession};

/// How many free-form entries of the profile the block lists at most.
const PERSONA_ENTRIES: usize = 30;

/// Whether `session` hands its profile to the assistant: a free chat whose
/// profile switch is on. A companion session and an agent session never do.
pub fn switch_on(session: &ChatSession) -> bool {
    session.mode == ChatMode::Libre && session.use_profile
}

/// The profile store the executor is handed for `session`: the store while
/// the switch is on, nothing otherwise, so a switched-off conversation builds
/// a prompt that cannot carry a profile.
pub fn switched_store<S: Clone>(session: &ChatSession, store: Option<&S>) -> Option<S> {
    switch_on(session).then(|| store.cloned()).flatten()
}

/// The block the assistant's prompt carries for the profile `designated`
/// names, or for the default profile when nothing is designated. `None`
/// without a profile store, and when that profile holds nothing.
pub fn profile_block(
    store: Option<&Mutex<UserMemoryRepository>>,
    designated: Option<&str>,
) -> Option<String> {
    let repo = match store?.lock() {
        Ok(repo) => repo,
        Err(e) => {
            warn!(error = %e, "memory.user.lock.poisoned");
            return None;
        }
    };
    match repo.recall_persona_brief(designated, PERSONA_ENTRIES) {
        Ok(brief) if !brief.is_empty() => Some(apollia_prompts::blocks::persona_block(&brief)),
        Ok(_) => None,
        Err(e) => {
            warn!(error = %e, "memory.user.read.failed");
            None
        }
    }
}

/// Whether the prompt of `session` carries a profile block on its next turn:
/// the switch is on and the profile of the conversation holds something.
pub fn injected(session: &ChatSession, store: Option<&Mutex<UserMemoryRepository>>) -> bool {
    switch_on(session) && profile_block(store, session.profile_id.as_deref()).is_some()
}

/// A conversation as the tests of the profile block need it.
#[cfg(test)]
pub(crate) fn test_session(
    mode: ChatMode,
    use_profile: bool,
    profile_id: Option<&str>,
) -> ChatSession {
    ChatSession {
        id: "sess-profile".into(),
        mode,
        agent_name: None,
        system_prompt: String::new(),
        status: super::types::SessionStatus::Active,
        history: vec![],
        authorized_tools: std::collections::HashSet::new(),
        available_tools: vec![],
        created_at: "2026-10-04T10:00:00Z".into(),
        active_exchange: None,
        llm_backend: None,
        title: None,
        parent_session_id: None,
        fork_depth: 0,
        plan_mode: false,
        plan_phase: super::types::PlanPhase::Done,
        profile_id: profile_id.map(str::to_owned),
        use_profile,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    /// A store whose default profile is a "CTO", with a "Formateur" profile
    /// and a blank one; returns the store and the two identifiers.
    fn store() -> (Arc<Mutex<UserMemoryRepository>>, String, String) {
        let dir = tempfile::tempdir().expect("tempdir").keep();
        let repo =
            UserMemoryRepository::new(&dir.join("user_memory.db")).expect("user_memory.db opens");
        let default = repo.default_profile().expect("default");
        repo.set_profile_field(&default.id, "role", Some("CTO"))
            .expect("role written");
        let formateur = repo.create_profile("Formateur").expect("created");
        repo.set_profile_field(&formateur.id, "role", Some("formateur"))
            .expect("role written");
        let blank = repo.create_profile("Blank").expect("created");
        (Arc::new(Mutex::new(repo)), formateur.id, blank.id)
    }

    #[test]
    fn the_gauge_is_true_exactly_when_the_prompt_carries_the_block() {
        // GIVEN a store, and conversations differing by switch, profile and mode
        let (store, formateur, blank) = store();
        let on = test_session(ChatMode::Libre, true, Some(&formateur));
        let off = test_session(ChatMode::Libre, false, Some(&formateur));
        let on_blank = test_session(ChatMode::Libre, true, Some(&blank));
        let companion = test_session(ChatMode::Companion, true, Some(&formateur));

        // WHEN the gauge is asked for each, and the block is built as the executor builds it
        let carried = |session: &ChatSession| {
            profile_block(
                switched_store(session, Some(&store)).as_deref(),
                session.profile_id.as_deref(),
            )
            .is_some()
        };

        // THEN it is true for the switched-on conversation alone, and always
        // agrees with what the prompt carries
        assert!(injected(&on, Some(&store)));
        assert!(!injected(&off, Some(&store)));
        assert!(!injected(&on_blank, Some(&store)));
        assert!(!injected(&companion, Some(&store)));
        assert!(!injected(&on, None));
        for session in [&on, &off, &on_blank, &companion] {
            assert_eq!(injected(session, Some(&store)), carried(session));
        }
    }

    #[test]
    fn the_block_reads_the_profile_of_the_conversation() {
        // GIVEN a conversation on "Formateur" and one that designates nothing
        let (store, formateur, _) = store();

        // WHEN the block of each is built
        let designated = profile_block(Some(&store), Some(&formateur)).expect("block");
        let default = profile_block(Some(&store), None).expect("block");

        // THEN each carries the role of its own profile
        assert!(designated.contains("## User Persona"), "{designated}");
        assert!(designated.contains("Role: formateur"), "{designated}");
        assert!(!designated.contains("CTO"), "{designated}");
        assert!(default.contains("Role: CTO"), "{default}");
    }
}
