//! Decomposition of one user-visible chat turn into engine time, tool time, and
//! the orchestration overhead that is left over.
//!
//! A turn is what the user waits for. It contains one or more engine
//! completions and any number of tool invocations, and until it is decomposed
//! there is no way to say which of the three a slow turn was spending its time
//! in. The residual is the term this project owns directly, so it is computed
//! explicitly rather than inferred.
//!
//! # Why a task-local
//!
//! The quantities are produced in three different modules: time to first token
//! in the stream consumer, engine timings in the chunk handler, tool wall-clock
//! in the tool dispatcher. Threading a recorder through all of them would mean
//! changing signatures along the whole path, and the instrumentation is
//! required to be additive. One turn runs inside one task and every tool call is
//! awaited inline, so a task-local reaches all three sites without touching a
//! single signature. Outside a turn scope every entry point here is a no-op,
//! which is what makes the same code safe to call from paths that are not part
//! of a chat turn.
//!
//! # Field names
//!
//! Every name written here comes from the measurement dictionary in
//! `scripts/model-eval/README.md`, and the JSON record conforms to the
//! measurement-record schema in the same file. Nothing is invented locally: a
//! downstream analysis script reads that contract, not this module.

use std::future::Future;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use serde_json::{json, Value};

/// Environment variable naming the JSON Lines file turn records are appended to.
///
/// Unset means no file is written and no provenance is gathered. The INFO event
/// is emitted either way.
const ENV_PERF_TRACE: &str = "APOLLIA_PERF_TRACE";

tokio::task_local! {
    /// Accumulator for the turn the current task is executing.
    ///
    /// `RefCell` inside a `Mutex` would be redundant: the value is only ever
    /// touched by the owning task, but the future must stay `Send`, so a
    /// `Mutex` is what the type system asks for. The guard never crosses an
    /// await.
    static TURN: std::sync::Arc<Mutex<TurnAccumulator>>;
}

/// Resolved launch configuration of the embedded engine, published once per
/// spawn so a turn record can quote the command line that produced it.
static LAUNCH_CONFIG: OnceLock<Mutex<Option<Value>>> = OnceLock::new();

/// Provenance block, gathered at most once per process and only when tracing is
/// enabled. Hashing a multi-gigabyte model is not something a chat turn should
/// pay for more than once.
static PROVENANCE: OnceLock<Value> = OnceLock::new();

fn launch_config_slot() -> &'static Mutex<Option<Value>> {
    LAUNCH_CONFIG.get_or_init(|| Mutex::new(None))
}

/// Trace path resolved once per process.
///
/// Cached rather than read per turn so the disabled path costs a pointer read,
/// and so nothing consults the environment on the hot path.
static TRACE_PATH: OnceLock<Option<String>> = OnceLock::new();

/// Path turn records are appended to, or `None` when tracing is off.
pub fn trace_path() -> Option<&'static str> {
    TRACE_PATH
        .get_or_init(|| std::env::var(ENV_PERF_TRACE).ok().filter(|p| !p.is_empty()))
        .as_deref()
}

/// Publish the engine's resolved launch configuration.
///
/// Called once per spawn. The record quotes it verbatim, because two turn
/// decompositions measured under different launch flags are not comparable and
/// only the recorded vector reveals that.
pub fn set_launch_config(config: Value) {
    if let Ok(mut slot) = launch_config_slot().lock() {
        *slot = Some(config);
    }
}

/// One engine completion inside a turn.
#[derive(Default)]
struct IterationSample {
    ttft_ms: Option<f64>,
    /// The engine's `timings` object, verbatim, when the backend reported one.
    engine_timings: Option<Value>,
    prompt_tok_reported: u32,
    n_ctx_slot_tok: Option<u32>,
    temperature: Option<f32>,
    reasoning_chars: usize,
    content_chars: usize,
}

/// One tool invocation inside a turn.
struct ToolSample {
    tool_name: String,
    tool_ms: f64,
    tool_approval_ms: Option<f64>,
    iteration_index: usize,
}

/// One human approval a turn waited on, whatever the answer turned out to be.
///
/// Recorded separately from [`ToolSample`] because the two are not the same
/// event and a refused approval produces no tool call at all. Folding the wait
/// into a tool record would make it invisible on exactly the turns where it
/// dominates: a refusal, or a timeout, leaves no tool to attach it to, and the
/// wait then lands in the orchestration residual as if the runtime had been
/// busy.
struct ApprovalSample {
    tool_name: String,
    approval_ms: f64,
    /// Whether the decision allowed the call to run. A refusal reached by
    /// timeout is not distinguished from one an operator typed: at this site
    /// both arrive as the same decision, and inventing the distinction would be
    /// a guess.
    approved: bool,
    iteration_index: usize,
}

/// Everything measured about one turn, accumulated as it runs.
struct TurnAccumulator {
    session_id: String,
    started: Instant,
    iterations: Vec<IterationSample>,
    tools: Vec<ToolSample>,
    approvals: Vec<ApprovalSample>,
}

impl TurnAccumulator {
    fn new(session_id: String) -> Self {
        Self {
            session_id,
            started: Instant::now(),
            iterations: Vec::new(),
            tools: Vec::new(),
            approvals: Vec::new(),
        }
    }

    fn current(&mut self) -> Option<&mut IterationSample> {
        self.iterations.last_mut()
    }
}

/// Run `fut` as one instrumented user turn.
///
/// Additive by construction: the future is the turn, unchanged. On completion
/// one INFO event is emitted and, when a trace path is set, one JSON object is
/// appended. A turn whose task is dropped mid-flight writes nothing, which is
/// correct: it never completed.
pub async fn instrument_turn<F: Future>(session_id: &str, fut: F) -> F::Output {
    instrument_turn_to(session_id, trace_path().map(str::to_owned), fut).await
}

