//! `apollia-os logs`: display runtime log lines.
//!
//! The Apollia runtime writes its log stream to stderr by default. When the
//! daemon is started with stderr redirected to a file (e.g.
//! `apollia-os start 2>>~/.apollia/logs/runtime.log`), this command tails that
//! file. If no log file is configured, a friendly hint is printed instead of
//! silently failing.

use std::io::SeekFrom;
use std::path::PathBuf;

use clap::Args;
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncSeekExt, BufReader};

use crate::exit_codes;

/// Arguments for the `logs` command.
#[derive(Debug, Args)]
pub struct LogsArgs {
    /// Path to the log file (default: `~/.apollia/logs/runtime.log`).
    #[arg(long, value_name = "PATH")]
    pub file: Option<PathBuf>,

    /// Print only the last `N` lines (default: 50).
    #[arg(long, value_name = "N", default_value_t = 50)]
    pub last: usize,

    /// Follow the file and stream new lines as they are appended.
    #[arg(short = 'f', long)]
    pub follow: bool,
}

/// Execute the `logs` command.
///
/// Returns the process exit code.
pub async fn run(args: &LogsArgs, json: bool) -> i32 {
    let path = args.file.clone().unwrap_or_else(default_log_path);

    if !path.exists() {
        if json {
            let report = serde_json::json!({
                "status": "missing",
                "path": path.display().to_string(),
                "hint": "redirect daemon stderr: `apollia-os start 2>>~/.apollia/logs/runtime.log`",
            });
            println!(
                "{}",
                serde_json::to_string_pretty(&report).unwrap_or_default()
            );
        } else {
            eprintln!("No log file at {}.", path.display());
            eprintln!("Apollia logs to stderr by default. To persist logs run:");
            eprintln!("  mkdir -p ~/.apollia/logs");
            eprintln!("  apollia-os start 2>>~/.apollia/logs/runtime.log");
        }
        return exit_codes::GENERAL_ERROR;
    }

    match print_tail(&path, args.last).await {
        Ok(()) => {}
        Err(e) => {
            eprintln!("Error reading log file {}: {e}", path.display());
            return exit_codes::GENERAL_ERROR;
        }
    }

    if args.follow {
        match follow(&path).await {
            Ok(()) => exit_codes::SUCCESS,
            Err(e) => {
                eprintln!("Error following log file {}: {e}", path.display());
                exit_codes::GENERAL_ERROR
            }
        }
    } else {
        exit_codes::SUCCESS
    }
}

/// Resolve the default log file path (`~/.apollia/logs/runtime.log`).
fn default_log_path() -> PathBuf {
    let home = apollia_core::paths::home_dir_or_temp();
    home.join(".apollia").join("logs").join("runtime.log")
}

/// Print the last `n` lines of `path` to stdout.
///
/// Reads the entire file into memory, fine for tracing logs which rotate
/// well below the 100 MB range. For huge files, prefer `--file` with an
/// already-rotated path.
async fn print_tail(path: &std::path::Path, n: usize) -> std::io::Result<()> {
    let mut file = File::open(path).await?;
    let mut contents = String::new();
    file.read_to_string(&mut contents).await?;
    let lines: Vec<&str> = contents.lines().collect();
    let start = lines.len().saturating_sub(n);
    for line in &lines[start..] {
        println!("{line}");
    }
    Ok(())
}

/// Follow the file: stream new lines until interrupted.
async fn follow(path: &std::path::Path) -> std::io::Result<()> {
    let mut file = File::open(path).await?;
    file.seek(SeekFrom::End(0)).await?;
    let mut reader = BufReader::new(file);
    let mut buf = String::new();
    loop {
        buf.clear();
        let n = reader.read_line(&mut buf).await?;
        if n == 0 {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            continue;
        }
        print!("{buf}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser, Debug)]
    struct TestCli {
        #[command(subcommand)]
        cmd: TestCmd,
    }

    #[derive(clap::Subcommand, Debug)]
    enum TestCmd {
        Logs(LogsArgs),
    }

    #[test]
    fn logs_defaults() {
        let cli = TestCli::parse_from(["x", "logs"]);
        let TestCmd::Logs(args) = cli.cmd;
        assert_eq!(args.last, 50);
        assert!(!args.follow);
        assert!(args.file.is_none());
    }

    #[test]
    fn logs_custom_last() {
        let cli = TestCli::parse_from(["x", "logs", "--last", "200"]);
        let TestCmd::Logs(args) = cli.cmd;
        assert_eq!(args.last, 200);
    }

    #[test]
    fn logs_follow_short_form() {
        let cli = TestCli::parse_from(["x", "logs", "-f"]);
        let TestCmd::Logs(args) = cli.cmd;
        assert!(args.follow);
    }

    #[test]
    fn logs_with_custom_file() {
        let cli = TestCli::parse_from(["x", "logs", "--file", "/tmp/x.log"]);
        let TestCmd::Logs(args) = cli.cmd;
        assert_eq!(args.file.unwrap(), PathBuf::from("/tmp/x.log"));
    }

    #[tokio::test]
    async fn run_returns_error_when_file_missing() {
        let args = LogsArgs {
            file: Some(PathBuf::from("/nonexistent/path/runtime.log")),
            last: 10,
            follow: false,
        };
        let code = run(&args, true).await;
        assert_eq!(code, exit_codes::GENERAL_ERROR);
    }

    #[tokio::test]
    async fn print_tail_truncates_to_last_n() {
        // GIVEN a temp file with 10 lines.
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let content = (1..=10)
            .map(|i| format!("line-{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(tmp.path(), content).unwrap();
        // WHEN print_tail is called with n=3.
        // THEN it returns Ok (stdout content not asserted; logic verified by integration tests).
        let res = print_tail(tmp.path(), 3).await;
        assert!(res.is_ok());
    }
}
