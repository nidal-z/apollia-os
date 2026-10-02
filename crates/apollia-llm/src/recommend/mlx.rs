//! What an MLX model costs to run, sized the way [`super::fit`] sizes a GGUF.
//!
//! An MLX model is a folder: a `config.json` that states the architecture and
//! one or more `*.safetensors` files that hold the weights. There is no header
//! to probe, so the two terms come from those two sources:
//!
//! - the weights are the summed size of the `*.safetensors` files;
//! - the key/value cache is computed from `config.json` for the context the
//!   model will run at:
//!
//! ```text
//! bytes = 2 * num_hidden_layers * num_key_value_heads * head_dim
//!         * n_ctx * n_parallel * bytes_per_element
//! ```
//!
//! The overhead is the one the GGUF estimate charges an accelerated model,
//! since MLX only runs on the accelerator of an Apple Silicon machine.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::fit::{overhead_gb, MemoryEstimate, RuntimeShape};

const BYTES_PER_GIB: f64 = 1024.0 * 1024.0 * 1024.0;

/// Why an MLX folder could not be sized.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum MlxEstimateError {
    /// `config.json` is missing or unreadable.
    #[error("cannot read {path}")]
    Read {
        /// The file that was read.
        path: PathBuf,
        /// The underlying error.
        #[source]
        source: std::io::Error,
    },
    /// `config.json` is not the JSON an MLX model ships.
    #[error("invalid config at {path}")]
    Parse {
        /// The file that was parsed.
        path: PathBuf,
        /// The underlying error.
        #[source]
        source: serde_json::Error,
    },
    /// `config.json` lacks a field the cache formula needs.
    #[error("config at {path} does not state {field}")]
    MissingField {
        /// The file that was parsed.
        path: PathBuf,
        /// The field that is absent.
        field: &'static str,
    },
}

/// The fields of an MLX `config.json` the cache depends on.
///
/// A multimodal model nests them under `text_config`; [`MlxConfig::from_json`]
/// reads either place.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
pub struct MlxConfig {
    /// Transformer layers.
    #[serde(default)]
    pub num_hidden_layers: Option<u64>,
    /// Attention heads.
    #[serde(default)]
    pub num_attention_heads: Option<u64>,
    /// Key/value heads; equal to the attention heads when absent (no grouping).
    #[serde(default)]
    pub num_key_value_heads: Option<u64>,
    /// Dimension of one head; `hidden_size / num_attention_heads` when absent.
    #[serde(default)]
    pub head_dim: Option<u64>,
    /// Model width.
    #[serde(default)]
    pub hidden_size: Option<u64>,
}

#[derive(Deserialize)]
struct ConfigFile {
    #[serde(flatten)]
    top: MlxConfig,
    #[serde(default)]
    text_config: Option<MlxConfig>,
}

impl MlxConfig {
    /// Parse a `config.json`, preferring the text model's fields when the file
    /// nests them.
    ///
    /// # Errors
    /// Returns the parser's error when the text is not a JSON object.
    pub fn from_json(text: &str) -> Result<Self, serde_json::Error> {
        let file: ConfigFile = serde_json::from_str(text)?;
        Ok(match file.text_config {
            Some(inner) if inner.num_hidden_layers.is_some() => inner,
            _ => file.top,
        })
    }

    fn layers(&self) -> Result<u64, &'static str> {
        self.num_hidden_layers.ok_or("num_hidden_layers")
    }

    fn kv_heads(&self) -> Result<u64, &'static str> {
        self.num_key_value_heads
            .or(self.num_attention_heads)
            .ok_or("num_key_value_heads")
    }

    fn head_dim(&self) -> Result<u64, &'static str> {
        if let Some(dim) = self.head_dim {
            return Ok(dim);
        }
        match (self.hidden_size, self.num_attention_heads) {
            (Some(hidden), Some(heads)) if heads > 0 => Ok(hidden / heads),
            _ => Err("head_dim"),
        }
    }
}

/// Key/value cache bytes for `config` at `shape`.
///
/// # Errors
/// Returns the name of the first field the formula needs and the config lacks.
pub fn mlx_kv_cache_bytes(config: &MlxConfig, shape: &RuntimeShape) -> Result<u64, &'static str> {
    let per_token = 2
        * config.layers()?
        * config.kv_heads()?
        * config.head_dim()?
        * u64::from(shape.kv_element_bytes);
    Ok(per_token * u64::from(shape.n_ctx) * u64::from(shape.n_parallel.max(1)))
}

/// Summed size of the `*.safetensors` files directly inside `dir`.
fn weights_bytes(dir: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("safetensors"))
        .filter_map(|p| std::fs::metadata(&p).ok())
        .map(|m| m.len())
        .sum()
}

