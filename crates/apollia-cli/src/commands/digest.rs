//! `apollia-os digest`: aggregate activity overview.
//!
//! Combines the runtime's tasks list, LLM cost summary, and audit stats into
//! a compact "what happened recently" snapshot. It is a lightweight aggregator
//! over the existing HTTP routes so the runtime does not need any new endpoint.

use std::path::PathBuf;

use clap::ValueEnum;

use crate::client::{ClientError, RuntimeClient, DEFAULT_SOCKET_PATH};
use crate::exit_codes;

/// Time windows recognised by the digest command.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum DigestWindow {
    /// Last 24 hours.
    #[value(name = "24h")]
    Day,
    /// Last 7 days.
    #[value(name = "7d")]
    Week,
    /// Last 30 days.
    #[value(name = "30d")]
    Month,
}

impl DigestWindow {
    fn label(self) -> &'static str {
        match self {
            DigestWindow::Day => "24h",
            DigestWindow::Week => "7d",
            DigestWindow::Month => "30d",
        }
    }
}

/// Execute `apollia-os digest [--since 24h|7d|30d]`.
pub async fn run(window: DigestWindow, socket: Option<PathBuf>, json: bool) -> i32 {
    let path = socket.unwrap_or_else(|| PathBuf::from(DEFAULT_SOCKET_PATH));
    let client = RuntimeClient::new(path);

    let tasks = match client.get("/api/v1/tasks").await {
        Ok(r) if r.status < 400 => serde_json::from_str(&r.body).unwrap_or(serde_json::Value::Null),
        Ok(r) => {
            eprintln!("Error fetching tasks: HTTP {}: {}", r.status, r.body);
            return exit_codes::GENERAL_ERROR;
        }
        Err(ClientError::ConnectionRefused) => {
            eprintln!("Error: runtime not started");
            return exit_codes::RUNTIME_ERROR;
        }
        Err(e) => {
            eprintln!("Error: {e}");
            return exit_codes::GENERAL_ERROR;
        }
    };

    let costs = client
        .get_llm_costs()
        .await
        .unwrap_or(serde_json::Value::Null);
    let audit_stats = client
        .get_audit_stats()
        .await
        .unwrap_or(serde_json::Value::Null);

    let (task_total, task_failed, task_running) = task_counts(&tasks);

    if json {
        let out = serde_json::json!({
            "since": window.label(),
            "tasks": {
                "total": task_total,
                "running": task_running,
                "failed": task_failed,
            },
            "llm_costs": costs,
            "audit": audit_stats,
        });
        println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
    } else {
        println!("  Apollia OS digest ({}):", window.label());
        println!();
        println!("  Tasks");
        println!("    total    {task_total}");
        println!("    running  {task_running}");
        println!("    failed   {task_failed}");
        println!();
        println!("  LLM");
        if let Some(total_cost) = costs
            .get("total_cost_usd")
            .or_else(|| costs.get("total_usd"))
            .and_then(|v| v.as_f64())
        {
            println!("    cost     ${total_cost:.4}");
        } else {
            println!("    cost     n/a");
        }
        if let Some(calls) = costs.get("total_calls").and_then(|v| v.as_u64()) {
            println!("    calls    {calls}");
        }
        println!();
        println!("  Audit");
        if let Some(total) = audit_stats.get("total").and_then(|v| v.as_u64()) {
            println!("    events   {total}");
        } else {
            println!("    events   n/a");
        }
    }
    exit_codes::SUCCESS
}

fn task_counts(tasks: &serde_json::Value) -> (usize, usize, usize) {
    let arr = tasks
        .get("tasks")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_else(|| tasks.as_array().cloned().unwrap_or_default());
    let total = arr.len();
    let mut failed = 0;
    let mut running = 0;
    for t in &arr {
        let status = t
            .get("status")
            .or_else(|| t.get("state"))
            .and_then(|s| s.as_str())
            .unwrap_or("");
        match status {
            "failed" | "Failed" => failed += 1,
            "running" | "Running" | "in_progress" => running += 1,
            _ => {}
        }
    }
    (total, failed, running)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_counts_empty() {
        let (t, f, r) = task_counts(&serde_json::json!({"tasks": []}));
        assert_eq!((t, f, r), (0, 0, 0));
    }

    #[test]
    fn task_counts_mixed_statuses() {
        let v = serde_json::json!({
            "tasks": [
                {"status": "running"},
                {"status": "failed"},
                {"status": "completed"},
                {"status": "running"},
            ]
        });
        let (t, f, r) = task_counts(&v);
        assert_eq!((t, f, r), (4, 1, 2));
    }
}
