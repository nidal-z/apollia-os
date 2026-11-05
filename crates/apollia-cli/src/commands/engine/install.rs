//! `engine install` and `engine import`: start an installation in the runtime,
//! then follow it until it ends.
//!
//! The runtime answers the start at once with where the installation stands,
//! and the CLI reads that state again every [`POLL`]: a bar on a terminal, one
//! line per change of state elsewhere, nothing but the final document under
//! `--json`. Ctrl-C asks the runtime to cancel the download
//! (`DELETE /api/v1/engines/installation`) and exits 5.
//!
//! Exit codes: an installed engine is 0; a failed download is 3; a directory
//! the runtime refuses to import (no manifest, a refused signature, an archive
//! that does not match) is 2, with the message of the runtime.

use std::future::Future;
use std::io::IsTerminal;
use std::path::Path;
use std::time::Duration;

use serde_json::{json, Value};

use super::{get_json, print_document, refusal, transport, InstallEngine, ENGINES_ROUTE};
use crate::client::RuntimeClient;
use crate::exit_codes;

/// The variants of llama-server a build of the channel may carry.
pub(crate) const VARIANTS: [&str; 6] = ["cuda-12", "cuda-13", "rocm", "vulkan", "cpu", "metal"];
/// The followed installation of the store.
pub(crate) const INSTALLATION_ROUTE: &str = "/api/v1/engines/installation";
/// The import of a directory.
pub(crate) const IMPORT_ROUTE: &str = "/api/v1/engines/installation/import";
/// The download of Node.js.
pub(crate) const NODE_ROUTE: &str = "/api/v1/engines/nodejs";
/// How often an installation is read again.
const POLL: Duration = Duration::from_millis(500);
/// How an operator follows a detached installation.
pub(crate) const FOLLOW_HINT: &str = "`apollia-os engine list` shows it once installed";

/// What `engine install` was asked for.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Wanted<'a> {
    pub(crate) engine: InstallEngine,
    pub(crate) variant: Option<&'a str>,
    pub(crate) detach: bool,
    pub(crate) force: bool,
}

/// Resolves on Ctrl-C, and never when the signal cannot be listened for.
pub(crate) async fn interrupted() {
    if tokio::signal::ctrl_c().await.is_err() {
        std::future::pending::<()>().await;
    }
}

/// Where an installation is followed from: the runtime, the output mode, and
/// how often the state is read again.
#[derive(Clone, Copy)]
pub(crate) struct Follower<'a> {
    pub(crate) client: &'a RuntimeClient,
    pub(crate) json: bool,
    pub(crate) poll: Duration,
}

impl<'a> Follower<'a> {
    /// The follower of the commands, reading again every [`POLL`].
    pub(crate) fn new(client: &'a RuntimeClient, json: bool) -> Self {
        Self {
            client,
            json,
            poll: POLL,
        }
    }
}

/// The installation a command follows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Job {
    /// A variant downloaded from the channel.
    Download,
    /// The archives of a directory.
    Import,
    /// Node.js, which has a download of its own.
    Node,
}

impl Job {
    fn state_route(self) -> &'static str {
        match self {
            Self::Download | Self::Import => INSTALLATION_ROUTE,
            Self::Node => NODE_ROUTE,
        }
    }

    /// The exit code of a failure: a refused directory is the runtime
    /// refusing its input, a failed download a task that failed.
    fn failed_code(self) -> i32 {
        match self {
            Self::Import => exit_codes::RUNTIME_ERROR,
            Self::Download | Self::Node => exit_codes::TASK_FAILED,
        }
    }
}

/// Where an installation stands, whichever route says it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Step {
    /// Working on something that has no measure: the signature, the extraction.
    Working(&'static str),
    /// The archive arriving.
    Downloading { received: u64, total: u64 },
    /// Installed.
    Done,
    /// Refused or failed, with the message of the runtime.
    Failed(String),
    /// Nothing running any more, and no outcome: cancelled elsewhere.
    Gone,
}

impl Step {
    fn label(&self) -> &'static str {
        match self {
            Self::Working(label) => label,
            Self::Downloading { .. } => "downloading",
            Self::Done => "installed",
            Self::Failed(_) => "failed",
            Self::Gone => "cancelled",
        }
    }
}

fn str_of<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str)
}

