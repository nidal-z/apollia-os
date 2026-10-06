//! What every local model receives when nothing closer to it says otherwise:
//! the default context window (`[llm.defaults]`) and the ceiling on the size
//! of an image sent to a model (`[llm.images]`), and the graphics devices held
//! back from the local engine (`[llm.devices]`).

use serde::{Deserialize, Serialize};

/// Dotted key of the way the default context window is chosen.
pub const CONTEXT_MODE_KEY: &str = "llm.defaults.context";

/// Dotted key of the default context window, in tokens.
pub const CONTEXT_TOKENS_KEY: &str = "llm.defaults.context_tokens";

/// Dotted key of the switch that holds an image to a largest side.
pub const IMAGE_LIMIT_KEY: &str = "llm.images.limit_side";

/// Dotted key of the largest side of an image, in pixels.
pub const IMAGE_MAX_SIDE_KEY: &str = "llm.images.max_side_px";

/// Dotted key of what "load anyway" asks before going past a refusal.
pub const LOAD_BYPASS_KEY: &str = "llm.guardrails.bypass";

/// Dotted key of the graphics devices held back from the local engine.
pub const DISABLED_DEVICES_KEY: &str = "llm.devices.disabled";

/// The values `llm.defaults.context` accepts.
pub const CONTEXT_MODES: &[&str] = &["custom", "model_max"];

/// The values `llm.guardrails.bypass` accepts.
pub const LOAD_BYPASS_MODES: &[&str] = &["confirm", "unrestricted"];

/// Largest side of an image, in pixels, when the key is absent.
pub const DEFAULT_IMAGE_MAX_SIDE_PX: u32 = 2048;

/// The default context window of a local model, as the settings state it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContextDefault {
    /// No one chose a figure: the engine sizes the window to what this
    /// machine holds, model by model.
    Machine,
    /// A window of this many tokens.
    Tokens(u32),
    /// The length the model was trained for.
    ModelMax,
}

/// The `[llm.defaults]` section: the context window of a local model that
/// neither `APOLLIA_LLAMA_N_CTX`, nor a preset, nor its backend fixes.
///
/// The engines read it at every load, so a change applies to the next model
/// loaded, without a restart.
///
/// ```toml
/// [llm.defaults]
/// context = "custom"       # or "model_max"
/// context_tokens = 16384   # 0 lets the engine size it to this machine
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LlmDefaultsConfig {
    /// `custom`, a figure, or `model_max`, the model's training length. An
    /// unknown value reads as `custom`.
    #[serde(default = "default_context_mode")]
    pub context: String,
    /// The figure of `custom`, in tokens. `0` leaves it to the engine, which
    /// sizes the window to the memory of the machine.
    #[serde(default)]
    pub context_tokens: u32,
}

fn default_context_mode() -> String {
    "custom".to_owned()
}

impl Default for LlmDefaultsConfig {
    fn default() -> Self {
        Self {
            context: default_context_mode(),
            context_tokens: 0,
        }
    }
}

impl ContextDefault {
    /// The default a mode and a figure state. A mode other than `model_max`
    /// reads as `custom`, so a typo never asks for the largest window.
    #[must_use]
    pub fn of(mode: &str, tokens: u64) -> Self {
        if mode == "model_max" {
            return Self::ModelMax;
        }
        match u32::try_from(tokens) {
            Ok(0) => Self::Machine,
            Ok(tokens) => Self::Tokens(tokens),
            Err(_) => Self::Tokens(u32::MAX),
        }
    }
}

impl LlmDefaultsConfig {
    /// The default this section states.
    #[must_use]
    pub fn context_default(&self) -> ContextDefault {
        ContextDefault::of(&self.context, u64::from(self.context_tokens))
    }
}

/// The `[llm.images]` section: the largest side an image keeps when it is
/// sent to a model that reads images.
///
/// ```toml
/// [llm.images]
/// limit_side = true
/// max_side_px = 2048
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LlmImagesConfig {
    /// Whether a larger image is scaled down before it is sent. Off, an image
    /// travels at its own size, up to what the engine accepts.
    #[serde(default = "default_limit_side")]
    pub limit_side: bool,
    /// Largest side, in pixels; the other side follows, proportions kept.
    #[serde(default = "default_max_side_px")]
    pub max_side_px: u32,
}

fn default_limit_side() -> bool {
    true
}

fn default_max_side_px() -> u32 {
    DEFAULT_IMAGE_MAX_SIDE_PX
}

impl Default for LlmImagesConfig {
    fn default() -> Self {
        Self {
            limit_side: default_limit_side(),
            max_side_px: DEFAULT_IMAGE_MAX_SIDE_PX,
        }
    }
}

/// The `[llm.devices]` section: the graphics devices the local engine is
/// kept away from.
///
/// Each entry is an identifier the engine prints with `--list-devices`
/// (`CUDA0`, `Vulkan1`). A listed device receives no layer of a model and its
/// memory is left out of every estimate. An empty list, the default, leaves
/// the engine every device it sees. The section is read at every load.
///
/// ```toml
/// [llm.devices]
/// disabled = ["CUDA1"]
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LlmDevicesConfig {
    /// Identifiers of the devices held back, as the engine names them.
    #[serde(default)]
    pub disabled: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_figure_leaves_the_window_to_the_machine() {
        // GIVEN the section nobody wrote
        let section = LlmDefaultsConfig::default();

        // WHEN its default is read
        let default = section.context_default();

        // THEN the engine sizes the window, and no figure is imposed
        assert_eq!(default, ContextDefault::Machine);
    }

    #[test]
    fn test_a_figure_and_the_model_maximum_are_told_apart() {
        // GIVEN a custom figure, the model maximum, and a misspelt mode
        // WHEN each is read
        let figure = ContextDefault::of("custom", 16_384);
        let maximum = ContextDefault::of("model_max", 16_384);
        let typo = ContextDefault::of("model-max", 8_192);

        // THEN the figure is kept, the maximum ignores it, and the typo reads
        //      as a custom figure rather than as the largest window
        assert_eq!(figure, ContextDefault::Tokens(16_384));
        assert_eq!(maximum, ContextDefault::ModelMax);
        assert_eq!(typo, ContextDefault::Tokens(8_192));
    }

    #[test]
    fn test_images_are_held_to_2048_pixels_by_default() {
        // GIVEN the section nobody wrote
        // WHEN its defaults are read
        let images = LlmImagesConfig::default();

        // THEN the limit is on, at 2048 pixels
        assert!(images.limit_side);
        assert_eq!(images.max_side_px, 2048);
    }
}
