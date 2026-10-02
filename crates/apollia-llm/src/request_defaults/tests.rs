//! Request settings resolution tests: the three levels, field by field, and
//! the agent binding that reads them at each call.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use apollia_core::manifest::PackageRequestSettings;
use apollia_core::model_preset::{AgentModel, ModelSource, PresetDraft, RequestSettings};
use apollia_core::LlmBackendRepository;
use futures::Stream;

use super::{model_level, RequestDefaults, SAMPLING_DEFAULTS_FILE_NAME};
use crate::model_defaults::{ModelDefaults, UserOverrides};
use crate::model_index::{ModelIndex, INDEX_FILE_NAME};
use crate::router::LlmRouter;
use crate::types::{
    CompletionModel, CompletionRequest, CompletionResponse, FinishReason, LlmError, StreamChunk,
    TokenUsage,
};
use crate::AgentModelBinding;

/// The Qwen3 (instruct) entry of the embedded table.
const QWEN3_TEMPERATURE: f32 = 0.7;
const QWEN3_TOP_P: f32 = 0.8;
const QWEN3_TOP_K: u32 = 20;

/// A local model file name the embedded table knows by its architecture.
const QWEN3_FILE: &str = "Qwen3-8B-Q4_K_M.gguf";

struct Named(&'static str);

#[async_trait::async_trait]
impl CompletionModel for Named {
    async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        Ok(CompletionResponse {
            engine_timings: None,
            content: self.0.to_string(),
            tool_calls: vec![],
            usage: TokenUsage::default(),
            finish_reason: FinishReason::Stop,
            latency_ms: 1,
            ttft_ms: None,
        })
    }

    async fn stream(
        &self,
        _req: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError> {
        Ok(Box::pin(futures::stream::empty()))
    }

    fn is_available(&self) -> bool {
        true
    }
    fn backend_name(&self) -> &str {
        self.0
    }
    fn model_id(&self) -> &str {
        self.0
    }
}

fn router(names: &[&'static str], default: &str) -> LlmRouter {
    let backends: HashMap<String, Arc<dyn CompletionModel>> = names
        .iter()
        .map(|n| {
            (
                n.to_string(),
                Arc::new(Named(n)) as Arc<dyn CompletionModel>,
            )
        })
        .collect();
    LlmRouter::with_backends(backends, default)
}

/// A GGUF header (version 3, no tensor) carrying the given string pairs.
fn gguf(pairs: &[(&str, &str)]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"GGUF");
    out.extend_from_slice(&3u32.to_le_bytes());
    out.extend_from_slice(&0u64.to_le_bytes());
    out.extend_from_slice(&(pairs.len() as u64).to_le_bytes());
    for (key, value) in pairs {
        out.extend_from_slice(&(key.len() as u64).to_le_bytes());
        out.extend_from_slice(key.as_bytes());
        out.extend_from_slice(&8u32.to_le_bytes());
        out.extend_from_slice(&(value.len() as u64).to_le_bytes());
        out.extend_from_slice(value.as_bytes());
    }
    out
}

/// A data directory holding a Qwen3 GGUF in its models directory, indexed.
/// Returns the path of its `system.db` (not created).
fn data_dir_with_qwen3(root: &Path) -> PathBuf {
    let models = root.join("models");
    std::fs::create_dir_all(&models).unwrap();
    std::fs::write(
        models.join(QWEN3_FILE),
        gguf(&[
            ("general.architecture", "qwen3"),
            ("general.name", "Qwen3 8B"),
        ]),
    )
    .unwrap();
    let mut index = ModelIndex::default();
    index.rebuild(&models).unwrap();
    index.save(&models.join(INDEX_FILE_NAME)).unwrap();
    root.join("system.db")
}

fn preset(db: &Path, model: &str, name: &str, request: RequestSettings, default: bool) -> i64 {
    let repo = LlmBackendRepository::open(db).unwrap();
    repo.create_preset(
        ModelSource::Local,
        model,
        &PresetDraft {
            name: name.into(),
            load: serde_json::Map::new(),
            request,
            is_model_default: default,
        },
    )
    .unwrap()
    .id
}

fn assign(db: &Path, agent: &str, model: &str, preset_id: Option<i64>) {
    LlmBackendRepository::open(db)
        .unwrap()
        .set_agent_model(&AgentModel {
            agent: agent.into(),
            model_source: ModelSource::Local,
            model_id: model.into(),
            preset_id,
        })
        .unwrap();
}

#[test]
fn a_model_the_table_knows_gets_its_temperature_and_top_p_with_nothing_else_set() {
    // GIVEN a Qwen3 GGUF in the models directory, no publisher defaults file,
    //       an agent with no package settings and no preset
    let tmp = tempfile::tempdir().unwrap();
    let db = data_dir_with_qwen3(tmp.path());
    let router = router(&[QWEN3_FILE], QWEN3_FILE);
    let binding = AgentModelBinding::new("notes", None, Some(db));

    // WHEN a call builds its request on that model
    let mut req = CompletionRequest::default();
    binding.request_defaults(&router, None).apply_to(&mut req);

    // THEN the request carries the temperature and top-p of the table
    assert_eq!(req.temperature, Some(QWEN3_TEMPERATURE));
    assert_eq!(req.top_p, Some(QWEN3_TOP_P));
    assert_eq!(req.top_k, Some(QWEN3_TOP_K));
}

#[test]
fn the_publisher_defaults_file_wins_over_the_embedded_table() {
    // GIVEN the downloader recorded a temperature for the Qwen3 file
    let tmp = tempfile::tempdir().unwrap();
    data_dir_with_qwen3(tmp.path());
    let models = tmp.path().join("models");
    UserOverrides::upsert(
        &models.join(SAMPLING_DEFAULTS_FILE_NAME),
        QWEN3_FILE,
        ModelDefaults {
            temperature: Some(0.3),
            ..ModelDefaults::default()
        },
    )
    .unwrap();

    // WHEN level 1 is resolved for that model
    let level = model_level(QWEN3_FILE, QWEN3_FILE, Some(&models));

    // THEN the file's temperature wins, the table fills the rest
    assert_eq!(level.temperature, Some(0.3));
    assert_eq!(level.top_p, Some(QWEN3_TOP_P));
}

#[test]
fn three_levels_merge_field_by_field_preset_over_package_over_model() {
    // GIVEN a model level, a package level and a preset level that each set
    //       some fields and leave others
    let model = RequestDefaults {
        temperature: Some(0.7),
        top_p: Some(0.8),
        top_k: Some(20),
        repeat_penalty: Some(1.0),
        ..RequestDefaults::default()
    };
    let package = RequestDefaults::from_package(&PackageRequestSettings {
        temperature: Some(0.2),
        top_k: Some(40),
        max_tokens: Some(512),
        ..PackageRequestSettings::default()
    });
    let preset = RequestDefaults::from_preset(&RequestSettings {
        temperature: Some(0.9),
        min_p: Some(0.05),
        ..RequestSettings::default()
    });

    // WHEN they are resolved
    let resolved = RequestDefaults::resolve(&model, &package, &preset);

    // THEN each field comes from the highest level that sets it
    assert_eq!(resolved.temperature, Some(0.9), "preset over package");
    assert_eq!(resolved.min_p, Some(0.05), "preset alone");
    assert_eq!(resolved.top_k, Some(40), "package over model");
    assert_eq!(resolved.max_tokens, Some(512), "package alone");
    assert_eq!(resolved.top_p, Some(0.8), "model alone");
    assert_eq!(resolved.repeat_penalty, Some(1.0), "model alone");
    assert_eq!(resolved.seed, None, "no level sets it");
}

#[test]
fn an_agent_call_reads_its_preset_over_its_package_over_its_model() {
    // GIVEN a Qwen3 model assigned to the agent with a preset of it, and a
    //       package that declares its own request settings
    let tmp = tempfile::tempdir().unwrap();
    let db = data_dir_with_qwen3(tmp.path());
    let chosen = preset(
        &db,
        QWEN3_FILE,
        "precise",
        RequestSettings {
            temperature: Some(0.1),
            ..RequestSettings::default()
        },
        false,
    );
    assign(&db, "notes", QWEN3_FILE, Some(chosen));
    let router = router(&["other", QWEN3_FILE], "other");
    let binding = AgentModelBinding::new("notes", None, Some(db)).with_package_request(Some(
        PackageRequestSettings {
            temperature: Some(0.4),
            top_k: Some(64),
            ..PackageRequestSettings::default()
        },
    ));

    // WHEN a call of the agent resolves its model and its request settings
    let backend = binding.resolve(&router, None);
    let resolved = binding.request_defaults(&router, backend.as_deref());

    // THEN the preset sets the temperature, the package the top-k, the model
    //      the top-p
    assert_eq!(backend.as_deref(), Some(QWEN3_FILE));
    assert_eq!(resolved.temperature, Some(0.1));
    assert_eq!(resolved.top_k, Some(64));
    assert_eq!(resolved.top_p, Some(QWEN3_TOP_P));
}

#[test]
fn without_a_chosen_preset_the_default_preset_of_the_assigned_model_applies() {
    // GIVEN an agent assigned a model with no preset, the model having a
    //       default preset
    let tmp = tempfile::tempdir().unwrap();
    let db = data_dir_with_qwen3(tmp.path());
    preset(
        &db,
        QWEN3_FILE,
        "house",
        RequestSettings {
            temperature: Some(0.5),
            ..RequestSettings::default()
        },
        true,
    );
    assign(&db, "notes", QWEN3_FILE, None);
    let router = router(&[QWEN3_FILE], QWEN3_FILE);
    let binding = AgentModelBinding::new("notes", None, Some(db));

    // WHEN the call's request settings are resolved
    let backend = binding.resolve(&router, None);
    let resolved = binding.request_defaults(&router, backend.as_deref());

    // THEN the model's default preset sets the temperature
    assert_eq!(resolved.temperature, Some(0.5));
}

#[test]
fn a_preset_does_not_follow_the_agent_onto_a_fallback_model() {
    // GIVEN an agent assigned a model the router cannot reach, with a preset
    let tmp = tempfile::tempdir().unwrap();
    let db = data_dir_with_qwen3(tmp.path());
    let chosen = preset(
        &db,
        "gone",
        "precise",
        RequestSettings {
            temperature: Some(0.1),
            ..RequestSettings::default()
        },
        false,
    );
    assign(&db, "notes", "gone", Some(chosen));
    let router = router(&[QWEN3_FILE], QWEN3_FILE);
    let binding = AgentModelBinding::new("notes", None, Some(db));

    // WHEN the call falls back to the default model
    let backend = binding.resolve(&router, None);
    let resolved = binding.request_defaults(&router, backend.as_deref());

    // THEN the preset of the unreachable model is not applied; the default
    //      model's own defaults are
    assert_eq!(backend, None);
    assert_eq!(resolved.temperature, Some(QWEN3_TEMPERATURE));
}

#[test]
fn a_value_the_call_sets_is_never_replaced() {
    // GIVEN resolved defaults and a request that sets its own temperature
    let defaults = RequestDefaults {
        temperature: Some(0.7),
        top_p: Some(0.8),
        stop: Some(vec!["END".into()]),
        ..RequestDefaults::default()
    };
    let mut req = CompletionRequest {
        temperature: Some(0.0),
        ..CompletionRequest::default()
    };

    // WHEN the defaults are applied
    defaults.apply_to(&mut req);

    // THEN the call's value stays and the unset fields are filled
    assert_eq!(req.temperature, Some(0.0));
    assert_eq!(req.top_p, Some(0.8));
    assert_eq!(req.stop, Some(vec!["END".to_string()]));
}
