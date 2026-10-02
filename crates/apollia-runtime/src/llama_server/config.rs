//! Launch configuration for the embedded `llama-server`, and the pure argument
//! vector builder that turns it into a command line.
//!
//! Every performance-relevant flag is a field here rather than a literal in the
//! spawn path, so a measurement campaign can vary one factor at a time without
//! recompiling. The defaults reproduce the historical command line exactly:
//!
//! ```text
//! -m <model> -ngl 999 -c 32768 -np 1 -cb --flash-attn on --jinja
//! --reasoning-format auto -lv 4 --host 127.0.0.1 --port <port>
//! ```
//!
//! `-lv 4` is the one flag that line did not carry. At the engine's default
//! verbosity the load prints no device and no offload tally, so the runtime's
//! journal could not say where the model went; at 4 it prints both, plus the
//! per-device buffer sizes, which `engine_facts` lifts into structured events.
//! Measured on 2026-09-09: 30 lines forwarded at the default, none naming a
//! device; 281 at 4, the placement among them.
//!
//! A field whose flag that line does not pass defaults to `None` and emits
//! nothing, leaving llama-server's own default in force. No upstream default is
//! mirrored into Rust, because mirroring one silently freezes a value that the
//! bundled binary is free to change between releases.
//!
//! Two flags stay hardcoded in [`build_args`] rather than becoming fields.
//! `--jinja` selects the tool-calling template path, and `--reasoning-format
//! auto` lets the engine split reasoning out with the parser that matches the
//! loaded template. The OpenAI client re-inlines that field as `<think>` tags,
//! which is the shape the whole chat pipeline parses.
//!
//! `none` was the value until 2026-09-22, when it was left over from before the
//! client read the separate field. It passed every family's own markers through
//! as text, and Gemma 4 does not spell them `<think>`: its reasoning reached the
//! chat as `<|channel>thought ... <channel|>` and was shown as the answer. After
//! a tool result that template opens the channel in the prompt, so the output
//! carries only the closing marker, and no text parser can recover where the
//! reasoning began. Only the engine, which rendered the prompt, knows.
//! Measured with the bundled 10092 build and Gemma 4 12B: both the first turn
//! and the turn after a tool result come back with the reasoning separated and
//! no marker left in the content, streamed or not.
//!
//! Neither flag is performance-relevant. `extra_args` is the escape hatch if
//! either genuinely needs to move.
//!
//! # Environment overrides
//!
//! Each field is overridable at spawn time by one variable, so an experiment
//! runner can sweep values without touching persisted user state:
//!
//! | Variable | Field |
//! |---|---|
//! | `APOLLIA_LLAMA_N_CTX` | `n_ctx` |
//! | `APOLLIA_LLAMA_N_GPU_LAYERS` | `n_gpu_layers` |
//! | `APOLLIA_LLAMA_N_BATCH` | `n_batch` |
//! | `APOLLIA_LLAMA_N_UBATCH` | `n_ubatch` |
//! | `APOLLIA_LLAMA_N_PARALLEL` | `n_parallel` |
//! | `APOLLIA_LLAMA_CONT_BATCHING` | `cont_batching` |
//! | `APOLLIA_LLAMA_CACHE_TYPE_K` | `cache_type_k` |
//! | `APOLLIA_LLAMA_CACHE_TYPE_V` | `cache_type_v` |
//! | `APOLLIA_LLAMA_FLASH_ATTN` | `flash_attn` |
//! | `APOLLIA_LLAMA_CACHE_REUSE` | `cache_reuse` |
//! | `APOLLIA_LLAMA_METRICS` | `metrics` |
//! | `APOLLIA_LLAMA_LOG_VERBOSITY` | `log_verbosity` |
//! | `APOLLIA_LLAMA_EXTRA_ARGS` | `extra_args` |
//!
//! For an optional field, an empty value clears it and omits the flag, which is
//! the only way to reach a llama-server default that differs from the Apollia
//! one (`-np` auto, for instance). An unparseable value is a no-op: it logs a
//! warning and keeps the value that was already there, so a typo in a sweep
//! script cannot silently launch a different engine or fail a spawn.
//!
//! A model's default preset is laid over the base before these variables, and
//! the automatic plans (window, offload, slots) only decide what neither of
//! the two fixed: the environment, then the preset, then the computation. See
//! `load_settings`.
//!
//! `model_path` deliberately has no override. It is user state owned by
//! `switch_model`, and an ambient variable pinning it would defeat every model
//! switch made from the UI.

use std::fmt;
use std::str::FromStr;

/// Loopback address the server binds to. Never configurable: the OpenAI-compatible
/// surface is for this machine only, and binding elsewhere would expose an
/// unauthenticated inference endpoint.
const HOST: &str = "127.0.0.1";

