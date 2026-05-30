//! `apollia-os plan-cache` subcommands: plan cache statistics and management.
//!
//! Accesses `~/.apollia/plan_cache.db` directly (no runtime connection required).
//! Exposes the `stats`, `clear`, and `evict` operations from [`PlanCacheRepository`].

use std::io::{self, Write};
use std::path::{Path, PathBuf};

use apollia_oria::plan_cache::PlanCacheRepository;
use clap::Subcommand;

use crate::exit_codes;

/// Plan-cache subcommands: `apollia-os plan-cache <verb>`.
#[derive(Debug, Subcommand)]
pub enum PlanCacheCommand {
    /// Display cache statistics (total entries, hits, oldest/newest entry).
    Stats,
    /// Remove all cached plans.
    ///
    /// Prompts for confirmation unless `--force` is passed.
    Clear {
        /// Skip interactive confirmation (non-interactive / CI-friendly).
        #[arg(long)]
        force: bool,
    },
    /// Evict entries older than `--max-age-days` days (default: 7).
    Evict {
        /// Maximum entry age in days before eviction.
        #[arg(long, default_value = "7", value_name = "DAYS")]
        max_age_days: u32,
    },
}

/// Execute a `plan-cache` subcommand. Returns the process exit code.
pub fn run(cmd: &PlanCacheCommand, json: bool) -> i32 {
    let data_dir = apollia_data_dir();
    let db_path = data_dir.join("plan_cache.db");

    match cmd {
        PlanCacheCommand::Stats => run_stats(&db_path, json),
        PlanCacheCommand::Clear { force } => run_clear(&db_path, *force, json),
        PlanCacheCommand::Evict { max_age_days } => run_evict(&db_path, *max_age_days, json),
    }
}

/// `apollia-os plan-cache stats`: display aggregate cache statistics.
fn run_stats(db_path: &Path, json: bool) -> i32 {
    let repo = match open_repo(db_path, json) {
        Some(r) => r,
        None => return exit_codes::GENERAL_ERROR,
    };

    let stats = match repo.stats() {
        Ok(s) => s,
        Err(e) => return print_error_and_exit(&format!("failed to read stats: {e}"), json),
    };

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&stats).unwrap_or_default()
        );
    } else {
        println!("Plan cache statistics:");
        println!("  Total entries : {}", stats.total_entries);
        println!("  Cache hits    : {}", stats.cache_hits);
        println!(
            "  Oldest entry  : {}",
            stats
                .oldest_entry_at
                .as_deref()
                .map(format_ts_compact)
                .unwrap_or_else(|| "(empty)".to_string())
        );
        println!(
            "  Newest entry  : {}",
            stats
                .newest_entry_at
                .as_deref()
                .map(format_ts_compact)
                .unwrap_or_else(|| "(empty)".to_string())
        );
    }
    exit_codes::SUCCESS
}

/// `apollia-os plan-cache clear [--force]`: remove all cached plans.
fn run_clear(db_path: &Path, force: bool, json: bool) -> i32 {
    if !force {
        print!("This will delete all cached plans. Continue? [y/N] ");
        let _ = io::stdout().flush();
        let mut input = String::new();
        if io::stdin().read_line(&mut input).is_err() {
            return print_error_and_exit("failed to read confirmation input", json);
        }
        if !input.trim().eq_ignore_ascii_case("y") {
            if json {
                println!("{}", serde_json::json!({"status": "cancelled"}));
            } else {
                println!("Cancelled.");
            }
            return exit_codes::SUCCESS;
        }
    }

    let repo = match open_repo(db_path, json) {
        Some(r) => r,
        None => return exit_codes::GENERAL_ERROR,
    };

    let deleted = match repo.clear_all() {
        Ok(n) => n,
        Err(e) => return print_error_and_exit(&format!("failed to clear cache: {e}"), json),
    };

    if json {
        println!(
            "{}",
            serde_json::json!({"deleted": deleted, "status": "ok"})
        );
    } else {
        println!("Plan cache cleared: {deleted} entries removed.");
    }
    exit_codes::SUCCESS
}

