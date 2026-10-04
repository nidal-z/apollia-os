//! The context window a local model is launched with when nothing closer to
//! it fixes one.
//!
//! The window of a launch is resolved in this order, the first that speaks
//! wins:
//!
//! 1. `APOLLIA_LLAMA_N_CTX`, the environment;
//! 2. the window of the model's preset;
//! 3. the `context_window` of the backend;
//! 4. the default of the settings, `[llm.defaults]` in `apollia.toml`;
//! 5. the default the engine is built with, 32768 tokens.
//!
//! The default of the settings is one of three things. A figure is taken as
//! written. `model_max` is the length the model was trained for. And when no
//! one chose a figure, the window is sized to the machine: the longest one, up
//! to the built-in default, at which the estimator that grades the load
//! guardrail still finds the model within the memory budget. Each is capped at
//! the training length, and a model that cannot be read falls to the last
//! step.
//!
//! The section is read at every load, so a change applies to the next model
//! loaded. The organisation policy can fix its two keys.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use apollia_core::config::{
    resolve_setting, ContextDefault, PolicySettings, CONTEXT_MODE_KEY, CONTEXT_TOKENS_KEY,
};
use apollia_llm::context_window::cap_at_trained;
use apollia_llm::hardware::HardwareProfile;
use apollia_llm::recommend::{MemoryArchitecture, RuntimeShape};

/// Smallest window the sizing to the machine settles on: under it a
/// conversation with tools does not hold, and the load guardrail is what
/// refuses a model that does not fit.
pub const MIN_FITTED_N_CTX: u32 = 4_096;

/// The window a launch gets: the environment's, else the one asked for (the
/// preset's, then the backend's), else `model_default`, which carries the
/// default of the settings and, last, the built-in one.
#[must_use]
pub fn resolve(env: Option<u32>, asked: Option<u32>, model_default: u32) -> u32 {
    env.or(asked.filter(|n| *n > 0)).unwrap_or(model_default)
}

/// The default in force now, read from `path` under the settings the
/// organisation policy fixes.
///
/// A missing file is the default of the section. A section that cannot be
/// read leaves the sizing to the machine, with a warning: a typo in the file
/// must not ask for the largest window.
#[must_use]
pub fn read(path: Option<&Path>, imposed: &PolicySettings) -> ContextDefault {
    let Some(path) = path else {
        return ContextDefault::Machine;
    };
    let mode = resolve_setting(path, CONTEXT_MODE_KEY, imposed);
    let tokens = resolve_setting(path, CONTEXT_TOKENS_KEY, imposed);
    match (mode, tokens) {
        (Ok(mode), Ok(tokens)) => ContextDefault::of(
            mode.value.as_str().unwrap_or_default(),
            tokens.value.as_u64().unwrap_or(0),
        ),
        (Err(error), _) | (_, Err(error)) => {
            tracing::event!(
                tracing::Level::WARN,
                path = %path.display(),
                error = %error,
                detail = "the window is sized to the machine",
                "llm.context_default.config_unreadable"
            );
            ContextDefault::Machine
        }
    }
}

/// The window `default` gives a model trained for `trained` tokens, or `None`
/// when it says nothing for this model: `model_max` on a model that does not
/// declare its length, or a sizing the estimator could not make.
///
/// `fitting` answers the longest window, up to the ceiling it is handed, at
/// which the model fits the memory budget.
#[must_use]
pub fn setting_window(
    default: ContextDefault,
    trained: Option<u64>,
    built_in: u32,
    fitting: impl FnOnce(u32) -> Option<u32>,
) -> Option<u32> {
    match default {
        ContextDefault::Tokens(tokens) => Some(cap_at_trained(tokens, trained)),
        ContextDefault::ModelMax => trained
            .and_then(|t| u32::try_from(t).ok())
            .filter(|t| *t > 0),
        ContextDefault::Machine => {
            let ceiling = cap_at_trained(built_in, trained);
            fitting(ceiling).map(|fitted| fitted.clamp(MIN_FITTED_N_CTX.min(ceiling), ceiling))
        }
    }
}

/// The longest window up to `ceiling`, in steps of 1024 tokens, that `fits`
/// accepts; `None` when not even the shortest does.
fn longest_fitting(ceiling: u32, fits: impl Fn(u32) -> bool) -> Option<u32> {
    if fits(ceiling) {
        return Some(ceiling);
    }
    let (mut low, mut high) = (0u32, ceiling);
    while high - low > 1024 {
        let mid = low + (high - low) / 2;
        if fits(mid) {
            low = mid;
        } else {
            high = mid;
        }
    }
    Some((low / 1024) * 1024).filter(|n| *n > 0)
}