pub(crate) const ENV_N_CTX: &str = "APOLLIA_LLAMA_N_CTX";
/// Read by `ngl::apply_offload_plan` too, which leaves the offload count alone
/// when the operator has set it.
pub(crate) const ENV_N_GPU_LAYERS: &str = "APOLLIA_LLAMA_N_GPU_LAYERS";
const ENV_N_BATCH: &str = "APOLLIA_LLAMA_N_BATCH";
const ENV_N_UBATCH: &str = "APOLLIA_LLAMA_N_UBATCH";
/// Read by `parallel::apply_parallel_plan` too, which leaves the slot count
/// alone when the operator has set it.
pub(crate) const ENV_N_PARALLEL: &str = "APOLLIA_LLAMA_N_PARALLEL";
const ENV_CONT_BATCHING: &str = "APOLLIA_LLAMA_CONT_BATCHING";
const ENV_CACHE_TYPE_K: &str = "APOLLIA_LLAMA_CACHE_TYPE_K";
const ENV_CACHE_TYPE_V: &str = "APOLLIA_LLAMA_CACHE_TYPE_V";
const ENV_FLASH_ATTN: &str = "APOLLIA_LLAMA_FLASH_ATTN";
const ENV_CACHE_REUSE: &str = "APOLLIA_LLAMA_CACHE_REUSE";
const ENV_METRICS: &str = "APOLLIA_LLAMA_METRICS";
const ENV_LOG_VERBOSITY: &str = "APOLLIA_LLAMA_LOG_VERBOSITY";
const ENV_EXTRA_ARGS: &str = "APOLLIA_LLAMA_EXTRA_ARGS";

/// Flash attention mode accepted by `--flash-attn`.
///
/// A typed enum rather than a free string so a bad override is rejected while
/// resolving the configuration, not minutes later when the process refuses to
/// start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlashAttn {
    /// Force flash attention on.
    On,
    /// Force flash attention off.
    Off,
    /// Let llama-server decide from the model and backend.
    Auto,
}

impl fmt::Display for FlashAttn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::On => "on",
            Self::Off => "off",
            Self::Auto => "auto",
        };
        f.write_str(s)
    }
}

/// Rejected value for [`FlashAttn::from_str`].
#[derive(Debug, thiserror::Error)]
#[error("expected one of on, off, auto, got {value}")]
pub struct ParseFlashAttnError {
    /// The value that could not be parsed.
    pub value: String,
}

impl FromStr for FlashAttn {
    type Err = ParseFlashAttnError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "on" => Ok(Self::On),
            "off" => Ok(Self::Off),
            "auto" => Ok(Self::Auto),
            _ => Err(ParseFlashAttnError {
                value: s.to_owned(),
            }),
        }
    }
}