fn u64_of(v: &Value, key: &str) -> u64 {
    v.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn failure(doc: &Value) -> Step {
    let message = str_of(doc, "error")
        .filter(|e| !e.is_empty())
        .or_else(|| str_of(doc, "code"))
        .unwrap_or("the installation failed");
    Step::Failed(message.to_owned())
}

/// The step a state of `job` says.
pub(crate) fn step_of(job: Job, doc: &Value) -> Step {
    let state = str_of(doc, "state").unwrap_or("");
    match (job, state) {
        (_, "downloading") => Step::Downloading {
            received: u64_of(doc, "received"),
            total: u64_of(doc, "total"),
        },
        (_, "failed") => failure(doc),
        (Job::Node, "ready") | (Job::Download | Job::Import, "installed") => Step::Done,
        (Job::Download | Job::Import, "verifying") => Step::Working("verifying the signature"),
        (Job::Download | Job::Import, "installing") => Step::Working("installing"),
        _ => Step::Gone,
    }
}

/// What the operator sees of the steps: a bar on a terminal, a line per
/// change of state elsewhere, nothing under `--json`.
struct Progress {
    json: bool,
    bar: Option<indicatif::ProgressBar>,
    last: Option<&'static str>,
}

impl Progress {
    fn new(json: bool) -> Self {
        let bar = (!json && std::io::stderr().is_terminal()).then(|| {
            let bar = indicatif::ProgressBar::new_spinner();
            bar.enable_steady_tick(Duration::from_millis(120));
            bar
        });
        Self {
            json,
            bar,
            last: None,
        }
    }

    fn show(&mut self, step: &Step) {
        if self.json {
            return;
        }
        let label = step.label();
        if let Some(bar) = &self.bar {
            match step {
                Step::Downloading { received, total } if *total > 0 => {
                    if self.last != Some(label) {
                        bar.set_style(
                            indicatif::ProgressStyle::with_template(
                                "{msg} [{bar:30}] {bytes}/{total_bytes} {bytes_per_sec}",
                            )
                            .unwrap_or_else(|_| indicatif::ProgressStyle::default_bar()),
                        );
                    }
                    bar.set_length(*total);
                    bar.set_position(*received);
                }
                _ => bar.set_style(
                    indicatif::ProgressStyle::with_template("{spinner} {msg}")
                        .unwrap_or_else(|_| indicatif::ProgressStyle::default_spinner()),
                ),
            }
            bar.set_message(label);
        } else if self.last != Some(label) {
            match step {
                Step::Downloading { total, .. } if *total > 0 => {
                    crate::note!("{label} ({total} bytes)");
                }
                _ => crate::note!("{label}"),
            }
        }
        self.last = Some(label);
    }

    fn finish(&self) {
        if let Some(bar) = &self.bar {
            bar.finish_and_clear();
        }
    }
}

/// Follow `job` from its first state until it ends, reading it again every
/// `poll`; `interrupt` resolving cancels it.
pub(crate) async fn follow(
    at: Follower<'_>,
    job: Job,
    first: Value,
    what: &str,
    interrupt: impl Future<Output = ()>,
) -> i32 {
    let Follower { client, json, poll } = at;
    let mut progress = Progress::new(json);
    let mut doc = first;
    tokio::pin!(interrupt);
    loop {
        let step = step_of(job, &doc);
        progress.show(&step);
        match step {
            Step::Done => {
                progress.finish();
                if json {
                    print_document(&doc);
                } else {
                    crate::note!("{what}: installed. The next model load can take it.");
                }
                return exit_codes::SUCCESS;
            }
            Step::Failed(message) => {
                progress.finish();
                return crate::output::emit_error(json, job.failed_code(), &message);
            }
            Step::Gone => {
                progress.finish();
                return crate::output::emit_error(
                    json,
                    exit_codes::INTERRUPTED,
                    &format!("{what}: the installation was cancelled"),
                );
            }
            Step::Working(_) | Step::Downloading { .. } => {}
        }
        tokio::select! {
            () = &mut interrupt => {
                progress.finish();
                return cancel(client, job, what, json).await;
            }
            () = tokio::time::sleep(poll) => {}
        }
        doc = match get_json(client, job.state_route(), json).await {
            Ok(doc) => doc,
            Err(code) => {
                progress.finish();
                return code;
            }
        };
    }
}

/// Ctrl-C: ask the runtime to cancel the installation, then exit 5. A
/// verification or an extraction under way is left to end, and Node.js has
/// no cancellation: the message says so.
pub(crate) async fn cancel(client: &RuntimeClient, job: Job, what: &str, json: bool) -> i32 {
    let message = if job == Job::Node {
        format!("{what}: interrupted; the download goes on in the runtime, {FOLLOW_HINT}")
    } else {
        match client.delete(INSTALLATION_ROUTE).await {
            Ok(resp) if resp.status < 400 => {
                let state: Value = serde_json::from_str(&resp.body).unwrap_or(Value::Null);
                match str_of(&state, "state") {
                    Some("idle") | None => format!("{what}: cancelled, nothing was installed"),
                    Some(other) => {
                        format!("{what}: interrupted; the runtime ends the {other} step on its own")
                    }
                }
            }
            Ok(resp) => format!(
                "{what}: interrupted; the runtime refused the cancellation (status {})",
                resp.status
            ),
            Err(e) => {
                format!("{what}: interrupted; the cancellation did not reach the runtime: {e}")
            }
        }
    };
    crate::output::emit_error(json, exit_codes::INTERRUPTED, &message)
}

/// The answer of a start: its state, or the exit code of its failure.
async fn start(
    client: &RuntimeClient,
    route: &str,
    body: Option<&Value>,
    json: bool,
) -> Result<Value, i32> {
    let resp = client
        .post(route, body)
        .await
        .map_err(|e| transport(json, &e))?;
    if resp.status >= 400 {
        return Err(refusal(json, &resp));
    }
    serde_json::from_str(&resp.body).map_err(|e| {
        crate::output::emit_error(
            json,
            exit_codes::RUNTIME_ERROR,
            &format!("invalid JSON response: {e}"),
        )
    })
}

/// `--detach`: the installation is accepted, unless the start itself failed.
fn detached(job: Job, state: &Value, what: &str, json: bool) -> i32 {
    if let Step::Failed(message) = step_of(job, state) {
        return crate::output::emit_error(json, job.failed_code(), &message);
    }
    if json {
        print_document(
            &json!({ "detached": true, "state": state, "follow": "apollia-os engine list" }),
        );
    } else {
        crate::note!("{what}: started in the runtime; {FOLLOW_HINT}.");
    }
    exit_codes::SUCCESS
}

/// The variant to install: the one named, refused when rated incompatible
/// without `force`, or the one rated recommended.
pub(crate) fn choose_variant(
    doc: &Value,
    named: Option<&str>,
    force: bool,
) -> Result<String, String> {
    let ratings = doc
        .get("ratings")
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice);
    let Some(variant) = named else {
        return ratings
            .iter()
            .find(|r| str_of(r, "verdict") == Some("recommended"))
            .and_then(|r| str_of(r, "variant"))
            .map(str::to_owned)
            .ok_or_else(|| {
                "no variant is rated recommended for this machine; name one (`apollia-os engine list --all` lists them)".to_owned()
            });
    };
    let rating = ratings
        .iter()
        .find(|r| str_of(r, "variant") == Some(variant));
    if !force {
        if let Some(rating) = rating.filter(|r| str_of(r, "verdict") == Some("incompatible")) {
            let why = rating.get("reason").filter(|r| !r.is_null()).map_or_else(
                || "rated incompatible".to_owned(),
                super::render::reason_text,
            );
            return Err(format!(
                "the {variant} build does not run on this machine ({why}); add --force to install it anyway"
            ));
        }
    }
    Ok(variant.to_owned())
}