/// What a model on disk says of itself: its training length, and the longest
/// window that fits `profile` up to a ceiling.
struct ModelOnDisk<'a> {
    path: &'a Path,
}

impl ModelOnDisk<'_> {
    fn gguf_facts(&self) -> Option<apollia_llm::gguf_probe::GgufHeaderFacts> {
        apollia_llm::gguf_probe::probe_file(self.path, apollia_llm::gguf_probe::SCREEN_BYTES).ok()
    }

    /// The window `default` gives this model, the built-in one when it gives
    /// none, capped at the training length either way.
    fn window(&self, default: ContextDefault, built_in: u32) -> u32 {
        let shape = |n_ctx| RuntimeShape {
            n_ctx,
            ..RuntimeShape::default()
        };
        if self.path.is_dir() {
            let trained = crate::mlx_engine::trained_context(self.path);
            let fitted = setting_window(default, trained, built_in, |ceiling| {
                let profile = apollia_llm::hardware::detect();
                longest_fitting(ceiling, |n_ctx| {
                    apollia_llm::recommend::estimate_mlx_memory(self.path, &shape(n_ctx))
                        .is_ok_and(|estimate| estimate.total_gb < profile.memory_budget_gb)
                })
            });
            return fitted.unwrap_or_else(|| cap_at_trained(built_in, trained));
        }
        let facts = self.gguf_facts();
        let trained = facts.as_ref().and_then(|f| f.context_length);
        let fitted = setting_window(default, trained, built_in, |ceiling| {
            let facts = facts.as_ref()?;
            let size = std::fs::metadata(self.path).ok()?.len();
            let profile: HardwareProfile = apollia_llm::hardware::detect();
            let accelerated = MemoryArchitecture::of(&profile).uses_accelerator();
            apollia_llm::recommend::max_context_for(
                size,
                facts,
                &shape(ceiling),
                &profile,
                accelerated,
            )
        });
        fitted.unwrap_or_else(|| cap_at_trained(built_in, trained))
    }
}

/// The default window of each model an engine launches, read from the
/// settings at every load.
///
/// Sizing a window reads the header of the model and the hardware, so the
/// answer for a model under a given default is kept: a request to a resident
/// model does not probe its file again.
#[derive(Debug, Default)]
pub struct SizedWindows {
    sized: Mutex<HashMap<(String, ContextDefault), u32>>,
}

impl SizedWindows {
    /// The window `model_path` is launched with when neither the environment,
    /// nor a preset, nor its backend fixes one: the default `config_file`,
    /// `apollia.toml`, states under the settings the policy fixes in
    /// `imposed`, else the engine's `built_in` window. Reads the file, and the
    /// model the first time: call it off the async workers.
    #[must_use]
    pub fn window_blocking(
        &self,
        config_file: Option<&Path>,
        imposed: &PolicySettings,
        built_in: u32,
        model_path: &str,
    ) -> u32 {
        let default = read(config_file, imposed);
        let key = (model_path.to_owned(), default);
        if let Some(known) = self.sized.lock().ok().and_then(|s| s.get(&key).copied()) {
            return known;
        }
        let window = ModelOnDisk {
            path: Path::new(model_path),
        }
        .window(default, built_in);
        tracing::event!(
            tracing::Level::DEBUG,
            model = %model_path,
            n_ctx = window,
            default = ?default,
            "llm.context_default.resolved"
        );
        if let Ok(mut sized) = self.sized.lock() {
            sized.insert(key, window);
        }
        window
    }