/// Launch configuration for the embedded `llama-server`.
#[derive(Clone, Debug)]
pub struct LlamaServerConfig {
    /// Absolute path to the `.gguf` model file to load (`-m`).
    pub model_path: String,
    /// Absolute path to the vision projector fetched with the model
    /// (`--mmproj`). `None` loads the model without vision. Set at spawn time
    /// from the model index, like `model_path`, and never from the environment.
    pub mmproj_path: Option<String>,
    /// Context window one request may use, in tokens. With a single slot it is
    /// the `-c` argument as is; with a unified buffer the engine is launched
    /// with one such window per slot, see [`LlamaServerConfig::engine_n_ctx`].
    pub n_ctx: u32,
    /// Number of layers to offload to the GPU (`-ngl`). `999` offloads all;
    /// a CPU-only build silently ignores the request.
    pub n_gpu_layers: i32,
    /// Logical maximum batch size (`-b`). `None` leaves the engine default.
    pub n_batch: Option<u32>,
    /// Physical maximum micro-batch size (`-ub`). `None` leaves the engine
    /// default. This is the knob that trades prefill throughput against peak
    /// memory during prompt processing.
    pub n_ubatch: Option<u32>,
    /// Number of server slots (`-np`). `1` here: the engine default is auto, so
    /// the field always names a count. At spawn time the count is planned from
    /// the model and the machine (see `parallel`), unless
    /// `APOLLIA_LLAMA_N_PARALLEL` pins it.
    pub n_parallel: Option<u32>,
    /// One key/value buffer shared by every slot (`--kv-unified`). Set by the
    /// slot planner together with the count it chose, so that each slot can
    /// hold a full window. Off by default, which leaves the engine to split
    /// `-c` evenly across the slots.
    pub kv_unified: bool,
    /// Continuous batching (`-cb` when true, `-nocb` when false). `None` leaves
    /// the engine default, which is enabled.
    pub cont_batching: Option<bool>,
    /// KV cache data type for keys (`-ctk`). `None` leaves the engine default.
    /// Values are validated by the binary, not here: the accepted set belongs to
    /// the bundled build and would drift if it were mirrored.
    pub cache_type_k: Option<String>,
    /// KV cache data type for values (`-ctv`). `None` leaves the engine default.
    pub cache_type_v: Option<String>,
    /// Flash attention mode (`--flash-attn`). `None` leaves the engine default,
    /// which is auto-detection.
    pub flash_attn: Option<FlashAttn>,
    /// Minimum chunk size to reuse from the prompt cache via KV shifting
    /// (`--cache-reuse`). `None` leaves the engine default.
    pub cache_reuse: Option<u32>,
    /// Expose the Prometheus metrics endpoint (`--metrics`). Off by default and
    /// opt-in only.
    ///
    /// The engine's introspection endpoints are not free of consequence: the
    /// slot endpoints can surface prompt content, and the project's first
    /// principle is that no user data leaves the machine without an explicit
    /// action. Binding to loopback is not a reason to expose an endpoint by
    /// default, only a reason it is safe to expose when asked for.
    pub metrics: bool,
    /// Engine log verbosity (`-lv`). `None` leaves the engine default, which
    /// prints neither the device the model loads onto nor the offload tally.
    /// The default here is 4, the lowest level at which the load names both.
    pub log_verbosity: Option<u8>,
    /// CPU threads used during generation (`-t`). `None` leaves the engine
    /// default.
    pub n_threads: Option<i32>,
    /// Context checkpoints kept per slot (`--ctx-checkpoints`). `None` leaves
    /// the engine default.
    pub ctx_checkpoints: Option<u32>,
    /// Token budget for reasoning (`--reasoning-budget`): `-1` unrestricted,
    /// `0` ends the reasoning at once. `None` leaves the engine default.
    pub reasoning_budget: Option<i32>,
    /// Message injected before the end of the reasoning when its budget is
    /// spent (`--reasoning-budget-message`). `None` injects nothing.
    pub reasoning_budget_message: Option<String>,
    /// RoPE base frequency (`--rope-freq-base`). `None` reads it from the
    /// model, which is the automatic mode.
    pub rope_freq_base: Option<f32>,
    /// RoPE frequency scaling factor (`--rope-freq-scale`). `None` reads it
    /// from the model, which is the automatic mode.
    pub rope_freq_scale: Option<f32>,
    /// Keep the key/value cache off the accelerator (`--no-kv-offload`).
    pub no_kv_offload: bool,
    /// Keep the model in memory rather than letting it be swapped (`--mlock`).
    pub mlock: bool,
    /// Load the model without memory-mapping it (`--no-mmap`).
    pub no_mmap: bool,
    /// Engine seed (`--seed`). `None` leaves the engine default, a random seed.
    pub seed: Option<u32>,
    /// Draft model for speculative decoding (`--model-draft`), an absolute
    /// path.
    pub draft_model: Option<String>,
    /// Speculative decoding mode (`--spec-type`). `None` leaves the engine
    /// default, which is off.
    pub spec_type: Option<SpecType>,
    /// File holding a Jinja chat template that replaces the model's own
    /// (`--chat-template-file`).
    pub chat_template_file: Option<String>,
    /// Extra arguments appended verbatim after every generated flag, so a later
    /// entry wins wherever llama-server takes the last occurrence.
    pub extra_args: Vec<String>,
}

/// Speculative decoding mode accepted by `--spec-type`.
///
/// Only the modes the model sheet offers are named, and only those the
/// bundled engine (tag 10092) accepts: a draft model, the model's own
/// multi-token prediction head, and DFlash. A mode absent from that build has
/// no variant, so a preset naming it is refused while it is read rather than
/// by a process that will not start.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SpecType {
    /// No speculative decoding.
    None,
    /// A separate draft model proposes the tokens (`draft-simple`).
    DraftSimple,
    /// The model's multi-token prediction head drafts (`draft-mtp`).
    DraftMtp,
    /// DFlash block drafting (`draft-dflash`).
    DraftDflash,
}

impl fmt::Display for SpecType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::None => "none",
            Self::DraftSimple => "draft-simple",
            Self::DraftMtp => "draft-mtp",
            Self::DraftDflash => "draft-dflash",
        };
        f.write_str(s)
    }
}

impl Default for LlamaServerConfig {
    fn default() -> Self {
        Self {
            model_path: String::new(),
            mmproj_path: None,
            n_ctx: 32_768,
            n_gpu_layers: 999,
            n_batch: None,
            n_ubatch: None,
            n_parallel: Some(1),
            kv_unified: false,
            cont_batching: Some(true),
            cache_type_k: None,
            cache_type_v: None,
            flash_attn: Some(FlashAttn::On),
            cache_reuse: None,
            metrics: false,
            log_verbosity: Some(4),
            n_threads: None,
            ctx_checkpoints: None,
            reasoning_budget: None,
            reasoning_budget_message: None,
            rope_freq_base: None,
            rope_freq_scale: None,
            no_kv_offload: false,
            mlock: false,
            no_mmap: false,
            seed: None,
            draft_model: None,
            spec_type: None,
            chat_template_file: None,
            extra_args: Vec::new(),
        }
    }
}

