use super::*;
use crate::task_repository::open_conn;

fn seed(path: &std::path::Path) {
    let conn = open_conn(path).unwrap();
    conn.execute_batch(
        "INSERT INTO tasks (task_id, agent_name, status) VALUES
             ('t-1', 'clerk', 'completed'), ('t-2', 'clerk', 'failed'), ('t-3', 'other', 'completed');
         INSERT INTO task_approvals (task_id, prompt, context_json) VALUES
             ('t-1', 'send?', '{}'), ('t-3', 'send?', '{}');
         INSERT INTO batches (batch_id, agent_name, source_json, max_failures, state) VALUES
             ('b-1', 'clerk', '{}', 5, 'done'), ('b-2', 'other', '{}', 5, 'done');
         INSERT INTO batch_items (batch_id, rank, origin_ref, input_json, task_id) VALUES
             ('b-1', 0, 'row-2', '{}', 't-2'), ('b-2', 0, 'row-2', '{}', 't-3');
         INSERT INTO workflow_runs (run_id, workflow_id, inputs_json) VALUES
             ('r-1', 'invoices/close-month', '{}'), ('r-2', 'other/flow', '{}');
         INSERT INTO workflow_steps (run_id, rank, name) VALUES
             ('r-1', 0, 'extract'), ('r-2', 0, 'extract');",
    )
    .unwrap();
}

fn remaining(path: &std::path::Path, sql: &str) -> Vec<String> {
    let conn = open_conn(path).unwrap();
    let mut stmt = conn.prepare(sql).unwrap();
    let rows = stmt.query_map([], |row| row.get::<_, String>(0)).unwrap();
    rows.collect::<Result<_, _>>().unwrap()
}

// GIVEN tasks, approvals, batches, items and workflow runs of two agents and two workflows
// WHEN the history of one agent and one workflow is removed
// THEN their rows and the rows that hang off them are gone, the others stay
#[tokio::test]
async fn test_remove_history_removes_only_the_named_agents_and_workflows() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hitl.db");
    let repo = TaskRepository::open(&path).await.unwrap();
    seed(&path);

    let removed = repo
        .remove_history(vec!["clerk".into()], vec!["invoices/close-month".into()])
        .await
        .unwrap();

    assert_eq!(
        removed,
        HistoryRemoved {
            tasks: 2,
            batches: 1,
            workflow_runs: 1
        }
    );
    assert_eq!(remaining(&path, "SELECT task_id FROM tasks"), vec!["t-3"]);
    assert_eq!(
        remaining(&path, "SELECT task_id FROM task_approvals"),
        vec!["t-3"]
    );
    assert_eq!(
        remaining(&path, "SELECT batch_id FROM batches"),
        vec!["b-2"]
    );
    assert_eq!(
        remaining(&path, "SELECT batch_id FROM batch_items"),
        vec!["b-2"]
    );
    assert_eq!(
        remaining(&path, "SELECT run_id FROM workflow_runs"),
        vec!["r-2"]
    );
    assert_eq!(
        remaining(&path, "SELECT run_id FROM workflow_steps"),
        vec!["r-2"]
    );
}

// GIVEN an empty history
// WHEN the history of an agent that never ran is removed
// THEN nothing is counted
#[tokio::test]
async fn test_remove_history_of_an_agent_without_history_counts_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let repo = TaskRepository::open(&dir.path().join("hitl.db"))
        .await
        .unwrap();

    let removed = repo
        .remove_history(vec!["ghost".into()], Vec::new())
        .await
        .unwrap();

    assert_eq!(removed, HistoryRemoved::default());
}