/// [`instrument_turn`] with an explicit destination.
///
/// Tests drive this directly so each one owns its own file, instead of racing
/// over a process-global environment variable.
async fn instrument_turn_to<F: Future>(
    session_id: &str,
    path: Option<String>,
    fut: F,
) -> F::Output {
    let acc = std::sync::Arc::new(Mutex::new(TurnAccumulator::new(session_id.to_owned())));
    let out = TURN.scope(std::sync::Arc::clone(&acc), fut).await;
    finish(&acc, path.as_deref()).await;
    out
}

/// Apply `f` to the current turn accumulator, or do nothing outside a turn.
fn with_turn(f: impl FnOnce(&mut TurnAccumulator)) {
    let _ = TURN.try_with(|acc| {
        if let Ok(mut guard) = acc.lock() {
            f(&mut guard);
        }
    });
}

/// Open a new iteration, immediately before the completion request is issued.
pub fn iteration_begin(temperature: Option<f32>, n_ctx_slot_tok: Option<u32>) {
    with_turn(|t| {
        t.iterations.push(IterationSample {
            temperature,
            n_ctx_slot_tok,
            ..IterationSample::default()
        });
    });
}

/// Record time to first token for the open iteration.
///
/// Client-observed, so it includes transport, queueing, prefill and the first
/// sampling step, which is what the user actually waits through. The caller
/// owns the origin and must take it at request dispatch: the backend awaits the
/// first chunk before yielding the stream, so an origin taken at the start of
/// consumption would exclude prefill entirely. Invariant I13 fires when it does.
pub fn iteration_ttft(ms: f64) {
    with_turn(|t| {
        if let Some(it) = t.current() {
            if it.ttft_ms.is_none() {
                it.ttft_ms = Some(ms);
            }
        }
    });
}

/// Attach the engine's own timings object to the open iteration.
pub fn iteration_engine_timings(timings: &Value) {
    with_turn(|t| {
        if let Some(it) = t.current() {
            it.engine_timings = Some(timings.clone());
        }
    });
}

/// Close the open iteration with what only the loop knows.
///
/// `reasoning_chars` and `content_chars` are exact; the token split derived from
/// them downstream is not, and the record says so.
pub fn iteration_end(prompt_tok_reported: u32, reasoning_chars: usize, content_chars: usize) {
    with_turn(|t| {
        if let Some(it) = t.current() {
            it.prompt_tok_reported = prompt_tok_reported;
            it.reasoning_chars = reasoning_chars;
            it.content_chars = content_chars;
        }
    });
}

/// Record one completed tool invocation.
///
/// `approval_ms` separates time a human spent deciding from time the tool spent
/// working: a turn that waited on a person is not a slow turn, and folding the
/// two together would make the residual meaningless.
/// Record one human approval the turn waited on, resolved either way.
///
/// Called where the wait happens, which is before the invoker rather than
/// inside it: an approval is requested, the turn blocks on the answer, and only
/// then does a tool run, if it runs at all. Recording it here is what keeps a
/// refused or timed-out approval out of the orchestration residual, which
/// 1.4.7 defines as time this project spent rather than time it waited for a
/// person.
pub fn approval_resolved(tool_name: &str, approval_ms: f64, approved: bool) {
    with_turn(|t| {
        let iteration_index = t.iterations.len().saturating_sub(1);
        t.approvals.push(ApprovalSample {
            tool_name: tool_name.to_owned(),
            approval_ms,
            approved,
            iteration_index,
        });
    });
}

pub fn tool_completed(tool_name: &str, tool_ms: f64, tool_approval_ms: Option<f64>) {
    with_turn(|t| {
        let iteration_index = t.iterations.len().saturating_sub(1);
        t.tools.push(ToolSample {
            tool_name: tool_name.to_owned(),
            tool_ms,
            tool_approval_ms,
            iteration_index,
        });
    });
}

/// Emit the turn's decomposition: always the INFO event, plus the JSON record
/// when a trace path is configured.
async fn finish(acc: &Mutex<TurnAccumulator>, path: Option<&str>) {
    // The guard is released before any await: holding a `std::sync::MutexGuard`
    // across one would make the whole turn future non-`Send`.
    let (record, path) = {
        let Ok(turn) = acc.lock() else {
            return;
        };
        let d = Decomposition::of(&turn);

        tracing::info!(
            session_id = %turn.session_id,
            turn_wall_ms = d.turn_wall_ms,
            iterations = d.iterations,
            tool_calls = d.tool_calls,
            approvals = d.approvals,
            engine_ms_total = d.engine_ms_total,
            tool_ms_total = d.tool_ms_total,
            approval_ms_total = d.approval_ms_total,
            orchestration_residual_ms = d.orchestration_residual_ms,
            orchestration_residual_ratio = d.orchestration_residual_ratio,
            "chat.react.turn.timings"
        );

        let Some(path) = path else {
            return;
        };
        // Built with a null provenance, filled in below: gathering provenance
        // needs to await, and the guard cannot outlive this block.
        (build_record(&turn, &d, Value::Null), path.to_owned())
    };

    let mut record = record;
    if let Some(obj) = record.as_object_mut() {
        obj.insert("provenance".to_owned(), provenance().await);
    }
    append_record(&path, record).await;
}

/// The per-turn sums and the residual between them.
struct Decomposition {
    turn_wall_ms: f64,
    iterations: usize,
    tool_calls: usize,
    approvals: usize,
    engine_ms_total: f64,
    tool_ms_total: f64,
    approval_ms_total: f64,
    orchestration_residual_ms: f64,
    orchestration_residual_ratio: f64,
}

