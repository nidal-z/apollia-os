//! How many requests the engine serves at once.
//!
//! The launch used to pass `-np 1`, so the engine served one request at a time
//! even when an agent's `ctx.llm.map` sent eight together: the batch queued
//! behind its own first item, and continuous batching had nothing to batch.
//!
//! The count is now planned from the model's own header and the machine's
//! memory, through the same fit arithmetic that grades the model: the largest
//! number of slots, up to [`MAX_SLOTS`], whose windows of cache still fit
//! beside the weights. The slots share one key/value buffer (`--kv-unified`)
//! sized to hold a full window each, so a request in a batch keeps the window a
//! single conversation has.

use super::config::{env_getter, LlamaServerConfig, ENV_N_PARALLEL};
use super::ngl::kv_element_bytes;

/// The most slots ever planned.
///
/// It is the number of requests `ctx.llm.map` keeps in flight when the caller
/// does not say otherwise: a slot beyond it would hold memory no request of a
/// batch ever reaches.
pub(super) const MAX_SLOTS: u32 = 8;

/// Replace the configured slot count with a planned one, when that is ours to
/// decide.
///
/// An operator who set `APOLLIA_LLAMA_N_PARALLEL`, even to the empty value that
/// leaves the count to the engine, has said what they want, and is left alone.
pub(super) fn apply_parallel_plan(config: &mut LlamaServerConfig) {
    apply_with(config, env_getter, plan_slots);
}

/// [`apply_parallel_plan`] with its two side effects injected, so the
/// precedence of the environment is testable without touching the process
/// environment or the disk.
fn apply_with(
    config: &mut LlamaServerConfig,
    get: impl Fn(&str) -> Option<String>,
    plan: impl FnOnce(&LlamaServerConfig) -> Option<u32>,
) {
    if get(ENV_N_PARALLEL).is_some() {
        return;
    }
    if let Some(slots) = plan(config) {
        config.n_parallel = Some(slots);
        config.kv_unified = true;
    }
}

/// The settings of one slot, as the recommender reads them.
fn per_slot_shape(config: &LlamaServerConfig) -> apollia_llm::recommend::RuntimeShape {
    apollia_llm::recommend::RuntimeShape {
        n_ctx: config.n_ctx,
        kv_element_bytes: kv_element_bytes(config.cache_type_k.as_deref()),
        n_parallel: 1,
        n_ubatch: config.n_ubatch.unwrap_or(512),
    }
}

/// How many slots to launch the model with, from the model and the machine.
///
/// Returns `None` when the model file cannot be read, which leaves the
/// configured count in force. An unreadable header plans one slot, through
/// [`apollia_llm::recommend::max_parallel_for`].
fn plan_slots(config: &LlamaServerConfig) -> Option<u32> {
    let path = std::path::Path::new(&config.model_path);
    let weights_bytes = std::fs::metadata(path).ok()?.len();
    let profile = apollia_llm::hardware::detect();
    let facts = match apollia_llm::gguf_probe::probe_file(
        path,
        apollia_llm::gguf_probe::SCREEN_BYTES,
    ) {
        Ok(facts) => facts,
        Err(e) => {
            tracing::debug!(model = %config.model_path, error = %e, "llama.server.parallel.probe_failed");
            apollia_llm::gguf_probe::GgufHeaderFacts::default()
        }
    };
    let slots = apollia_llm::recommend::max_parallel_for(
        weights_bytes,
        &facts,
        &profile,
        &per_slot_shape(config),
        MAX_SLOTS,
    );
    tracing::info!(
        model = %config.model_path,
        n_parallel = slots,
        n_ctx_per_slot = config.n_ctx,
        memory_budget_gb = profile.memory_budget_gb,
        "llama.server.parallel.planned"
    );
    Some(slots)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llama_server::config::{build_args, resolve_env_overrides};
    use crate::llama_server::ngl::runtime_shape;
    use apollia_llm::recommend::parallel_shape;

    fn no_env(_: &str) -> Option<String> {
        None
    }

    #[test]
    fn a_planned_count_is_launched_with_a_unified_buffer() {
        // GIVEN the default configuration and no variable set
        let mut config = LlamaServerConfig::default();

        // WHEN the planner answers eight slots
        apply_with(&mut config, no_env, |_| Some(8));

        // THEN the engine gets eight slots sharing one buffer of eight windows
        let args = build_args(&config, 8080);
        let at = |flag: &str| args.iter().position(|a| a == flag);
        let np = at("-np").map(|i| args[i + 1].clone());
        let ctx = at("-c").map(|i| args[i + 1].clone());
        assert_eq!(np.as_deref(), Some("8"));
        assert_eq!(ctx.as_deref(), Some("262144"));
        assert!(at("--kv-unified").is_some());
    }

    #[test]
    fn the_variable_keeps_priority_over_the_plan() {
        // GIVEN an operator who pinned three slots
        let get = |name: &str| (name == ENV_N_PARALLEL).then(|| "3".to_owned());
        let mut config = resolve_env_overrides(&LlamaServerConfig::default(), get);

        // WHEN the plan is applied, with a planner that would answer eight
        let mut asked = false;
        apply_with(&mut config, get, |_| {
            asked = true;
            Some(8)
        });

        // THEN the pinned value stands, the planner is not consulted, and the
        // engine splits its context as it always did
        assert_eq!(config.n_parallel, Some(3));
        assert!(!asked);
        assert!(!config.kv_unified);
        assert_eq!(config.engine_n_ctx(), config.n_ctx);
    }

    #[test]
    fn an_unreadable_model_keeps_the_configured_count() {
        // GIVEN a planner that cannot read the model
        let mut config = LlamaServerConfig::default();

        // WHEN the plan is applied
        apply_with(&mut config, no_env, |_| None);

        // THEN the single slot of the default stands, without a unified buffer
        assert_eq!(config.n_parallel, Some(1));
        assert!(!config.kv_unified);
    }

    #[test]
    fn the_offload_plan_counts_the_slots_that_were_planned() {
        // GIVEN a configuration whose per-request window is 16384 tokens with
        // a q8 cache
        let mut config = LlamaServerConfig {
            n_ctx: 16_384,
            cache_type_k: Some("q8_0".to_owned()),
            ..LlamaServerConfig::default()
        };
        let per_slot = per_slot_shape(&config);

        // WHEN five slots are planned
        apply_with(&mut config, no_env, |_| Some(5));

        // THEN the shape the `-ngl` plan sizes is exactly the one the slot
        // count was chosen against: five windows, five slots, same element
        assert_eq!(runtime_shape(&config), parallel_shape(&per_slot, 5));
        assert_eq!(runtime_shape(&config).n_ctx, 5 * 16_384);
        assert_eq!(runtime_shape(&config).kv_element_bytes, 1);
    }
}