/// `apollia-os plan-cache evict [--max-age-days N]`: evict entries older than N days.
fn run_evict(db_path: &Path, max_age_days: u32, json: bool) -> i32 {
    let repo = match open_repo(db_path, json) {
        Some(r) => r,
        None => return exit_codes::GENERAL_ERROR,
    };

    let evicted = match repo.evict_expired(max_age_days) {
        Ok(n) => n,
        Err(e) => return print_error_and_exit(&format!("failed to evict entries: {e}"), json),
    };

    if json {
        println!(
            "{}",
            serde_json::json!({"evicted": evicted, "max_age_days": max_age_days, "status": "ok"})
        );
    } else {
        println!("Evicted {evicted} entries older than {max_age_days} days.");
    }
    exit_codes::SUCCESS
}

/// Open (or create) the plan cache database. Prints an error and returns `None` on failure.
fn open_repo(db_path: &Path, json: bool) -> Option<PlanCacheRepository> {
    if let Some(parent) = db_path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            print_error_and_exit(
                &format!("cannot create data directory {}: {e}", parent.display()),
                json,
            );
            return None;
        }
    }
    match PlanCacheRepository::open(db_path) {
        Ok(r) => Some(r),
        Err(e) => {
            print_error_and_exit(
                &format!("cannot open plan_cache.db at {}: {e}", db_path.display()),
                json,
            );
            None
        }
    }
}

/// Resolve `~/.apollia/` data directory.
fn apollia_data_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".apollia")
}

fn print_error_and_exit(msg: &str, json: bool) -> i32 {
    if json {
        println!("{}", serde_json::json!({"error": msg}));
    } else {
        eprintln!("Error: {msg}");
    }
    exit_codes::GENERAL_ERROR
}

