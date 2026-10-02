//! Notifications of the batch engine: an item waiting for an access, and the
//! end of a batch with its counters.

use std::collections::HashMap;

use apollia_core::RuntimeEvent;
use chrono::Utc;

use crate::{config::Severity, engine::Notification};

/// The notification of a batch event, `None` for any other event.
pub(super) fn map_batch_event(dashboard_url: String, event: &RuntimeEvent) -> Option<Notification> {
    match event {
        RuntimeEvent::BatchItemAwaitingApproval {
            batch_id,
            rank,
            agent_id,
            access,
        } => {
            let mut metadata = HashMap::new();
            metadata.insert("batch_id".into(), batch_id.clone());
            metadata.insert("rank".into(), rank.to_string());
            metadata.insert("access".into(), access.clone());
            metadata.insert("dashboard_url".into(), dashboard_url);
            Some(Notification {
                event: "batch.item_awaiting_approval".into(),
                timestamp: Utc::now(),
                task_id: None,
                agent: Some(agent_id.to_string()),
                message: format!(
                    "An item of a batch waits for your decision: the agent asks to use {access}"
                ),
                metadata,
                severity: Severity::Warning,
            })
        }
        RuntimeEvent::BatchEnded {
            batch_id,
            agent_id,
            state,
            total,
            completed,
            failed,
            canceled,
            awaiting,
        } => {
            let mut metadata = HashMap::new();
            metadata.insert("batch_id".into(), batch_id.clone());
            metadata.insert("state".into(), state.clone());
            metadata.insert("total".into(), total.to_string());
            metadata.insert("completed".into(), completed.to_string());
            metadata.insert("failed".into(), failed.to_string());
            metadata.insert("canceled".into(), canceled.to_string());
            metadata.insert("awaiting".into(), awaiting.to_string());
            metadata.insert("dashboard_url".into(), dashboard_url);
            let severity = if *failed > 0 || *awaiting > 0 || state != "completed" {
                Severity::Warning
            } else {
                Severity::Info
            };
            Some(Notification {
                event: "batch.ended".into(),
                timestamp: Utc::now(),
                task_id: None,
                agent: Some(agent_id.to_string()),
                message: format!(
                    "Batch {state}: {completed} succeeded, {failed} failed, {awaiting} waiting, out of {total}"
                ),
                metadata,
                severity,
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_an_item_waiting_for_an_access_asks_for_a_decision() {
        // GIVEN an item of a batch stopped on an access nobody granted
        let event = RuntimeEvent::BatchItemAwaitingApproval {
            batch_id: "b-1".into(),
            rank: 4,
            agent_id: "clerk".into(),
            access: "send_email".into(),
        };
        // WHEN it is mapped
        let notif = map_batch_event("http://x/dashboard".into(), &event).expect("mapped");
        // THEN a warning names the batch, the item and the access
        assert_eq!(notif.event, "batch.item_awaiting_approval");
        assert_eq!(notif.severity, Severity::Warning);
        assert_eq!(notif.agent.as_deref(), Some("clerk"));
        assert_eq!(notif.metadata["batch_id"], "b-1");
        assert_eq!(notif.metadata["rank"], "4");
        assert!(notif.message.contains("send_email"), "{}", notif.message);
    }

    #[test]
    fn test_the_end_of_a_batch_carries_its_successes_and_failures() {
        // GIVEN a batch that ended with two failures
        let event = RuntimeEvent::BatchEnded {
            batch_id: "b-1".into(),
            agent_id: "clerk".into(),
            state: "completed".into(),
            total: 10,
            completed: 8,
            failed: 2,
            canceled: 0,
            awaiting: 0,
        };
        // WHEN it is mapped
        let notif = map_batch_event("http://x/dashboard".into(), &event).expect("mapped");
        // THEN the notification counts successes and failures
        assert_eq!(notif.event, "batch.ended");
        assert_eq!(notif.metadata["completed"], "8");
        assert_eq!(notif.metadata["failed"], "2");
        assert_eq!(notif.severity, Severity::Warning);
        assert!(
            notif.message.contains("8 succeeded, 2 failed"),
            "{}",
            notif.message
        );
    }
}