    /// [`window_blocking`](Self::window_blocking), off the async workers. A
    /// read that panicked answers the built-in window.
    pub async fn window(
        self: &Arc<Self>,
        config_file: Option<PathBuf>,
        imposed: PolicySettings,
        built_in: u32,
        model_path: &str,
    ) -> u32 {
        let sized = Arc::clone(self);
        let model_path = model_path.to_owned();
        tokio::task::spawn_blocking(move || {
            sized.window_blocking(config_file.as_deref(), &imposed, built_in, &model_path)
        })
        .await
        .unwrap_or(built_in)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(text: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("apollia.toml");
        std::fs::write(&path, text).expect("write");
        (dir, path)
    }

    #[test]
    fn test_the_default_of_the_settings_sits_between_the_backend_and_the_built_in_window() {
        // GIVEN a file at 16384 tokens, another that says nothing, and a model
        //       that cannot be read
        let (_dir, file) = config("[llm.defaults]\ncontext_tokens = 16384\n");
        let (_other, silent) = config("");
        let none = PolicySettings::none();
        let sized = SizedWindows::default();
        let model = "/nonexistent/model.gguf";

        // WHEN the default of the model is read under each
        let stated = sized.window_blocking(Some(&file), &none, 32_768, model);
        let unstated = sized.window_blocking(Some(&silent), &none, 32_768, model);

        // THEN the figure of the settings applies, and 32768 is the last
        //      resort; a window asked for wins over either, and the
        //      environment over all
        assert_eq!((stated, unstated), (16_384, 32_768));
        assert_eq!(resolve(None, None, stated), 16_384);
        assert_eq!(resolve(None, Some(12_288), stated), 12_288);
        assert_eq!(resolve(Some(4_096), Some(12_288), stated), 4_096);
        assert_eq!(resolve(None, Some(0), unstated), 32_768);
    }

    #[test]
    fn test_a_figure_and_the_model_maximum_are_capped_at_the_training_length() {
        // GIVEN a model trained for 8192 tokens
        let trained = Some(8_192);

        // WHEN a figure above it, the model maximum, and a figure under it
        //      are resolved
        let above = setting_window(ContextDefault::Tokens(65_536), trained, 32_768, |_| None);
        let maximum = setting_window(ContextDefault::ModelMax, trained, 32_768, |_| None);
        let under = setting_window(ContextDefault::Tokens(4_096), trained, 32_768, |_| None);

        // THEN the first two are the training length and the third is kept
        assert_eq!(
            (above, maximum, under),
            (Some(8_192), Some(8_192), Some(4_096))
        );
    }

    #[test]
    fn test_the_model_maximum_of_a_model_that_does_not_say_gives_nothing() {
        // GIVEN a model whose header declares no training length
        // WHEN the model maximum is resolved for it
        let window = setting_window(ContextDefault::ModelMax, None, 32_768, |_| Some(1));

        // THEN the setting says nothing, and the built-in window applies
        assert_eq!(window, None);
    }

    #[test]
    fn test_without_a_figure_the_window_is_the_longest_that_fits_the_machine() {
        // GIVEN a model trained for 131072 tokens on a machine that holds
        //       12288 tokens of it, then 1024, then all of it
        let trained = Some(131_072);
        let sized = |fits: u32| {
            setting_window(ContextDefault::Machine, trained, 32_768, |ceiling| {
                assert_eq!(ceiling, 32_768, "sized up to the built-in window");
                longest_fitting(ceiling, |n_ctx| n_ctx <= fits)
            })
        };

        // WHEN the window is sized to the machine
        let (reduced, floor, whole) = (sized(12_288), sized(1_024), sized(200_000));

        // THEN it is the longest that fits, never under the floor, and never
        //      past the built-in window
        assert_eq!(reduced, Some(12_288));
        assert_eq!(floor, Some(MIN_FITTED_N_CTX));
        assert_eq!(whole, Some(32_768));
    }

    #[test]
    fn test_a_policy_fixes_the_default_over_the_file() {
        // GIVEN a file at 16384 tokens and a policy that fixes 8192
        let (_dir, file) = config("[llm.defaults]\ncontext_tokens = 16384\n");
        let imposed = PolicySettings::none().impose(CONTEXT_TOKENS_KEY, serde_json::json!(8_192));

        // WHEN the default is read under the policy, then without it
        let fixed = read(Some(&file), &imposed);
        let free = read(Some(&file), &PolicySettings::none());

        // THEN the policy wins
        assert_eq!(fixed, ContextDefault::Tokens(8_192));
        assert_eq!(free, ContextDefault::Tokens(16_384));
    }

    #[test]
    fn test_an_unreadable_section_leaves_the_sizing_to_the_machine() {
        // GIVEN a file whose default window is a string
        let (_dir, file) = config("[llm.defaults]\ncontext_tokens = \"many\"\n");

        // WHEN the default is read
        let default = read(Some(&file), &PolicySettings::none());

        // THEN no figure is taken from it
        assert_eq!(default, ContextDefault::Machine);
    }
}