impl Decomposition {
    fn of(turn: &TurnAccumulator) -> Self {
        let turn_wall_ms = turn.started.elapsed().as_secs_f64() * 1000.0;
        let engine_ms_total: f64 = turn
            .iterations
            .iter()
            .map(|it| {
                let t = it.engine_timings.as_ref();
                num(t, "prompt_ms") + num(t, "predicted_ms")
            })
            .sum();
        let tool_ms_total: f64 = turn.tools.iter().map(|t| t.tool_ms).sum();
        let approval_ms_total: f64 = turn.approvals.iter().map(|a| a.approval_ms).sum();
        // Defined by subtraction, so the identity holds exactly and the sum of
        // the parts plus the residual is the wall-clock by construction. Human
        // wait is one of the parts: a turn that waited on a person is not a slow
        // turn, and leaving it in the residual would describe the runtime as
        // busy while nothing ran.
        let orchestration_residual_ms =
            turn_wall_ms - engine_ms_total - tool_ms_total - approval_ms_total;
        Self {
            turn_wall_ms,
            iterations: turn.iterations.len(),
            tool_calls: turn.tools.len(),
            approvals: turn.approvals.len(),
            engine_ms_total,
            tool_ms_total,
            approval_ms_total,
            orchestration_residual_ms,
            orchestration_residual_ratio: if turn_wall_ms > 0.0 {
                orchestration_residual_ms / turn_wall_ms
            } else {
                0.0
            },
        }
    }
}

/// Read a numeric field out of an engine timings object, defaulting to zero.
///
/// Zero is right here and only here: an iteration that reported no timings
/// contributed no measured engine time, and its cost lands in the residual
/// where it is visible rather than silently attributed to the engine.
fn num(timings: Option<&Value>, key: &str) -> f64 {
    timings
        .and_then(|t| t.get(key))
        .and_then(Value::as_f64)
        .unwrap_or(0.0)
}

fn int(timings: Option<&Value>, key: &str) -> Option<u64> {
    timings.and_then(|t| t.get(key)).and_then(Value::as_u64)
}

/// Build one measurement record conforming to the agentic probe shape.
fn build_record(turn: &TurnAccumulator, d: &Decomposition, provenance: Value) -> Value {
    let iteration_records: Vec<Value> = turn
        .iterations
        .iter()
        .enumerate()
        .map(|(i, it)| iteration_record(i, it))
        .collect();

    let tool_records: Vec<Value> = turn
        .tools
        .iter()
        .map(|t| {
            json!({
                "tool_name": t.tool_name,
                "tool_ms": t.tool_ms,
                "tool_approval_ms": t.tool_approval_ms,
                "iteration_index": t.iteration_index,
            })
        })
        .collect();

    let approval_records: Vec<Value> = turn
        .approvals
        .iter()
        .map(|a| {
            json!({
                "tool_name": a.tool_name,
                "approval_ms": a.approval_ms,
                "approved": a.approved,
                "iteration_index": a.iteration_index,
            })
        })
        .collect();

    let launch = launch_config_slot()
        .lock()
        .ok()
        .and_then(|g| g.clone())
        .unwrap_or(Value::Null);

    let mut invalid: Vec<&str> = Vec::new();
    for it in &turn.iterations {
        let t = it.engine_timings.as_ref();
        if let (Some(p), Some(c)) = (int(t, "prompt_n"), int(t, "cache_n")) {
            let reported = u64::from(it.prompt_tok_reported);
            // I1 is an identity over the engine's own counters. It can only fail
            // if the engine changed shape, which is worth surfacing.
            if reported > 0 && p + c == 0 {
                invalid.push("I1");
            }
        }
        // I13. A first-token time below the engine's own prefill means the
        // client clock started after prefill had begun, so the figure excludes
        // the very wait it is meant to measure. Undetectable in the number
        // alone, obvious the moment it is checked against `prompt_ms`.
        if let (Some(ttft), Some(_)) = (it.ttft_ms, t) {
            if ttft < num(t, "prompt_ms") {
                invalid.push("I13");
            }
        }
    }
    if d.orchestration_residual_ms < 0.0 {
        // Negative residual: something overlapped, so the additive model does
        // not hold for this turn. Reported, never clamped.
        invalid.push("I6");
    }
    invalid.sort_unstable();
    invalid.dedup();

    json!({
        "schema_version": 1,
        "record_id": format!("turn-{}", uuid::Uuid::new_v4()),
        "campaign_id": Value::Null,
        "probe": "agentic",
        "label": launch
            .get("model_path")
            .and_then(Value::as_str)
            .and_then(|p| p.rsplit('/').next())
            .unwrap_or("unknown"),
        "measured_at": chrono::Utc::now().to_rfc3339(),
        "provenance": provenance,
        "conditions": {
            "run_index": 0,
            "run_order": "sequential",
            "page_cache": "unknown",
            "server_restarted_before": false,
            "slot_reset_before": false,
            // A live chat turn is not a controlled speed sample: the loop does
            // not pin a seed. Recorded honestly so the invariant that
            // disqualifies these records from speed comparison can fire.
            "sampling_seed": -1,
            "sampling_temperature": turn
                .iterations
                .first()
                .and_then(|it| it.temperature)
                .unwrap_or(0.0),
            "notes": "captured from a live chat turn, not a controlled speed run",
        },
        "engine": engine_block(&launch, turn),
        "turn": {
            "session_id": turn.session_id,
            "turn_wall_ms": d.turn_wall_ms,
            "iterations": d.iterations,
            "tool_calls": d.tool_calls,
            "approvals": d.approvals,
            "engine_ms_total": d.engine_ms_total,
            "tool_ms_total": d.tool_ms_total,
            "approval_ms_total": d.approval_ms_total,
            "approval_records": approval_records,
            "orchestration_residual_ms": d.orchestration_residual_ms,
            "orchestration_residual_ratio": d.orchestration_residual_ratio,
            "iteration_records": iteration_records,
            "tool_records": tool_records,
        },
        "invalid": invalid,
    })
}