/// Estimate the runtime footprint of the MLX model in `dir`.
///
/// # Errors
/// Fails when `config.json` cannot be read, parsed, or lacks a field the cache
/// formula needs.
pub fn estimate_mlx_memory(
    dir: &Path,
    shape: &RuntimeShape,
) -> Result<MemoryEstimate, MlxEstimateError> {
    let path = dir.join("config.json");
    let text = std::fs::read_to_string(&path).map_err(|source| MlxEstimateError::Read {
        path: path.clone(),
        source,
    })?;
    let config = MlxConfig::from_json(&text).map_err(|source| MlxEstimateError::Parse {
        path: path.clone(),
        source,
    })?;
    let kv_bytes = mlx_kv_cache_bytes(&config, shape)
        .map_err(|field| MlxEstimateError::MissingField { path, field })?;

    let weights_gb = weights_bytes(dir) as f64 / BYTES_PER_GIB;
    let kv_cache_gb = kv_bytes as f64 / BYTES_PER_GIB;
    let overhead_gb = overhead_gb(weights_gb, true);
    Ok(MemoryEstimate {
        weights_gb,
        kv_cache_gb,
        overhead_gb,
        total_gb: weights_gb + kv_cache_gb + overhead_gb,
        kv_cache_measured: true,
        layout: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `config.json` of Qwen2.5 7B Instruct, as published, trimmed to the
    /// fields that matter and a few that do not.
    const QWEN25_7B: &str = r#"{
        "architectures": ["Qwen2ForCausalLM"],
        "hidden_size": 3584,
        "intermediate_size": 18944,
        "num_attention_heads": 28,
        "num_hidden_layers": 28,
        "num_key_value_heads": 4,
        "rope_theta": 1000000.0,
        "torch_dtype": "bfloat16",
        "quantization": {"group_size": 64, "bits": 4}
    }"#;

    #[test]
    fn the_cache_of_a_reference_config_is_the_published_arithmetic() {
        // GIVEN the Qwen2.5 7B config and the runtime's default 32k f16 window
        let config = MlxConfig::from_json(QWEN25_7B).expect("reference config parses");
        let shape = RuntimeShape::default();

        // WHEN the key/value cache is sized
        let bytes = mlx_kv_cache_bytes(&config, &shape).expect("every field is present");

        // THEN it is 2 x 28 layers x 4 heads x 128 dims x 32768 tokens x 2
        // bytes, 1 879 048 192 bytes (1.75 GiB)
        assert_eq!(bytes, 1_879_048_192);
    }

    #[test]
    fn a_nested_text_config_and_a_declared_head_dim_are_read() {
        // GIVEN a multimodal config whose text model declares its head width
        let text = r#"{
            "model_type": "gemma3",
            "text_config": {
                "hidden_size": 2560,
                "num_attention_heads": 8,
                "num_hidden_layers": 34,
                "num_key_value_heads": 4,
                "head_dim": 256
            }
        }"#;

        // WHEN it is parsed and sized at 1024 tokens of f16
        let config = MlxConfig::from_json(text).expect("config parses");
        let shape = RuntimeShape {
            n_ctx: 1024,
            ..RuntimeShape::default()
        };

        // THEN the declared 256 wins over 2560 / 8 = 320
        assert_eq!(
            mlx_kv_cache_bytes(&config, &shape),
            Ok(2 * 34 * 4 * 256 * 1024 * 2)
        );
    }

    #[test]
    fn a_folder_is_sized_from_its_weights_and_its_config() {
        // GIVEN an MLX folder with the reference config and two weight shards
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("config.json"), QWEN25_7B).expect("config");
        for name in [
            "model-00001-of-00002.safetensors",
            "model-00002-of-00002.safetensors",
        ] {
            let f = std::fs::File::create(dir.path().join(name)).expect("shard");
            f.set_len(1024 * 1024 * 1024).expect("size");
        }
        std::fs::write(dir.path().join("README.md"), "not weights").expect("readme");

        // WHEN the folder is estimated at the default window
        let est = estimate_mlx_memory(dir.path(), &RuntimeShape::default()).expect("estimate");

        // THEN weights are the two shards, the cache is the formula's 1.75 GiB,
        // and the overhead is the accelerated one
        assert!((est.weights_gb - 2.0).abs() < 1e-9);
        assert!((est.kv_cache_gb - 1.75).abs() < 1e-9);
        assert!((est.overhead_gb - 0.75).abs() < 1e-9);
        assert!((est.total_gb - 4.5).abs() < 1e-9);
        assert!(est.kv_cache_measured);
    }

    #[test]
    fn a_config_without_layers_says_which_field_is_missing() {
        // GIVEN a folder whose config omits the layer count
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("config.json"), r#"{"hidden_size": 1024}"#).expect("config");

        // WHEN it is estimated
        let err =
            estimate_mlx_memory(dir.path(), &RuntimeShape::default()).expect_err("no layer count");

        // THEN the error names the field
        assert!(matches!(
            err,
            MlxEstimateError::MissingField {
                field: "num_hidden_layers",
                ..
            }
        ));
    }
}
