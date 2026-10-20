//! The engines this runtime started, recorded so that the next start stops
//! those a killed process left behind.
//!
//! Each engine is started with `kill_on_drop`, which stops it when the
//! runtime ends cleanly. A runtime that is killed (a development rebuild, a
//! crash, a forced quit) drops nothing: its MLX engines, llama-server
//! instances and runners kept running with no owner, twenty gigabytes each
//! for a large model, and the next start loaded the model a second time
//! beside them. Every engine is written here with the process that started
//! it; at start, an engine whose owner is gone, and which is still one of our
//! engines, is stopped. An engine whose owner still runs (the desktop while a
//! command of the CLI starts its own runtime) is never touched.

use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

/// The record, one `engine_pid owner_pid` line per engine started.
const FILE_NAME: &str = "engine-pids";

/// Words one of our engines carries on its command line. Only the unix
/// `is_our_engine` reads a command line; Windows stops nothing yet.
#[cfg(unix)]
const ENGINE_MARKERS: [&str; 3] = ["vllm_mlx", "llama-server", "apollia-runner"];

/// Serialises the writes of this process to the record.
static RECORD: Mutex<()> = Mutex::new(());

/// The record of this machine, under the data directory. A test run keeps
/// none: the engines its tests start are theirs to stop, and the data
/// directory of a test run is the home of the person running it.
fn record_path() -> Option<PathBuf> {
    if cfg!(test) {
        return None;
    }
    apollia_core::paths::data_dir().map(|dir| dir.join(FILE_NAME))
}

/// Writes down the engine `pid`, started by this process.
pub(crate) fn record(pid: Option<u32>) {
    let (Some(pid), Some(path)) = (pid, record_path()) else {
        return;
    };
    let _guard = RECORD
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let written = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .and_then(|mut file| writeln!(file, "{pid} {}", std::process::id()));
    if let Err(e) = written {
        tracing::warn!(error = %e, pid, "engine.pids.record_failed");
    }
}

/// Stops the engines a runtime that is gone left running, and keeps in the
/// record those whose owner still runs. Returns how many were stopped.
pub fn reap_orphans() -> usize {
    let Some(path) = record_path() else {
        return 0;
    };
    let _guard = RECORD
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return 0;
    };
    let mut kept = String::new();
    let mut stopped = 0;
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        let (Some(Ok(engine)), Some(Ok(owner))) = (
            fields.next().map(str::parse::<u32>),
            fields.next().map(str::parse::<u32>),
        ) else {
            continue;
        };
        if !is_alive(engine) {
            continue;
        }
        if is_alive(owner) {
            kept.push_str(line);
            kept.push('\n');
            continue;
        }
        if is_our_engine(engine) {
            stop(engine);
            stopped += 1;
            tracing::info!(pid = engine, owner, "engine.orphan.stopped");
        }
    }
    if let Err(e) = std::fs::write(&path, kept) {
        tracing::warn!(error = %e, "engine.pids.rewrite_failed");
    }
    stopped
}

#[cfg(unix)]
fn is_alive(pid: u32) -> bool {
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(unix)]
fn is_our_engine(pid: u32) -> bool {
    std::process::Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "command="])
        .output()
        .ok()
        .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
        .is_some_and(|command| ENGINE_MARKERS.iter().any(|marker| command.contains(marker)))
}

#[cfg(unix)]
fn stop(pid: u32) {
    // An orphan engine holds no state worth a clean exit, and waiting for one
    // would hold the start: it is ended at once.
    let _ = std::process::Command::new("kill")
        .args(["-KILL", &pid.to_string()])
        .status();
}

// Windows ends the children of a killed process with its job object only when
// one is set up; until then the record is kept and nothing is stopped there.
#[cfg(not(unix))]
fn is_alive(_pid: u32) -> bool {
    false
}

#[cfg(not(unix))]
fn is_our_engine(_pid: u32) -> bool {
    false
}

#[cfg(not(unix))]
fn stop(_pid: u32) {}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn test_an_engine_command_line_is_told_from_another_process() {
        // GIVEN this test process, whose command line names no engine
        let me = std::process::id();

        // WHEN it is checked
        // THEN it is alive, and it is not taken for one of our engines
        assert!(is_alive(me));
        assert!(!is_our_engine(me));
    }

    #[test]
    fn test_a_process_that_does_not_exist_is_not_alive() {
        // GIVEN a process id far above any live one
        // WHEN it is checked
        // THEN it is not alive, so a stale line is dropped
        assert!(!is_alive(4_000_000));
    }
}
