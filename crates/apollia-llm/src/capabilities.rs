//! What a model can do: call tools, reason before answering, read images.
//!
//! Computed here, beside the registry and the index, so every surface reads
//! the same answer. The three facts come from what the model ships with:
//!
//! - **tools** and **reasoning** from its chat template, the Jinja text the
//!   engine renders every conversation through. A template that never mentions
//!   tools cannot carry a tool call to the model, and a template that opens a
//!   reasoning block is one whose model was trained to use it;
//! - **vision** from what the engine needs to see an image: a projector
//!   (`mmproj`) beside a GGUF model, or a `vision_config` in the `config.json`
//!   of an MLX model.
//!
//! For an installed model, vision is true only when the engine that serves the
//! format can actually decode an image for it, and the capability then names
//! the image formats that engine reads.

use serde::{Deserialize, Serialize};

mod reasoning;

pub use reasoning::{
    family_profile, mlx_config_model_type, reasoning_profile, ReasoningProfile,
    TemplateReasoningVariables, FAMILY_PROFILES, REASONING_EFFORT_VARIABLE, THINKING_VARIABLE,
};

/// Image formats `llama-server` decodes, through `stb_image`.
///
/// Source: `tools/server/README.md` of llama.cpp at the tag the runtime ships
/// ([`crate::recommend::LLAMA_CPP_TAG`]), section on multimodal input: "Accepts
/// formats supported by `stb_image` (jpeg, png, tga, bmp, gif, ...)".
pub const LLAMA_SERVER_IMAGE_FORMATS: &[&str] = &["jpeg", "png", "tga", "bmp", "gif"];

/// Image formats the MLX engine decodes.
///
/// `vllm-mlx` writes an incoming image to a file and hands it to `mlx-vlm`,
/// whose `load_image` opens it with Pillow (`Image.open`), which reads the
/// format from the bytes rather than the name. These are the formats the
/// Pillow wheels pinned in `packaging/requirements-mlx-engine.txt` decode.
pub const VLLM_MLX_IMAGE_FORMATS: &[&str] = &["jpeg", "png", "gif", "bmp", "webp", "tiff"];

/// The engine that serves a model, as far as reading images goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisionEngine {
    /// The embedded `llama-server`, for GGUF models.
    LlamaServer,
    /// The MLX engine, for MLX models.
    VllmMlx,
}

impl VisionEngine {
    /// The image formats this engine decodes.
    #[must_use]
    pub fn image_formats(self) -> &'static [&'static str] {
        match self {
            Self::LlamaServer => LLAMA_SERVER_IMAGE_FORMATS,
            Self::VllmMlx => VLLM_MLX_IMAGE_FORMATS,
        }
    }
}

/// What a model can do.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    /// The chat template carries tool definitions and tool calls.
    pub tools: bool,
    /// The chat template opens a reasoning block.
    pub reasoning: bool,
    /// The reasoning setting the chat template lets a caller fix, see
    /// [`ReasoningProfile`]. `None` when it is not known: an entry of the
    /// catalogue, or an index entry written before this field existed.
    #[serde(default)]
    pub reasoning_profile: Option<ReasoningProfile>,
    /// The model can read images.
    pub vision: bool,
    /// Image formats the serving engine decodes; empty when `vision` is false,
    /// and on a catalogue entry, where the engine is not decided yet.
    #[serde(default)]
    pub image_formats: Vec<String>,
}

impl Capabilities {
    /// The tool and reasoning capabilities a chat template declares, without
    /// vision. `None` (no template) declares neither, and no reasoning setting.
    #[must_use]
    pub fn from_chat_template(template: Option<&str>) -> Self {
        Self::from_chat_template_of_family(template, None)
    }

    /// [`Self::from_chat_template`] for a model of `family` (an architecture
    /// or an MLX `model_type`), which settles the reasoning profile when the
    /// template does not, through [`FAMILY_PROFILES`].
    #[must_use]
    pub fn from_chat_template_of_family(template: Option<&str>, family: Option<&str>) -> Self {
        let profile = reasoning_profile(template.map(TemplateReasoningVariables::of), family);
        let template = template.unwrap_or_default();
        Self {
            tools: template_handles_tools(template),
            reasoning: template_handles_reasoning(template),
            reasoning_profile: Some(profile),
            vision: false,
            image_formats: Vec::new(),
        }
    }