impl LlamaServerConfig {
    /// The context the engine is launched with (`-c`).
    ///
    /// Under a unified buffer every slot draws on one pool, so the pool holds
    /// one window per slot: eight requests of a batch each keep the window a
    /// single conversation has. Without it the engine splits `-c` across the
    /// slots itself, and the window is passed as is.
    #[must_use]
    pub fn engine_n_ctx(&self) -> u32 {
        if self.kv_unified {
            self.n_ctx
                .saturating_mul(self.n_parallel.unwrap_or(1).max(1))
        } else {
            self.n_ctx
        }
    }
}

/// Build the `llama-server` argument vector for a configuration and a port.
///
/// Pure, so the launch line can be asserted without spawning a process.
pub(crate) fn build_args(config: &LlamaServerConfig, port: u16) -> Vec<String> {
    let mut args: Vec<String> = Vec::with_capacity(24);

    let mut push = |flag: &str, value: String| {
        args.push(flag.to_owned());
        args.push(value);
    };

    push("-m", config.model_path.clone());
    if let Some(projector) = &config.mmproj_path {
        push("--mmproj", projector.clone());
    }
    push("-ngl", config.n_gpu_layers.to_string());
    push("-c", config.engine_n_ctx().to_string());

    if let Some(v) = config.n_batch {
        push("-b", v.to_string());
    }
    if let Some(v) = config.n_ubatch {
        push("-ub", v.to_string());
    }
    if let Some(v) = &config.cache_type_k {
        push("-ctk", v.clone());
    }
    if let Some(v) = &config.cache_type_v {
        push("-ctv", v.clone());
    }
    if let Some(v) = config.cache_reuse {
        push("--cache-reuse", v.to_string());
    }
    if let Some(v) = config.n_parallel {
        push("-np", v.to_string());
    }
    if config.kv_unified {
        args.push("--kv-unified".to_owned());
    }

    if let Some(enabled) = config.cont_batching {
        args.push(if enabled { "-cb" } else { "-nocb" }.to_owned());
    }
    if let Some(mode) = config.flash_attn {
        args.push("--flash-attn".to_owned());
        args.push(mode.to_string());
    }

    args.push("--jinja".to_owned());
    args.push("--reasoning-format".to_owned());
    args.push("auto".to_owned());
    if let Some(v) = config.log_verbosity {
        args.push("-lv".to_owned());
        args.push(v.to_string());
    }
    if config.metrics {
        args.push("--metrics".to_owned());
    }
    push_load_args(config, &mut args);
    args.push("--host".to_owned());
    args.push(HOST.to_owned());
    args.push("--port".to_owned());
    args.push(port.to_string());

    args.extend(config.extra_args.iter().cloned());
    args
}

/// The flags of the model sheet's load settings that the historical command
/// line never carried. Each is absent unless its field is set, so the default
/// configuration still reproduces that line exactly.
fn push_load_args(config: &LlamaServerConfig, args: &mut Vec<String>) {
    let mut push = |flag: &str, value: Option<String>| {
        if let Some(value) = value {
            args.push(flag.to_owned());
            args.push(value);
        }
    };
    push("-t", config.n_threads.map(|v| v.to_string()));
    push(
        "--ctx-checkpoints",
        config.ctx_checkpoints.map(|v| v.to_string()),
    );
    push(
        "--reasoning-budget",
        config.reasoning_budget.map(|v| v.to_string()),
    );
    push(
        "--reasoning-budget-message",
        config.reasoning_budget_message.clone(),
    );
    push(
        "--rope-freq-base",
        config.rope_freq_base.map(|v| v.to_string()),
    );
    push(
        "--rope-freq-scale",
        config.rope_freq_scale.map(|v| v.to_string()),
    );
    push("--seed", config.seed.map(|v| v.to_string()));
    push("--model-draft", config.draft_model.clone());
    push("--spec-type", config.spec_type.map(|v| v.to_string()));
    push("--chat-template-file", config.chat_template_file.clone());
    for (set, flag) in [
        (config.no_kv_offload, "--no-kv-offload"),
        (config.mlock, "--mlock"),
        (config.no_mmap, "--no-mmap"),
    ] {
        if set {
            args.push(flag.to_owned());
        }
    }
}

