//! The profile a conversation designates, in `chat_sessions.profile_id`.
//!
//! The column names a profile of `user_memory.db`, a separate file, so no
//! foreign key holds it: deleting a profile empties the sessions that named
//! it through [`ChatSessionRepository::release_profile`], and a designation
//! left behind by a crash reads as empty wherever the profile is resolved.

use rusqlite::params;

use super::ChatSessionRepository;
use crate::chat::types::ChatError;

impl ChatSessionRepository {
    /// Empty `profile_id` on every session designating `profile_id`; returns
    /// how many sessions designated it.
    pub fn release_profile(&self, profile_id: &str) -> Result<usize, ChatError> {
        self.conn
            .execute(
                "UPDATE chat_sessions SET profile_id = NULL WHERE profile_id = ?1",
                params![profile_id],
            )
            .map_err(|e| ChatError::InternalError(format!("release_profile: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::types::{ChatMode, SessionConfigUpdate};

    fn session(repo: &ChatSessionRepository, id: &str) {
        repo.create_session(
            id,
            &ChatMode::Libre,
            None,
            "",
            &[],
            "2026-10-04T00:00:00Z",
            None,
        )
        .expect("session created");
    }

    fn profile_of(repo: &ChatSessionRepository, id: &str) -> Option<String> {
        repo.get_session(id)
            .expect("read")
            .expect("session exists")
            .profile_id
    }

    #[test]
    fn a_new_session_designates_no_profile() {
        // GIVEN a freshly migrated chat database
        let repo = ChatSessionRepository::open_in_memory().expect("open");

        // WHEN a session is created without a choice
        session(&repo, "b");

        // THEN its profile is empty
        assert_eq!(profile_of(&repo, "b"), None);
    }

    #[test]
    fn releasing_a_profile_empties_only_the_sessions_naming_it() {
        // GIVEN two sessions on "p-form" and one on "p-jur"
        let repo = ChatSessionRepository::open_in_memory().expect("open");
        for id in ["a1", "a2", "c"] {
            session(&repo, id);
        }
        for (id, profile) in [("a1", "p-form"), ("a2", "p-form"), ("c", "p-jur")] {
            let update = SessionConfigUpdate {
                profile_id: Some(Some(profile.to_owned())),
                ..SessionConfigUpdate::default()
            };
            repo.update_session_config(id, &update).expect("designated");
        }

        // WHEN "p-form" is released
        let released = repo.release_profile("p-form").expect("released");

        // THEN both of its sessions are empty and the other keeps its profile
        assert_eq!(released, 2);
        assert_eq!(profile_of(&repo, "a1"), None);
        assert_eq!(profile_of(&repo, "a2"), None);
        assert_eq!(profile_of(&repo, "c").as_deref(), Some("p-jur"));
    }
}