    /// The tool and reasoning capabilities a GGUF header declares through its
    /// chat template, without vision.
    #[must_use]
    pub fn from_gguf_header(facts: &crate::gguf_probe::GgufHeaderFacts) -> Self {
        let variables = facts
            .has_chat_template
            .then_some(facts.chat_template_reasoning_variables);
        Self {
            tools: facts.chat_template_tools,
            reasoning: facts.chat_template_reasoning,
            reasoning_profile: Some(reasoning_profile(variables, facts.architecture.as_deref())),
            vision: false,
            image_formats: Vec::new(),
        }
    }

    /// Whether this record was computed by a build that reads the reasoning
    /// profile. An older record is read again from the model's files.
    #[must_use]
    pub(crate) fn is_current(&self) -> bool {
        self.reasoning_profile.is_some()
    }

    /// Mark the model as reading images, with the formats `engine` decodes.
    #[must_use]
    pub fn with_vision(mut self, engine: VisionEngine) -> Self {
        self.vision = true;
        self.image_formats = engine
            .image_formats()
            .iter()
            .map(|f| (*f).to_owned())
            .collect();
        self
    }

    /// Mark the model as reading images once downloaded, without naming the
    /// formats: the catalogue does not know yet which engine will serve it.
    #[must_use]
    pub fn with_catalogue_vision(mut self, vision: bool) -> Self {
        self.vision = vision;
        self.image_formats = Vec::new();
        self
    }
}

/// Template fragments that only a template rendering tool definitions or tool
/// calls contains: the `tools` variable an OpenAI-style request fills, the
/// `tool_calls` of an assistant message, and the markers of the main families.
const TOOL_MARKERS: &[&str] = &[
    "tools",
    "tool_call",
    "[tool_calls]",
    "[available_tools]",
    "<|python_tag|>",
];

/// Template fragments that open or toggle a reasoning block: Qwen's `<think>`
/// and its `enable_thinking` switch, the `reasoning_content` field templates
/// re-inline, Mistral's `[THINK]`, Gemma 4's thought channel, and the
/// `reasoning_effort` of gpt-oss.
const REASONING_MARKERS: &[&str] = &[
    "<think>",
    "</think>",
    "enable_thinking",
    "reasoning_content",
    "[think]",
    "<|channel>",
    "reasoning_effort",
];

/// Whether a chat template renders tools.
#[must_use]
pub fn template_handles_tools(template: &str) -> bool {
    let lower = template.to_ascii_lowercase();
    TOOL_MARKERS.iter().any(|m| lower.contains(m))
}

/// Whether a chat template opens a reasoning block.
#[must_use]
pub fn template_handles_reasoning(template: &str) -> bool {
    let lower = template.to_ascii_lowercase();
    REASONING_MARKERS.iter().any(|m| lower.contains(m))
}

/// Whether the text of an MLX `config.json` declares a vision tower.
///
/// A multimodal MLX conversion keeps the `vision_config` block of its upstream
/// configuration; a text-only one has none. Unreadable JSON declares nothing.
#[must_use]
pub fn mlx_config_has_vision(config_json: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(config_json)
        .ok()
        .and_then(|v| v.get("vision_config").map(|c| !c.is_null()))
        .unwrap_or(false)
}

/// The chat template an MLX folder ships: `chat_template.jinja`, then the
/// `chat_template` of `chat_template.json`, then that of
/// `tokenizer_config.json`. `None` when none of them carries one.
#[must_use]
pub fn mlx_chat_template(dir: &std::path::Path) -> Option<String> {
    if let Ok(text) = std::fs::read_to_string(dir.join("chat_template.jinja")) {
        return Some(text);
    }
    ["chat_template.json", "tokenizer_config.json"]
        .iter()
        .find_map(|name| {
            let text = std::fs::read_to_string(dir.join(name)).ok()?;
            chat_template_of_json(&text)
        })
}

/// The `chat_template` field of a JSON document, as a string, or the template
/// named `default` when the field is a list of named templates.
#[must_use]
pub fn chat_template_of_json(text: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    template_of_value(value.get("chat_template")?)
}

pub(crate) fn template_of_value(value: &serde_json::Value) -> Option<String> {
    if let Some(s) = value.as_str() {
        return Some(s.to_owned());
    }
    value.as_array()?.iter().find_map(|named| {
        (named.get("name")?.as_str()? == "default")
            .then(|| named.get("template")?.as_str().map(str::to_owned))
            .flatten()
    })
}

#[cfg(test)]
mod tests;