/// Overlay the `APOLLIA_LLAMA_` environment variables onto a configuration.
///
/// The lookup is injected rather than read from `std::env` directly so tests can
/// drive it without mutating process-global state, which would make them
/// order-dependent.
pub(crate) fn resolve_env_overrides(
    base: &LlamaServerConfig,
    get: impl Fn(&str) -> Option<String>,
) -> LlamaServerConfig {
    let mut cfg = base.clone();

    if let Some(raw) = get(ENV_N_CTX) {
        cfg.n_ctx = parse_required(ENV_N_CTX, &raw, cfg.n_ctx);
    }
    if let Some(raw) = get(ENV_N_GPU_LAYERS) {
        cfg.n_gpu_layers = parse_required(ENV_N_GPU_LAYERS, &raw, cfg.n_gpu_layers);
    }
    if let Some(raw) = get(ENV_N_BATCH) {
        cfg.n_batch = parse_optional(ENV_N_BATCH, &raw, cfg.n_batch);
    }
    if let Some(raw) = get(ENV_N_UBATCH) {
        cfg.n_ubatch = parse_optional(ENV_N_UBATCH, &raw, cfg.n_ubatch);
    }
    if let Some(raw) = get(ENV_N_PARALLEL) {
        cfg.n_parallel = parse_optional(ENV_N_PARALLEL, &raw, cfg.n_parallel);
    }
    if let Some(raw) = get(ENV_CONT_BATCHING) {
        cfg.cont_batching = parse_optional_bool(ENV_CONT_BATCHING, &raw, cfg.cont_batching);
    }
    if let Some(raw) = get(ENV_CACHE_TYPE_K) {
        cfg.cache_type_k = parse_optional(ENV_CACHE_TYPE_K, &raw, cfg.cache_type_k.clone());
    }
    if let Some(raw) = get(ENV_CACHE_TYPE_V) {
        cfg.cache_type_v = parse_optional(ENV_CACHE_TYPE_V, &raw, cfg.cache_type_v.clone());
    }
    if let Some(raw) = get(ENV_FLASH_ATTN) {
        cfg.flash_attn = parse_optional(ENV_FLASH_ATTN, &raw, cfg.flash_attn);
    }
    if let Some(raw) = get(ENV_CACHE_REUSE) {
        cfg.cache_reuse = parse_optional(ENV_CACHE_REUSE, &raw, cfg.cache_reuse);
    }
    if let Some(raw) = get(ENV_METRICS) {
        cfg.metrics = parse_optional_bool(ENV_METRICS, &raw, Some(cfg.metrics)).unwrap_or(false);
    }
    if let Some(raw) = get(ENV_LOG_VERBOSITY) {
        cfg.log_verbosity = parse_optional(ENV_LOG_VERBOSITY, &raw, cfg.log_verbosity);
    }
    if let Some(raw) = get(ENV_EXTRA_ARGS) {
        cfg.extra_args = raw.split_whitespace().map(str::to_owned).collect();
    }

    cfg
}

/// Read one `APOLLIA_LLAMA_` variable from the process environment.
pub(crate) fn env_getter(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

/// Parse an override for a field that always emits a flag.
///
/// An unparseable value keeps `current` and warns, so a malformed variable never
/// fails a spawn.
fn parse_required<T: FromStr>(var: &str, raw: &str, current: T) -> T {
    match raw.trim().parse::<T>() {
        Ok(v) => v,
        Err(_) => {
            tracing::warn!(env_var = %var, value = %raw, "llama.server.config.env.invalid");
            current
        }
    }
}

/// Parse an override for an optional field.
///
/// An empty value clears the field, which omits the flag and restores the engine
/// default. An unparseable value keeps `current` and warns.
fn parse_optional<T: FromStr>(var: &str, raw: &str, current: Option<T>) -> Option<T> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    match trimmed.parse::<T>() {
        Ok(v) => Some(v),
        Err(_) => {
            tracing::warn!(env_var = %var, value = %raw, "llama.server.config.env.invalid");
            current
        }
    }
}

/// Parse an optional boolean override, accepting the usual spellings.
fn parse_optional_bool(var: &str, raw: &str, current: Option<bool>) -> Option<bool> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    match trimmed.to_ascii_lowercase().as_str() {
        "1" | "true" | "on" | "yes" => Some(true),
        "0" | "false" | "off" | "no" => Some(false),
        _ => {
            tracing::warn!(env_var = %var, value = %raw, "llama.server.config.env.invalid");
            current
        }
    }
}