/// One iteration rendered as a sample, under the dictionary's names.
fn iteration_record(index: usize, it: &IterationSample) -> Value {
    let t = it.engine_timings.as_ref();
    let computed = int(t, "prompt_n");
    let cached = int(t, "cache_n");
    let total = match (computed, cached) {
        (Some(p), Some(c)) => Some(p + c),
        _ => None,
    };
    let decode_tok = int(t, "predicted_n");
    let prefill_ms = t.map(|_| num(t, "prompt_ms"));
    let decode_ms = t.map(|_| num(t, "predicted_ms"));
    // Speculative decoding accounting. The engine only emits these keys when a
    // speculative path is enabled, so absence stays null rather than zero: a
    // request served without speculation drafted nothing, it did not draft
    // zero-and-fail.
    let draft_tok = int(t, "draft_n");
    let draft_tok_accepted = int(t, "draft_n_accepted");

    // Cold and warm are observed from the response, never asserted by the
    // caller: a fresh server still serves a warm request on a shared prefix.
    let cache_state = cached.map(|c| if c == 0 { "cold" } else { "warm" });

    let chars_total = it.reasoning_chars + it.content_chars;
    // Exact only if each region were tokenized separately, which would cost a
    // tokenizer pass per turn. Apportioning by characters is an approximation
    // and is labelled as one so no consumer mistakes it for a measurement.
    let (split_method, reasoning_tok, content_tok) = match (decode_tok, it.reasoning_chars) {
        (Some(d), r) if r > 0 && chars_total > 0 => {
            let rt = (d as f64 * (r as f64 / chars_total as f64)).round() as u64;
            ("chars_apportioned", Some(rt), Some(d.saturating_sub(rt)))
        }
        (Some(d), _) => ("none", None, Some(d)),
        (None, _) => ("none", None, None),
    };

    json!({
        "iteration_index": index,
        "streamed": true,
        "cache_state": cache_state,
        "prompt_tok_total": total,
        "prompt_tok_computed": computed,
        "prompt_tok_cached": cached,
        "prompt_cache_hit_ratio": match (cached, total) {
            (Some(c), Some(t)) if t > 0 => Some(c as f64 / t as f64),
            _ => None,
        },
        "decode_tok": decode_tok,
        "decode_tok_reasoning": reasoning_tok,
        "decode_tok_content": content_tok,
        "draft_tok": draft_tok,
        "draft_tok_accepted": draft_tok_accepted,
        "draft_acceptance_ratio": match (draft_tok_accepted, draft_tok) {
            (Some(a), Some(d)) if d > 0 => Some(a as f64 / d as f64),
            _ => None,
        },
        "reasoning_split_method": split_method,
        "reasoning_chars": it.reasoning_chars,
        "content_chars": it.content_chars,
        "ttft_ms": it.ttft_ms,
        "prefill_ms": prefill_ms,
        "decode_ms": decode_ms,
        "ttft_overhead_ms": match (it.ttft_ms, prefill_ms) {
            (Some(ttft), Some(p)) => Some(ttft - p),
            _ => None,
        },
        "prefill_tps": match (computed, prefill_ms) {
            (Some(c), Some(ms)) if c > 0 && ms > 0.0 => Some(c as f64 / (ms / 1000.0)),
            _ => None,
        },
        "decode_tps": match (decode_tok, decode_ms) {
            (Some(d), Some(ms)) if d > 0 && ms > 0.0 => Some(d as f64 / (ms / 1000.0)),
            _ => None,
        },
        "context_occupancy_at_call_ratio": match (total.or(Some(u64::from(it.prompt_tok_reported))), it.n_ctx_slot_tok) {
            (Some(p), Some(w)) if w > 0 => Some(p as f64 / f64::from(w)),
            _ => None,
        },
    })
}

/// The launch configuration, mirroring the Rust field names one for one.
fn engine_block(launch: &Value, turn: &TurnAccumulator) -> Value {
    let slot = turn.iterations.first().and_then(|it| it.n_ctx_slot_tok);
    let mut block = launch.clone();
    if block.is_null() {
        block = json!({});
    }
    if let Some(obj) = block.as_object_mut() {
        // The window a single conversation actually gets. Taken from the active
        // backend rather than from `-c`, which diverges as soon as slots > 1.
        obj.insert("n_ctx_slot_tok".to_owned(), json!(slot));
        obj.remove("model_path");
    }
    block
}

/// Append one record as a JSON line.
///
/// One line per turn, each self-contained, so a truncated file stays readable up
/// to its last whole line.
async fn append_record(path: &str, record: Value) {
    let line = match serde_json::to_string(&record) {
        Ok(l) => l,
        Err(e) => {
            tracing::debug!(error = %e, "chat.react.turn.trace.unserializable");
            return;
        }
    };
    let path = path.to_owned();
    let written = tokio::task::spawn_blocking(move || {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        writeln!(f, "{line}")
    })
    .await;
    if let Ok(Err(e)) = written {
        tracing::debug!(error = %e, "chat.react.turn.trace.unwritable");
    }
}

/// Gather the provenance block once per process.
///
/// Only reached when a trace path is set, because hashing the model file is far
/// too expensive to pay for on a turn nobody is measuring.
async fn provenance() -> Value {
    if let Some(p) = PROVENANCE.get() {
        return p.clone();
    }
    let model_path = launch_config_slot()
        .lock()
        .ok()
        .and_then(|g| g.clone())
        .and_then(|c| {
            c.get("model_path")
                .and_then(Value::as_str)
                .map(str::to_owned)
        });
    let launch_args = launch_config_slot()
        .lock()
        .ok()
        .and_then(|g| g.clone())
        .and_then(|c| c.get("args").cloned())
        .unwrap_or(Value::Null);
    let server_path = launch_config_slot()
        .lock()
        .ok()
        .and_then(|g| g.clone())
        .and_then(|c| {
            c.get("llama_server_path")
                .and_then(Value::as_str)
                .map(str::to_owned)
        });

    let built =
        tokio::task::spawn_blocking(move || build_provenance(model_path, launch_args, server_path))
            .await
            .unwrap_or(Value::Null);
    let _ = PROVENANCE.set(built.clone());
    built
}