/// Render an RFC3339 timestamp as `YYYY-MM-DD HH:MM:SS`.
///
/// Falls back to the raw string on parse failure so the operator still
/// sees something usable when the stored value is malformed.
fn format_ts_compact(raw: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(raw)
        .map(|dt| {
            dt.with_timezone(&chrono::Utc)
                .format("%Y-%m-%d %H:%M:%S")
                .to_string()
        })
        .unwrap_or_else(|_| raw.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Debug, Parser)]
    struct TestCli {
        #[command(subcommand)]
        cmd: PlanCacheCommand,
    }

    #[test]
    fn test_plan_cache_stats_parses() {
        // GIVEN "stats"
        // WHEN parsed
        let cli = TestCli::parse_from(["test", "stats"]);
        // THEN PlanCacheCommand::Stats
        assert!(matches!(cli.cmd, PlanCacheCommand::Stats));
    }

    #[test]
    fn test_plan_cache_clear_with_force_parses() {
        // GIVEN "clear --force"
        // WHEN parsed
        let cli = TestCli::parse_from(["test", "clear", "--force"]);
        // THEN PlanCacheCommand::Clear { force: true }
        assert!(matches!(cli.cmd, PlanCacheCommand::Clear { force: true }));
    }

    #[test]
    fn test_plan_cache_clear_default_no_force() {
        // GIVEN "clear" without --force
        // WHEN parsed
        let cli = TestCli::parse_from(["test", "clear"]);
        // THEN force defaults to false
        assert!(matches!(cli.cmd, PlanCacheCommand::Clear { force: false }));
    }

    #[test]
    fn test_plan_cache_evict_default_age() {
        // GIVEN "evict" without --max-age-days
        // WHEN parsed
        let cli = TestCli::parse_from(["test", "evict"]);
        // THEN max_age_days defaults to 7
        match cli.cmd {
            PlanCacheCommand::Evict { max_age_days } => assert_eq!(max_age_days, 7),
            other => panic!("expected Evict, got {other:?}"),
        }
    }

    #[test]
    fn test_plan_cache_evict_custom_age() {
        // GIVEN "evict --max-age-days 14"
        // WHEN parsed
        let cli = TestCli::parse_from(["test", "evict", "--max-age-days", "14"]);
        // THEN max_age_days is 14
        match cli.cmd {
            PlanCacheCommand::Evict { max_age_days } => assert_eq!(max_age_days, 14),
            other => panic!("expected Evict, got {other:?}"),
        }
    }

    #[test]
    fn test_stats_json_output_format() {
        // GIVEN a PlanCacheStats value with 10 entries and 25 hits
        let stats = apollia_oria::plan_cache::PlanCacheStats {
            total_entries: 10,
            cache_hits: 25,
            oldest_entry_at: Some("2026-04-01T00:00:00".to_string()),
            newest_entry_at: Some("2026-04-25T12:00:00".to_string()),
        };
        // WHEN serialized to JSON
        let output = serde_json::to_string_pretty(&stats).unwrap();
        // THEN output contains all expected fields
        assert!(output.contains("\"total_entries\": 10"));
        assert!(output.contains("\"cache_hits\": 25"));
        assert!(output.contains("oldest_entry_at"));
        assert!(output.contains("newest_entry_at"));
    }

    #[test]
    fn test_clear_force_on_fresh_db_returns_zero() {
        // GIVEN a fresh (empty) plan_cache.db
        let dir = tempfile::tempdir().expect("create tmpdir");
        let db_path = dir.path().join("plan_cache.db");
        // WHEN clear --force is called
        let code = run_clear(&db_path, true, false);
        // THEN exit code is SUCCESS and 0 entries deleted (printed but not checked here)
        assert_eq!(code, exit_codes::SUCCESS);
    }

    #[test]
    fn test_clear_force_with_entries_returns_success() {
        // GIVEN a plan_cache.db with 2 stored entries
        let dir = tempfile::tempdir().expect("create tmpdir");
        let db_path = dir.path().join("plan_cache.db");
        let repo = PlanCacheRepository::open(&db_path).expect("open");
        let plan = apollia_oria::plan::ExecutionPlan {
            plan_id: "p1".to_string(),
            task_id: "t1".to_string(),
            steps: vec![],
        };
        repo.store("k1", &plan, "agent", "1.0").expect("store k1");
        repo.store("k2", &plan, "agent", "1.0").expect("store k2");
        drop(repo);
        // WHEN clear --force is called
        let code = run_clear(&db_path, true, false);
        // THEN exit code is SUCCESS
        assert_eq!(code, exit_codes::SUCCESS);
        // AND the DB is now empty
        let repo2 = PlanCacheRepository::open(&db_path).expect("open2");
        let stats = repo2.stats().expect("stats");
        assert_eq!(stats.total_entries, 0);
    }

    #[test]
    fn test_stats_on_fresh_db_returns_zeros() {
        // GIVEN a fresh plan_cache.db
        let dir = tempfile::tempdir().expect("create tmpdir");
        let db_path = dir.path().join("plan_cache.db");
        // WHEN stats is called in JSON mode
        let code = run_stats(&db_path, true);
        // THEN exit code is SUCCESS
        assert_eq!(code, exit_codes::SUCCESS);
    }

    #[test]
    fn test_evict_on_fresh_db_returns_success() {
        // GIVEN a fresh plan_cache.db
        let dir = tempfile::tempdir().expect("create tmpdir");
        let db_path = dir.path().join("plan_cache.db");
        // WHEN evict with default age is called
        let code = run_evict(&db_path, 7, false);
        // THEN exit code is SUCCESS
        assert_eq!(code, exit_codes::SUCCESS);
    }

    #[test]
    fn test_data_dir_ends_with_apollia() {
        // GIVEN the HOME env var
        // WHEN apollia_data_dir is called
        let dir = apollia_data_dir();
        // THEN path ends with .apollia
        assert!(dir.to_string_lossy().ends_with(".apollia"));
    }
}
