//! How many layers the engine is asked to place on the accelerator.
//!
//! The launch used to pass a literal `-ngl 999`, which says "put every layer on
//! the accelerator". That is right up to the point where they do not fit. Past
//! it the engine fails to allocate rather than placing what it can, so a card
//! smaller than the model ran nothing at all, on a machine with plenty of
//! system memory to hold the rest.
//!
//! So on a discrete accelerator the count is planned from the model's own
//! header and the machine's memory, through the same placement arithmetic the
//! recommender uses to rank the model in the first place. Everywhere else the
//! literal stays, for the reasons on [`plan_gpu_layers`].

use super::config::{env_getter, LlamaServerConfig, ENV_N_GPU_LAYERS};

/// Replace the configured offload count with a planned one, when that is ours
/// to decide.
///
/// An operator who set `APOLLIA_LLAMA_N_GPU_LAYERS` has said what they want,
/// and is left alone.
pub(super) fn apply_offload_plan(config: &mut LlamaServerConfig) {
    if env_getter(ENV_N_GPU_LAYERS).is_some() {
        return;
    }
    if let Some(planned) = plan_gpu_layers(config) {
        config.n_gpu_layers = planned;
    }
}

/// How many layers to place on the accelerator, from the model and the machine.
///
/// Returns `None` whenever the answer should be left alone, which covers more
/// cases than it decides:
///
/// - **unified memory**, where there is one pool and every layer belongs on the
///   accelerator regardless;
/// - **no detected accelerator**, because a detection that missed a device
///   would otherwise turn `-ngl 0` into a silent loss of acceleration, and the
///   engine already ignores a high count when there is nothing to offload to;
/// - **an unreadable header**, where guessing is worse than the default.
///
/// So this only ever speaks for a discrete device, which is the only case where
/// the unconditional 999 was wrong.
fn plan_gpu_layers(config: &LlamaServerConfig) -> Option<i32> {
    use apollia_llm::recommend::{MemoryArchitecture, RuntimeShape};

    let path = std::path::Path::new(&config.model_path);
    let weights_bytes = std::fs::metadata(path).ok()?.len();

    let profile = apollia_llm::hardware::detect();
    if !matches!(
        MemoryArchitecture::of(&profile),
        MemoryArchitecture::Discrete { .. }
    ) {
        return None;
    }

    let facts = match apollia_llm::gguf_probe::probe_file(
        path,
        apollia_llm::gguf_probe::SCREEN_BYTES,
    ) {
        Ok(facts) => facts,
        Err(e) => {
            tracing::debug!(model = %config.model_path, error = %e, "llama.server.ngl.probe_failed");
            return None;
        }
    };

    let shape = RuntimeShape {
        n_ctx: config.n_ctx,
        kv_element_bytes: kv_element_bytes(config.cache_type_k.as_deref()),
        n_parallel: config.n_parallel.unwrap_or(1),
        n_ubatch: config.n_ubatch.unwrap_or(512),
    };

    let plan = apollia_llm::recommend::plan_offload(weights_bytes, &facts, &profile, &shape);
    tracing::info!(
        model = %config.model_path,
        n_gpu_layers = plan.n_gpu_layers,
        total_layers = plan.total_layers,
        fully_offloaded = plan.fully_offloaded,
        "llama.server.ngl.planned"
    );
    i32::try_from(plan.n_gpu_layers).ok()
}

/// Bytes per key/value cache element implied by the configured cache type.
///
/// The engine defaults to `f16` when `-ctk` is not given, which is what the
/// runtime ships.
fn kv_element_bytes(cache_type_k: Option<&str>) -> u32 {
    match cache_type_k {
        Some("q8_0") | Some("q8_1") => 1,
        Some("f32") => 4,
        _ => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_engine_default_cache_is_sized_as_f16() {
        // GIVEN a configuration that leaves `-ctk` to the engine
        // WHEN the element width is derived
        // THEN it is the two bytes of the engine's own f16 default
        assert_eq!(kv_element_bytes(None), 2);
        assert_eq!(kv_element_bytes(Some("f16")), 2);
    }

    #[test]
    fn a_quantised_or_widened_cache_changes_the_element_width() {
        // GIVEN the cache types an operator can set through the environment
        // WHEN each is converted
        // THEN q8 halves the reservation and f32 doubles it, which moves how
        // many layers the plan can place
        assert_eq!(kv_element_bytes(Some("q8_0")), 1);
        assert_eq!(kv_element_bytes(Some("f32")), 4);
    }
}