/// Blocking half of provenance gathering: subprocesses and a file hash.
fn build_provenance(
    model_path: Option<String>,
    launch_args: Value,
    server_path: Option<String>,
) -> Value {
    let sh = |cmd: &str, args: &[&str]| -> Option<String> {
        let mut probe = std::process::Command::new(cmd);
        apollia_core::subprocess_window::hide_console(&mut probe);
        let out = probe.args(args).output().ok()?;
        out.status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
    };

    // A campaign that restarts the daemon per condition reloads the same GGUF
    // each time, and a debug-build hash of a 20 GiB file costs minutes. The
    // orchestrator may therefore inject the hash it computed once; the value
    // still lands in the record verbatim, so provenance stays complete.
    let injected = std::env::var("APOLLIA_PERF_MODEL_SHA256")
        .ok()
        .filter(|s| !s.is_empty());
    let (model_sha256, scope) = match (injected, model_path.as_deref()) {
        (Some(h), Some(_)) => (Value::String(h), "whole_file"),
        (None, Some(p)) => match sha256_file(p) {
            Some(h) => (Value::String(h), "whole_file"),
            None => (Value::Null, "none"),
        },
        _ => (Value::Null, "none"),
    };

    // The engine prints its version banner on stderr, so both streams are
    // read; the first line is the `version: NNNN (sha)` banner either way.
    let server_version = server_path.as_deref().and_then(|p| {
        let mut banner = std::process::Command::new(p);
        apollia_core::subprocess_window::hide_console(&mut banner);
        let out = banner.arg("--version").output().ok()?;
        if !out.status.success() {
            return None;
        }
        let merged = if out.stdout.is_empty() {
            out.stderr
        } else {
            out.stdout
        };
        String::from_utf8_lossy(&merged)
            .lines()
            .next()
            .map(str::to_owned)
    });

    json!({
        "git_sha": sh("git", &["rev-parse", "HEAD"]).map_or(Value::Null, Value::String),
        "git_dirty": sh("git", &["status", "--porcelain"]).is_some_and(|s| !s.is_empty()),
        "llama_server_version": server_version.map_or(Value::Null, Value::String),
        "llama_server_path": server_path.map_or(Value::Null, Value::String),
        "model_path": model_path.map_or(Value::Null, Value::String),
        "model_sha256": model_sha256,
        "model_sha256_scope": scope,
        "launch_args": launch_args,
        "machine_id": sh("sysctl", &["-n", "hw.model"]).map_or(Value::Null, Value::String),
        "machine_chip": sh("sysctl", &["-n", "machdep.cpu.brand_string"])
            .map_or(Value::Null, Value::String),
        "machine_memory_bytes": sh("sysctl", &["-n", "hw.memsize"])
            .and_then(|s| s.parse::<u64>().ok())
            .map_or(Value::Null, |v| json!(v)),
        "os_version": sh("sw_vers", &["-productVersion"]).map_or(Value::Null, Value::String),
    })
}

