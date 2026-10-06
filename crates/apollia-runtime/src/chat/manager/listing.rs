//! The lines of the session list, each with what the conversation weighs.

use super::*;

impl ChatSessionManager {
    /// The list entries of `rows`, each with its message count and the tokens
    /// generated for it. A row whose mode or status is unknown is left out;
    /// totals that cannot be read count for zero, with a warning.
    pub(in crate::chat::manager) fn infos_of(
        &self,
        rows: Vec<crate::chat::repository::SessionRow>,
    ) -> Vec<SessionInfo> {
        let totals = self.repository.session_totals().unwrap_or_else(|e| {
            warn!(error = %e, "chat.sessions.totals.failed");
            std::collections::HashMap::new()
        });
        rows.into_iter()
            .filter_map(|row| {
                let mode = ChatMode::from_sql(&row.mode)?;
                let status = SessionStatus::from_sql(&row.status)?;
                let weight = totals.get(&row.id).copied().unwrap_or_default();
                Some(SessionInfo {
                    id: row.id,
                    mode,
                    agent_name: row.agent_name,
                    status,
                    created_at: row.created_at,
                    title: row.title,
                    message_count: weight.message_count,
                    output_tokens: weight.output_tokens,
                })
            })
            .collect()
    }
}