/// `engine install <engine> [<variant>]`.
pub(crate) async fn install(
    client: &RuntimeClient,
    wanted: Wanted<'_>,
    json: bool,
    interrupt: impl Future<Output = ()>,
) -> i32 {
    install_with(Follower::new(client, json), wanted, interrupt).await
}

/// [`install`], followed from `at`.
pub(crate) async fn install_with(
    at: Follower<'_>,
    wanted: Wanted<'_>,
    interrupt: impl Future<Output = ()>,
) -> i32 {
    let Follower { client, json, .. } = at;
    if wanted.engine == InstallEngine::Nodejs {
        if wanted.variant.is_some() {
            return crate::output::emit_error(
                json,
                exit_codes::GENERAL_ERROR,
                "Node.js has one build per platform: name no variant",
            );
        }
        let what = "nodejs";
        let state = match start(client, NODE_ROUTE, None, json).await {
            Ok(state) => state,
            Err(code) => return code,
        };
        if wanted.detach {
            return detached(Job::Node, &state, what, json);
        }
        return follow(at, Job::Node, state, what, interrupt).await;
    }
    let engines = match get_json(client, ENGINES_ROUTE, json).await {
        Ok(doc) => doc,
        Err(code) => return code,
    };
    let variant = match choose_variant(&engines, wanted.variant, wanted.force) {
        Ok(variant) => variant,
        Err(message) => {
            return crate::output::emit_error(json, exit_codes::GENERAL_ERROR, &message);
        }
    };
    let what = format!("llama-server {variant}");
    let body = json!({ "engine": "llama-server", "variant": variant });
    let state = match start(client, INSTALLATION_ROUTE, Some(&body), json).await {
        Ok(state) => state,
        Err(code) => return code,
    };
    if wanted.detach {
        return detached(Job::Download, &state, &what, json);
    }
    follow(at, Job::Download, state, &what, interrupt).await
}

/// `engine import <dir>`.
pub(crate) async fn import(
    client: &RuntimeClient,
    dir: &Path,
    detach: bool,
    json: bool,
    interrupt: impl Future<Output = ()>,
) -> i32 {
    import_with(Follower::new(client, json), dir, detach, interrupt).await
}

/// [`import`], followed from `at`.
pub(crate) async fn import_with(
    at: Follower<'_>,
    dir: &Path,
    detach: bool,
    interrupt: impl Future<Output = ()>,
) -> i32 {
    let Follower { client, json, .. } = at;
    // The runtime reads the directory from its own working directory: it is
    // handed the absolute path.
    let absolute = std::path::absolute(dir).unwrap_or_else(|_| dir.to_path_buf());
    let what = format!("the engines of {}", absolute.display());
    let body = json!({ "path": absolute.display().to_string() });
    let state = match start(client, IMPORT_ROUTE, Some(&body), json).await {
        Ok(state) => state,
        Err(code) => return code,
    };
    if detach {
        return detached(Job::Import, &state, &what, json);
    }
    follow(at, Job::Import, state, &what, interrupt).await
}