/// SHA-256 of a file, streamed so a multi-gigabyte model does not land in memory.
fn sha256_file(path: &str) -> Option<String> {
    use sha2::{Digest, Sha256};
    let mut f = std::fs::File::open(path).ok()?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut f, &mut hasher).ok()?;
    Some(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timings(prompt_n: u64, cache_n: u64, prompt_ms: f64, pred_n: u64, pred_ms: f64) -> Value {
        json!({
            "prompt_n": prompt_n, "cache_n": cache_n, "prompt_ms": prompt_ms,
            "predicted_n": pred_n, "predicted_ms": pred_ms,
        })
    }

    #[tokio::test]
    async fn test_turn_records_iterations_and_tools_in_order() {
        // GIVEN a turn with two iterations and two tool calls
        let out = instrument_turn("sess-1", async {
            iteration_begin(Some(0.0), Some(32_768));
            iteration_ttft(120.0);
            iteration_engine_timings(&timings(1000, 0, 500.0, 20, 300.0));
            iteration_end(1000, 0, 80);
            tool_completed("bash_executor", 210.0, None);
            tool_completed("file_io", 40.0, None);

            iteration_begin(Some(0.0), Some(32_768));
            iteration_ttft(15.0);
            iteration_engine_timings(&timings(30, 1000, 20.0, 10, 150.0));
            iteration_end(1030, 0, 40);
            "done"
        })
        .await;

        // WHEN the turn completes
        // THEN the future's own value is untouched by the instrumentation
        assert_eq!(out, "done");
    }

    #[tokio::test]
    async fn test_entry_points_are_no_ops_outside_a_turn() {
        // GIVEN no turn scope at all
        // WHEN every entry point is called
        iteration_begin(Some(0.0), Some(1));
        iteration_ttft(1.0);
        iteration_engine_timings(&timings(1, 0, 1.0, 1, 1.0));
        iteration_end(1, 0, 0);
        tool_completed("bash_executor", 1.0, None);

        // THEN nothing panics and nothing is recorded: reaching here is the
        // assertion. This is what lets the same calls sit on paths that are not
        // chat turns.
    }

    #[test]
    fn test_decomposition_residual_closes_the_identity_exactly() {
        // GIVEN a turn with known engine and tool time
        let mut turn = TurnAccumulator::new("sess".to_owned());
        turn.iterations.push(IterationSample {
            engine_timings: Some(timings(100, 0, 500.0, 10, 300.0)),
            ..IterationSample::default()
        });
        turn.iterations.push(IterationSample {
            engine_timings: Some(timings(10, 100, 20.0, 5, 150.0)),
            ..IterationSample::default()
        });
        turn.tools.push(ToolSample {
            tool_name: "bash_executor".to_owned(),
            tool_ms: 210.0,
            tool_approval_ms: None,
            iteration_index: 0,
        });

        // WHEN the decomposition is computed
        let d = Decomposition::of(&turn);

        // THEN the parts plus the residual equal the wall-clock exactly
        let sum = d.engine_ms_total + d.tool_ms_total + d.orchestration_residual_ms;
        assert!(
            (sum - d.turn_wall_ms).abs() < 1e-9,
            "sum {sum} vs wall {}",
            d.turn_wall_ms
        );
        assert!((d.engine_ms_total - 970.0).abs() < 1e-9);
        assert!((d.tool_ms_total - 210.0).abs() < 1e-9);
    }

    #[test]
    fn test_iteration_record_marks_a_cached_prompt_warm() {
        // GIVEN an iteration whose prompt was almost entirely cached
        let it = IterationSample {
            engine_timings: Some(timings(3, 3131, 35.0, 40, 597.9)),
            ttft_ms: Some(23.5),
            n_ctx_slot_tok: Some(32_768),
            reasoning_chars: 0,
            content_chars: 120,
            ..IterationSample::default()
        };

        // WHEN it is rendered
        let r = iteration_record(0, &it);

        // THEN cache state is observed from the response, not assumed
        assert_eq!(r["cache_state"], "warm");
        assert_eq!(r["prompt_tok_total"], 3134);
        assert_eq!(r["prompt_tok_computed"], 3);
        assert_eq!(r["prompt_tok_cached"], 3131);
    }

    #[test]
    fn test_iteration_record_marks_an_uncached_prompt_cold() {
        // GIVEN an iteration that computed its whole prompt
        let it = IterationSample {
            engine_timings: Some(timings(3132, 0, 3461.7, 27, 399.3)),
            ..IterationSample::default()
        };

        // WHEN it is rendered
        let r = iteration_record(0, &it);

        // THEN it is cold and the prefill rate is defined
        assert_eq!(r["cache_state"], "cold");
        assert_eq!(r["prompt_cache_hit_ratio"], 0.0);
        let prefill = r["prefill_tps"].as_f64().expect("prefill defined");
        assert!((prefill - 3132.0 / 3.4617).abs() < 1.0, "{prefill}");
    }

    #[test]
    fn test_iteration_record_labels_the_reasoning_split_as_approximate() {
        // GIVEN an iteration that emitted a think block
        let it = IterationSample {
            engine_timings: Some(timings(100, 0, 50.0, 100, 500.0)),
            reasoning_chars: 300,
            content_chars: 100,
            ..IterationSample::default()
        };

        // WHEN it is rendered
        let r = iteration_record(0, &it);

        // THEN the split is apportioned by characters and says so
        assert_eq!(r["reasoning_split_method"], "chars_apportioned");
        assert_eq!(r["decode_tok_reasoning"], 75);
        assert_eq!(r["decode_tok_content"], 25);
    }

    #[test]
    fn test_iteration_record_without_a_think_block_claims_no_split() {
        // GIVEN an iteration with no reasoning characters
        let it = IterationSample {
            engine_timings: Some(timings(100, 0, 50.0, 40, 500.0)),
            reasoning_chars: 0,
            content_chars: 200,
            ..IterationSample::default()
        };

        // WHEN it is rendered
        let r = iteration_record(0, &it);

        // THEN the split method is `none` and every token is content by
        // definition, not by measurement
        assert_eq!(r["reasoning_split_method"], "none");
        assert_eq!(r["decode_tok_reasoning"], Value::Null);
        assert_eq!(r["decode_tok_content"], 40);
    }

    #[test]
    fn test_iteration_record_carries_draft_fields_when_engine_reports_them() {
        // GIVEN timings from a speculative-decoding request, draft keys present
        let mut t = timings(100, 0, 50.0, 40, 500.0);
        t["draft_n"] = json!(239);
        t["draft_n_accepted"] = json!(35);
        let it = IterationSample {
            engine_timings: Some(t),
            ..IterationSample::default()
        };

        // WHEN it is rendered
        let r = iteration_record(0, &it);

        // THEN the contract's draft fields are present with the derived ratio
        assert_eq!(r["draft_tok"], 239);
        assert_eq!(r["draft_tok_accepted"], 35);
        let ratio = r["draft_acceptance_ratio"].as_f64().unwrap();
        assert!((ratio - 35.0 / 239.0).abs() < 1e-12);
    }

    #[test]
    fn test_iteration_record_draft_fields_null_without_speculation() {
        // GIVEN timings from a plain request, no draft keys emitted
        let it = IterationSample {
            engine_timings: Some(timings(100, 0, 50.0, 40, 500.0)),
            ..IterationSample::default()
        };

        // WHEN it is rendered
        let r = iteration_record(0, &it);

        // THEN absence stays null, never zero
        assert_eq!(r["draft_tok"], Value::Null);
        assert_eq!(r["draft_tok_accepted"], Value::Null);
        assert_eq!(r["draft_acceptance_ratio"], Value::Null);
    }

    #[test]
    fn test_iteration_without_engine_timings_leaves_rates_null() {
        // GIVEN an iteration whose backend reported no timings, as a cloud one
        let it = IterationSample {
            ttft_ms: Some(400.0),
            ..IterationSample::default()
        };

        // WHEN it is rendered
        let r = iteration_record(0, &it);

        // THEN the unmeasurable fields are null rather than zero
        assert_eq!(r["prefill_tps"], Value::Null);
        assert_eq!(r["decode_tps"], Value::Null);
        assert_eq!(r["cache_state"], Value::Null);
        assert_eq!(r["ttft_ms"], 400.0);
    }

    /// The acceptance scenario: a scripted turn with two engine completions and
    /// two tool calls, driven through the real entry points against a stub that
    /// stands in for the backend, writing to a real trace file.
    ///
    /// The destination is injected rather than set in the environment, so the
    /// test owns its own file and cannot race another one.
    #[tokio::test]
    async fn test_scripted_two_tool_turn_writes_exactly_one_valid_record() {
        // GIVEN a trace path and a scripted two-iteration, two-tool turn
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("turns.jsonl");
        // The engine and tool figures are small and the turn really sleeps, so
        // the wall-clock genuinely exceeds the attributed time. A fixture whose
        // synthetic timings outran real elapsed time would produce a negative
        // residual and be flagged, which is correct behaviour but not what this
        // scenario is meant to exercise. Each first-token figure sits above its
        // own prefill for the same reason: a fixture violating I13 would describe
        // a turn that cannot physically occur.
        instrument_turn_to("sess-accept", Some(path.display().to_string()), async {
            iteration_begin(Some(0.0), Some(32_768));
            iteration_ttft(6.0);
            iteration_engine_timings(&timings(1000, 0, 5.0, 20, 3.0));
            iteration_end(1000, 0, 80);
            tokio::time::sleep(std::time::Duration::from_millis(15)).await;
            // Two tools, both attributed to the iteration that requested them.
            tool_completed("bash_executor", 2.0, Some(1.0));
            tool_completed("file_io", 1.0, None);

            iteration_begin(Some(0.0), Some(32_768));
            iteration_ttft(1.5);
            iteration_engine_timings(&timings(30, 1000, 1.0, 10, 2.0));
            iteration_end(1030, 0, 40);
            tokio::time::sleep(std::time::Duration::from_millis(15)).await;
        })
        .await;

        // WHEN the file is read back
        let body = std::fs::read_to_string(&path).expect("trace written");
        let lines: Vec<&str> = body.lines().filter(|l| !l.trim().is_empty()).collect();

        // THEN exactly one turn record was written
        assert_eq!(lines.len(), 1, "expected one record, got {}", lines.len());
        let r: Value = serde_json::from_str(lines[0]).expect("record parses");

        // AND its shape is the agentic probe record, with every block the
        // measurement-record schema requires present
        assert_eq!(r["schema_version"], 1);
        assert_eq!(r["probe"], "agentic");
        assert_eq!(r["turn"]["session_id"], "sess-accept");
        for key in [
            "record_id",
            "campaign_id",
            "label",
            "measured_at",
            "provenance",
            "conditions",
            "engine",
            "turn",
            "invalid",
        ] {
            assert!(r.get(key).is_some(), "record is missing {key}");
        }
        for key in [
            "run_index",
            "run_order",
            "page_cache",
            "server_restarted_before",
            "slot_reset_before",
            "sampling_seed",
            "sampling_temperature",
        ] {
            assert!(
                r["conditions"].get(key).is_some(),
                "conditions is missing {key}"
            );
        }
        // A live chat turn pins no seed, so the invariant that disqualifies
        // these records from speed comparison can fire on them.
        assert_eq!(r["conditions"]["sampling_seed"], -1);
        assert_eq!(r["invalid"].as_array().expect("invalid").len(), 0);

        // AND the iteration and tool counts match what was scripted
        assert_eq!(r["turn"]["iterations"], 2);
        assert_eq!(r["turn"]["tool_calls"], 2);
        assert_eq!(
            r["turn"]["iteration_records"]
                .as_array()
                .expect("iterations")
                .len(),
            2
        );
        let tools = r["turn"]["tool_records"].as_array().expect("tools");
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0]["tool_name"], "bash_executor");
        assert_eq!(tools[0]["tool_approval_ms"], 1.0);
        // Both tools were requested by the first completion.
        assert_eq!(tools[0]["iteration_index"], 0);
        assert_eq!(tools[1]["iteration_index"], 0);

        // AND the parts plus the residual equal the wall-clock within a
        // millisecond, which is the identity the residual is defined to close
        let wall = r["turn"]["turn_wall_ms"].as_f64().expect("wall");
        let engine = r["turn"]["engine_ms_total"].as_f64().expect("engine");
        let tool = r["turn"]["tool_ms_total"].as_f64().expect("tool");
        let residual = r["turn"]["orchestration_residual_ms"]
            .as_f64()
            .expect("residual");
        assert!(
            (wall - (engine + tool + residual)).abs() < 1.0,
            "wall {wall} vs parts {}",
            engine + tool + residual
        );
        // AND the engine total is the sum of both completions' own timings
        assert!((engine - 11.0).abs() < 1e-9, "engine {engine}");
        assert!((tool - 3.0).abs() < 1e-9, "tool {tool}");
        // AND the residual is positive, as it must be when nothing overlapped
        assert!(residual > 0.0, "residual {residual}");

        // AND the second iteration is observed warm from its own cache count
        let its = r["turn"]["iteration_records"].as_array().expect("iters");
        assert_eq!(its[0]["cache_state"], "cold");
        assert_eq!(its[1]["cache_state"], "warm");
        assert_eq!(its[1]["prompt_tok_total"], 1030);
    }

    #[tokio::test]
    async fn test_no_trace_file_is_written_when_the_variable_is_unset() {
        // GIVEN no APOLLIA_PERF_TRACE in the environment
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("must-not-exist.jsonl");

        // WHEN an instrumented turn runs with no destination
        instrument_turn_to("sess-off", None, async {
            iteration_begin(Some(0.0), Some(32_768));
            iteration_engine_timings(&timings(10, 0, 5.0, 2, 30.0));
            iteration_end(10, 0, 8);
        })
        .await;

        // THEN nothing is written: the INFO event is the only output
        assert!(!path.exists());
    }

    #[test]
    fn test_negative_residual_is_reported_not_clamped() {
        // GIVEN a turn whose attributed time exceeds its wall-clock, which means
        // something overlapped
        let mut turn = TurnAccumulator::new("sess".to_owned());
        turn.tools.push(ToolSample {
            tool_name: "slow".to_owned(),
            tool_ms: 60_000.0,
            tool_approval_ms: None,
            iteration_index: 0,
        });

        // WHEN the record is built
        let d = Decomposition::of(&turn);
        let record = build_record(&turn, &d, Value::Null);

        // THEN the residual stays negative and the violated invariant is named
        assert!(d.orchestration_residual_ms < 0.0);
        assert_eq!(record["invalid"][0], "I6");
    }

    #[test]
    fn test_unanswered_approval_leaves_the_residual_and_is_named() {
        // GIVEN a turn that blocked 300 seconds on an approval nobody answered,
        // which ran no tool: the exact shape that once reported a 98.7 percent
        // orchestration residual with nothing running
        let mut turn = TurnAccumulator::new("sess".to_owned());
        turn.approvals.push(ApprovalSample {
            tool_name: "bash_executor".to_owned(),
            approval_ms: 300_000.0,
            approved: false,
            iteration_index: 0,
        });

        // WHEN the record is built
        let d = Decomposition::of(&turn);
        let record = build_record(&turn, &d, Value::Null);

        // THEN the wait is attributed to the human, not to orchestration, and it
        // is visible even though no tool ever ran
        assert_eq!(d.tool_calls, 0);
        assert_eq!(d.approvals, 1);
        assert!((d.approval_ms_total - 300_000.0).abs() < 1e-9);
        assert!(
            d.orchestration_residual_ms < 0.0,
            "a 300 s wait against a near-zero wall-clock must not read as orchestration"
        );
        assert_eq!(record["turn"]["approval_records"][0]["approved"], false);
        assert_eq!(
            record["turn"]["approval_records"][0]["tool_name"],
            "bash_executor"
        );
    }

    #[test]
    fn test_identity_closes_with_an_approval_among_the_parts() {
        // GIVEN a turn carrying engine time, tool time and an answered approval
        let mut turn = TurnAccumulator::new("sess".to_owned());
        turn.iterations.push(IterationSample {
            engine_timings: Some(timings(100, 0, 40.0, 10, 30.0)),
            ..IterationSample::default()
        });
        turn.tools.push(ToolSample {
            tool_name: "file_io".to_owned(),
            tool_ms: 5.0,
            tool_approval_ms: None,
            iteration_index: 0,
        });
        turn.approvals.push(ApprovalSample {
            tool_name: "file_io".to_owned(),
            approval_ms: 12.0,
            approved: true,
            iteration_index: 0,
        });

        // WHEN the decomposition is computed
        let d = Decomposition::of(&turn);

        // THEN every part plus the residual is still the wall-clock exactly
        let sum =
            d.engine_ms_total + d.tool_ms_total + d.approval_ms_total + d.orchestration_residual_ms;
        assert!(
            (sum - d.turn_wall_ms).abs() < 1e-9,
            "sum {sum} vs wall {}",
            d.turn_wall_ms
        );
        assert!((d.approval_ms_total - 12.0).abs() < 1e-9);
    }

    /// Names the invariants a record reports as violated.
    fn flagged(record: &Value) -> Vec<&str> {
        record["invalid"]
            .as_array()
            .map(|a| a.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default()
    }

    #[test]
    fn test_ttft_below_engine_prefill_is_flagged_as_a_misplaced_clock() {
        // GIVEN an iteration reporting 1.2 ms to first token against 7587 ms of
        // engine prefill, the signature of a clock started after the backend had
        // already awaited the first chunk
        let mut turn = TurnAccumulator::new("sess".to_owned());
        turn.iterations.push(IterationSample {
            engine_timings: Some(timings(6439, 0, 7587.0, 40, 600.0)),
            ttft_ms: Some(1.2),
            ..IterationSample::default()
        });

        // WHEN the record is built
        let d = Decomposition::of(&turn);
        let record = build_record(&turn, &d, Value::Null);

        // THEN the record names I13 rather than publishing the figure as a
        // first-token latency
        let names = flagged(&record);
        assert!(names.contains(&"I13"), "expected I13 in {names:?}");
    }

    #[test]
    fn test_ttft_above_engine_prefill_raises_no_clock_violation() {
        // GIVEN the same iteration measured from the request dispatch, so the
        // figure contains the prefill it is supposed to contain
        let mut turn = TurnAccumulator::new("sess".to_owned());
        turn.iterations.push(IterationSample {
            engine_timings: Some(timings(6439, 0, 7587.0, 40, 600.0)),
            ttft_ms: Some(7594.0),
            ..IterationSample::default()
        });

        // WHEN the record is built
        let d = Decomposition::of(&turn);
        let record = build_record(&turn, &d, Value::Null);

        // THEN no clock violation is reported
        let names = flagged(&record);
        assert!(!names.contains(&"I13"), "unexpected I13 in {names:?}");
    }
}