/// Render an optional field for the spawn-time configuration log.
pub(crate) fn display_opt<T: fmt::Display>(value: Option<&T>) -> String {
    match value {
        Some(v) => v.to_string(),
        None => "unset".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Fixed port so the expected vectors are literal.
    const PORT: u16 = 8080;

    /// The launch line as it stood before the parameters became pilotable. Every
    /// assertion below is expressed as a delta against this vector, so any
    /// accidental change to an unrelated flag fails a test.
    fn frozen_baseline() -> Vec<String> {
        [
            "-m",
            "",
            "-ngl",
            "999",
            "-c",
            "32768",
            "-np",
            "1",
            "-cb",
            "--flash-attn",
            "on",
            "--jinja",
            "--reasoning-format",
            "auto",
            "-lv",
            "4",
            "--host",
            "127.0.0.1",
            "--port",
            "8080",
        ]
        .iter()
        .map(|s| (*s).to_owned())
        .collect()
    }

    /// Resolve the default config against a single environment variable.
    fn with_env(var: &str, value: &str) -> LlamaServerConfig {
        let env: HashMap<String, String> = [(var.to_owned(), value.to_owned())].into();
        resolve_env_overrides(&LlamaServerConfig::default(), |name| env.get(name).cloned())
    }

    /// The baseline vector with `patch` applied, as the expected value of a
    /// single-variable override.
    fn baseline_with(patch: impl Fn(&mut Vec<String>)) -> Vec<String> {
        let mut v = frozen_baseline();
        patch(&mut v);
        v
    }

    /// Index just past `-c <n_ctx>`, where the optional flags are emitted.
    const OPTIONAL_BLOCK: usize = 6;

    fn insert(v: &mut Vec<String>, at: usize, flag: &str, value: &str) {
        v.insert(at, value.to_owned());
        v.insert(at, flag.to_owned());
    }

    #[test]
    fn test_build_args_default_reproduces_frozen_baseline() {
        // GIVEN the default configuration, which no caller overrides
        let config = LlamaServerConfig::default();

        // WHEN the argument vector is built
        let args = build_args(&config, PORT);

        // THEN it is the historical launch line, flag for flag and in order
        assert_eq!(args, frozen_baseline());
    }

    #[test]
    fn test_build_args_projector_is_passed_right_after_the_model() {
        // GIVEN a model fetched with its vision projector
        let config = LlamaServerConfig {
            model_path: "/models/Vision-Q4_K_M/Vision-Q4_K_M.gguf".to_owned(),
            mmproj_path: Some("/models/Vision-Q4_K_M/mmproj-Vision-f16.gguf".to_owned()),
            ..LlamaServerConfig::default()
        };

        // WHEN the argument vector is built
        let args = build_args(&config, PORT);

        // THEN the projector follows the model and nothing else moves
        assert_eq!(
            args,
            baseline_with(|v| {
                v[1] = "/models/Vision-Q4_K_M/Vision-Q4_K_M.gguf".to_owned();
                insert(
                    v,
                    2,
                    "--mmproj",
                    "/models/Vision-Q4_K_M/mmproj-Vision-f16.gguf",
                );
            })
        );
    }

    #[test]
    fn test_build_args_unset_optional_fields_emit_no_flag() {
        // GIVEN the default configuration, whose optional fields are all None
        let config = LlamaServerConfig::default();

        // WHEN the argument vector is built
        let args = build_args(&config, PORT);

        // THEN not one of their flags appears
        for flag in ["-b", "-ub", "-ctk", "-ctv", "--cache-reuse"] {
            assert!(!args.iter().any(|a| a == flag), "{flag} should be absent");
        }
    }

    #[test]
    fn test_resolve_env_n_ctx_changes_only_the_context_size() {
        // GIVEN APOLLIA_LLAMA_N_CTX set to a different window
        let config = with_env(ENV_N_CTX, "8192");

        // WHEN the argument vector is built
        let args = build_args(&config, PORT);

        // THEN only the value after -c differs
        assert_eq!(args, baseline_with(|v| v[5] = "8192".to_owned()));
    }

    #[test]
    fn test_resolve_env_n_gpu_layers_changes_only_the_offload_count() {
        // GIVEN APOLLIA_LLAMA_N_GPU_LAYERS set to a partial offload
        let config = with_env(ENV_N_GPU_LAYERS, "24");

        // WHEN the argument vector is built
        let args = build_args(&config, PORT);

        // THEN only the value after -ngl differs
        assert_eq!(args, baseline_with(|v| v[3] = "24".to_owned()));
    }

    #[test]
    fn test_resolve_env_n_batch_changes_only_the_batch_flag() {
        // GIVEN APOLLIA_LLAMA_N_BATCH set
        let config = with_env(ENV_N_BATCH, "1024");

        // WHEN the argument vector is built
        let args = build_args(&config, PORT);

        // THEN -b is added and nothing else moves
        assert_eq!(
            args,
            baseline_with(|v| insert(v, OPTIONAL_BLOCK, "-b", "1024"))
        );
    }

    #[test]
    fn test_resolve_env_n_ubatch_changes_only_the_micro_batch_flag() {
        // GIVEN APOLLIA_LLAMA_N_UBATCH set
        let config = with_env(ENV_N_UBATCH, "2048");

        // WHEN the argument vector is built
        let args = build_args(&config, PORT);

        // THEN -ub is added and nothing else moves
        assert_eq!(
            args,
            baseline_with(|v| insert(v, OPTIONAL_BLOCK, "-ub", "2048"))
        );
    }

    #[test]
    fn test_resolve_env_n_parallel_changes_only_the_slot_count() {
        // GIVEN APOLLIA_LLAMA_N_PARALLEL set to more slots
        let config = with_env(ENV_N_PARALLEL, "4");

        // WHEN the argument vector is built
        let args = build_args(&config, PORT);

        // THEN only the value after -np differs
        assert_eq!(args, baseline_with(|v| v[7] = "4".to_owned()));
    }

    #[test]
    fn test_resolve_env_n_parallel_empty_omits_the_slot_flag() {
        // GIVEN APOLLIA_LLAMA_N_PARALLEL cleared, which is how a sweep reaches
        // the engine's auto slot count
        let config = with_env(ENV_N_PARALLEL, "");

        // WHEN the argument vector is built
        let args = build_args(&config, PORT);

        // THEN -np and its value are gone and nothing else moves
        assert_eq!(
            args,
            baseline_with(|v| {
                v.remove(6);
                v.remove(6);
            })
        );
    }

    #[test]
    fn test_resolve_env_cont_batching_false_emits_the_negative_flag() {
        // GIVEN APOLLIA_LLAMA_CONT_BATCHING disabled. The engine default is
        // enabled, so omitting the flag would not disable it.
        let config = with_env(ENV_CONT_BATCHING, "false");

        // WHEN the argument vector is built
        let args = build_args(&config, PORT);

        // THEN -cb becomes -nocb and nothing else moves
        assert_eq!(args, baseline_with(|v| v[8] = "-nocb".to_owned()));
    }

    #[test]
    fn test_resolve_env_cache_type_k_changes_only_the_key_cache_flag() {
        // GIVEN APOLLIA_LLAMA_CACHE_TYPE_K set to a quantised type
        let config = with_env(ENV_CACHE_TYPE_K, "q8_0");

        // WHEN the argument vector is built
        let args = build_args(&config, PORT);

        // THEN -ctk is added and nothing else moves
        assert_eq!(
            args,
            baseline_with(|v| insert(v, OPTIONAL_BLOCK, "-ctk", "q8_0"))
        );
    }

    #[test]
    fn test_resolve_env_cache_type_v_changes_only_the_value_cache_flag() {
        // GIVEN APOLLIA_LLAMA_CACHE_TYPE_V set to a quantised type
        let config = with_env(ENV_CACHE_TYPE_V, "q8_0");

        // WHEN the argument vector is built
        let args = build_args(&config, PORT);

        // THEN -ctv is added and nothing else moves
        assert_eq!(
            args,
            baseline_with(|v| insert(v, OPTIONAL_BLOCK, "-ctv", "q8_0"))
        );
    }

    #[test]
    fn test_resolve_env_flash_attn_changes_only_the_flash_attention_mode() {
        // GIVEN APOLLIA_LLAMA_FLASH_ATTN set to auto
        let config = with_env(ENV_FLASH_ATTN, "auto");

        // WHEN the argument vector is built
        let args = build_args(&config, PORT);

        // THEN only the value after --flash-attn differs
        assert_eq!(args, baseline_with(|v| v[10] = "auto".to_owned()));
    }

    #[test]
    fn test_resolve_env_cache_reuse_changes_only_the_cache_reuse_flag() {
        // GIVEN APOLLIA_LLAMA_CACHE_REUSE set to a reuse threshold
        let config = with_env(ENV_CACHE_REUSE, "256");

        // WHEN the argument vector is built
        let args = build_args(&config, PORT);

        // THEN --cache-reuse is added and nothing else moves
        assert_eq!(
            args,
            baseline_with(|v| insert(v, OPTIONAL_BLOCK, "--cache-reuse", "256"))
        );
    }

    #[test]
    fn test_build_args_metrics_endpoint_is_off_by_default() {
        // GIVEN the default configuration, which does not opt into metrics
        let config = LlamaServerConfig::default();

        // WHEN the argument vector is built
        let args = build_args(&config, PORT);

        // THEN the endpoint flag is absent, so no introspection surface is opened
        assert!(!args.iter().any(|a| a == "--metrics"));
    }

    #[test]
    fn test_resolve_env_metrics_opt_in_adds_only_the_metrics_flag() {
        // GIVEN APOLLIA_LLAMA_METRICS explicitly enabled
        let config = with_env(ENV_METRICS, "1");

        // WHEN the argument vector is built
        let args = build_args(&config, PORT);

        // THEN --metrics is added just before --host and nothing else moves
        assert_eq!(
            args,
            baseline_with(|v| v.insert(16, "--metrics".to_owned()))
        );
    }

    #[test]
    fn test_resolve_env_metrics_invalid_value_stays_off() {
        // GIVEN a malformed APOLLIA_LLAMA_METRICS
        let config = with_env(ENV_METRICS, "sure");

        // WHEN the argument vector is built
        let args = build_args(&config, PORT);

        // THEN the endpoint stays closed, since a typo must never open a surface
        assert_eq!(args, frozen_baseline());
    }

    #[test]
    fn test_resolve_env_extra_args_are_appended_last() {
        // GIVEN APOLLIA_LLAMA_EXTRA_ARGS carrying two tokens
        let config = with_env(ENV_EXTRA_ARGS, "--mlock --no-warmup");

        // WHEN the argument vector is built
        let args = build_args(&config, PORT);

        // THEN they land after every generated flag, in order
        assert_eq!(
            args,
            baseline_with(|v| {
                v.push("--mlock".to_owned());
                v.push("--no-warmup".to_owned());
            })
        );
    }

    #[test]
    fn test_resolve_env_invalid_value_keeps_the_baseline() {
        // GIVEN a malformed APOLLIA_LLAMA_N_BATCH
        let config = with_env(ENV_N_BATCH, "not-a-number");

        // WHEN the argument vector is built
        let args = build_args(&config, PORT);

        // THEN the launch line is untouched, so a typo cannot alter the engine
        assert_eq!(args, frozen_baseline());
    }

    #[test]
    fn test_resolve_env_invalid_flash_attn_keeps_the_baseline() {
        // GIVEN an APOLLIA_LLAMA_FLASH_ATTN value outside on, off, auto
        let config = with_env(ENV_FLASH_ATTN, "yes-please");

        // WHEN the argument vector is built
        let args = build_args(&config, PORT);

        // THEN the mode stays as it was
        assert_eq!(args, frozen_baseline());
    }

    #[test]
    fn test_resolve_env_no_variables_set_returns_the_default() {
        // GIVEN an empty environment
        let config = resolve_env_overrides(&LlamaServerConfig::default(), |_| None);

        // WHEN the argument vector is built
        let args = build_args(&config, PORT);

        // THEN resolution is a no-op over the default configuration
        assert_eq!(args, frozen_baseline());
    }

    #[test]
    fn test_resolve_env_does_not_override_the_model_path() {
        // GIVEN a model chosen by switch_model and an ambient variable that
        // names the same field
        let base = LlamaServerConfig {
            model_path: "/models/chosen.gguf".to_owned(),
            ..LlamaServerConfig::default()
        };
        let env: HashMap<String, String> = [(
            "APOLLIA_LLAMA_MODEL_PATH".to_owned(),
            "/models/other.gguf".to_owned(),
        )]
        .into();

        // WHEN the environment is resolved
        let config = resolve_env_overrides(&base, |name| env.get(name).cloned());

        // THEN the chosen model still wins
        assert_eq!(config.model_path, "/models/chosen.gguf");
    }

    #[test]
    fn test_resolve_env_log_verbosity_changes_only_the_lv_flag() {
        // GIVEN APOLLIA_LLAMA_LOG_VERBOSITY asking for the engine's quietest level
        let config = with_env(ENV_LOG_VERBOSITY, "1");

        // WHEN the argument vector is built
        let args = build_args(&config, PORT);

        // THEN only the value after -lv differs
        assert_eq!(args, baseline_with(|v| v[15] = "1".to_owned()));
    }

    #[test]
    fn test_build_args_no_log_verbosity_emits_no_flag() {
        // GIVEN a configuration that leaves the engine's own verbosity in force.
        // This is the control for the default: without it, a build that always
        // printed `-lv 4` would pass the baseline test for the wrong reason.
        let config = LlamaServerConfig {
            log_verbosity: None,
            ..LlamaServerConfig::default()
        };

        // WHEN the argument vector is built
        let args = build_args(&config, PORT);

        // THEN the flag is absent and nothing else moves
        assert_eq!(
            args,
            baseline_with(|v| {
                v.remove(14);
                v.remove(14);
            })
        );
    }

    #[test]
    fn test_build_args_all_optional_fields_emit_in_declaration_order() {
        // GIVEN every optional field populated
        let config = LlamaServerConfig {
            model_path: "/models/m.gguf".to_owned(),
            n_batch: Some(4096),
            n_ubatch: Some(1024),
            cache_type_k: Some("q8_0".to_owned()),
            cache_type_v: Some("q8_0".to_owned()),
            cache_reuse: Some(256),
            extra_args: vec!["--mlock".to_owned()],
            ..LlamaServerConfig::default()
        };

        // WHEN the argument vector is built
        let args = build_args(&config, PORT);

        // THEN the whole line is exactly as declared
        let expected: Vec<String> = [
            "-m",
            "/models/m.gguf",
            "-ngl",
            "999",
            "-c",
            "32768",
            "-b",
            "4096",
            "-ub",
            "1024",
            "-ctk",
            "q8_0",
            "-ctv",
            "q8_0",
            "--cache-reuse",
            "256",
            "-np",
            "1",
            "-cb",
            "--flash-attn",
            "on",
            "--jinja",
            "--reasoning-format",
            "auto",
            "-lv",
            "4",
            "--host",
            "127.0.0.1",
            "--port",
            "8080",
            "--mlock",
        ]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
        assert_eq!(args, expected);
    }
}
