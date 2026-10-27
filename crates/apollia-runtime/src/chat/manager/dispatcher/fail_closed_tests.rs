//! The chat dispatcher under a governance database that cannot be read: a
//! tool governance disabled stays out of the conversation, and a sensitive
//! call is refused by the decision.

use std::sync::Arc;

use super::*;
use crate::governance::{
    AccessCall, AccessDecision, DecisionReason, GovernanceContract, GovernanceHandle,
    GovernanceSources,
};

// GIVEN a governance.db that stopped being readable after disabling
//       `file_grep`, behind the governance actor the chat reads
// WHEN the chat computes the tools it leaves out, and a writing call is
//      decided
// THEN `file_grep` stays out, and the write is refused as fail-closed
#[tokio::test]
async fn test_unreadable_store_keeps_disabled_tools_and_refuses() {
    // GIVEN
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("governance.db");
    apollia_tools::GovernanceDb::open(dir.path()).unwrap();
    apollia_tools::NativeToolRegistry::new(&db)
        .unwrap()
        .set_enabled("file_grep", false)
        .unwrap();
    let governance = GovernanceHandle::spawn(GovernanceSources {
        governance_db: Some(db.clone()),
        config_path: None,
        policy: Arc::new(apollia_permissions::PolicyVerdict::Absent),
        journal: None,
        events: None,
        forbidden_connections: Default::default(),
    });
    for suffix in ["-wal", "-shm"] {
        let mut side = db.clone().into_os_string();
        side.push(suffix);
        let _ = std::fs::remove_file(std::path::PathBuf::from(side));
    }
    std::fs::write(&db, vec![0x5a_u8; 4096]).unwrap();
    governance.refresh().await;
    let cfg = ChatToolsConfig {
        data_dir: dir.path().to_path_buf(),
        brave_api_key: None,
        tools_config: apollia_core::ToolsConfig::default(),
        tool_turn_temperature: None,
        source_registry: None,
        governance: Some(governance.clone()),
    };

    // WHEN
    let disabled = effective_disabled_tools(&cfg);
    let write = governance.decide(&AccessCall {
        agent_id: "apollia",
        access_id: "file_write",
        argument: None,
        reaches: &[],
        family: apollia_tools::ActionFamily::WriteOrDelete,
        unattended: false,
        granted_for_run: false,
    });

    // THEN
    assert!(disabled.contains(&"file_grep".to_owned()), "{disabled:?}");
    assert_eq!(
        write,
        AccessDecision::Denied {
            reason: DecisionReason::FailClosed
        }
    );
}