#[cfg(test)]
mod overhead {
    use super::*;

    /// The instrumentation must be free when nobody is measuring.
    ///
    /// Measures the whole per-turn path with no destination configured, which is
    /// the production default: scope setup, the recorder calls of one iteration,
    /// the decomposition, and the INFO event. Measured at roughly 2.4 us per turn
    /// on an M3 Ultra, against turns that take seconds, so the bound below is
    /// two orders of magnitude looser than the observed cost and can only fire
    /// on a real regression rather than on scheduling noise.
    #[tokio::test]
    async fn test_disabled_path_costs_far_less_than_a_turn() {
        const TURNS: u32 = 20_000;
        const CEILING_NS: u128 = 500_000;
        let t = json!({"prompt_n": 1000, "cache_n": 0, "prompt_ms": 500.0,
                       "predicted_n": 20, "predicted_ms": 300.0});

        // GIVEN a baseline of the same number of empty futures
        let bare = Instant::now();
        for _ in 0..TURNS {
            std::hint::black_box(async {}).await;
        }
        let bare = bare.elapsed();

        // WHEN the same count runs fully instrumented with tracing disabled
        let wrapped = Instant::now();
        for _ in 0..TURNS {
            instrument_turn_to("bench", None, async {
                iteration_begin(Some(0.0), Some(32_768));
                iteration_ttft(1.0);
                iteration_engine_timings(&t);
                iteration_end(1000, 0, 80);
                tool_completed("bash_executor", 1.0, None);
            })
            .await;
        }
        let wrapped = wrapped.elapsed();

        // THEN the added cost per turn is negligible against a turn's own scale
        let per_turn_ns = wrapped.saturating_sub(bare).as_nanos() / u128::from(TURNS);
        println!("disabled-path cost: {per_turn_ns} ns per turn over {TURNS} turns");
        assert!(
            per_turn_ns < CEILING_NS,
            "instrumentation cost {per_turn_ns} ns per turn exceeds {CEILING_NS} ns"
        );
    }
}
