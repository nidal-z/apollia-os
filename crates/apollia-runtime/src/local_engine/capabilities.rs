//! What the model behind a backend name can do, read from the model index.
//!
//! A conversation names its model the way the router routes it: a backend
//! row, the index identifier of a local model, or nothing for the default of
//! the machine. A local model answers with the capabilities its index entry
//! records, the reasoning profile and vision among them. A model served by a
//! remote server answers no capability: its template is not at hand, so
//! nothing it could offer is known.

use std::path::Path;

use apollia_core::{LlmBackendError, LlmBackendRepository, LlmProvider};
use apollia_llm::model_index::{ModelIndex, INDEX_FILE_NAME};
use apollia_llm::Capabilities;

/// The model a name resolves to, and what it can do.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModelCapabilities {
    /// The backend or model name the conversation runs on, the default row
    /// when it names none. `None` when no model is configured at all.
    pub model: Option<String>,
    /// Whether a local engine serves it.
    pub local: bool,
    /// Its capabilities, `None` when they are not known: a remote model, a
    /// name nothing resolves, or an index entry not read yet.
    pub capabilities: Option<Capabilities>,
}

/// What the model `model` names can do, `None` naming the default model of
/// the machine.
///
/// # Errors
/// [`LlmBackendError`] when the backend table cannot be read.
pub fn model_capabilities(
    repo: &LlmBackendRepository,
    models_dir: &Path,
    model: Option<&str>,
) -> Result<ModelCapabilities, LlmBackendError> {
    let name = match model {
        Some(name) => name.to_owned(),
        None => match repo.find_default()? {
            Some(row) => row.name,
            None => return Ok(ModelCapabilities::default()),
        },
    };
    let index = ModelIndex::load(&models_dir.join(INDEX_FILE_NAME)).ok();
    let indexed = |path: Option<&Path>| {
        let index = index.as_ref()?;
        index
            .get(&name)
            .or_else(|| path.and_then(|p| index.models.iter().find(|m| m.owns(p))))
            .filter(|m| m.is_language_model())
            .map(|m| m.capabilities.clone())
    };
    let (local, capabilities) = match repo.find_by_name(&name)? {
        Some(row) if row.provider == LlmProvider::LlamaCpp => {
            (true, indexed(Some(Path::new(&row.model))).flatten())
        }
        Some(_) => (false, None),
        None => match indexed(None) {
            Some(caps) => (true, caps),
            None => (false, None),
        },
    };
    Ok(ModelCapabilities {
        model: Some(name),
        local,
        capabilities,
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use apollia_core::LlmBackendConfig;
    use apollia_llm::model_index::{IndexedModel, ModelCategory, ModelFormat, ModelOrigin};
    use apollia_llm::{ReasoningProfile, VisionEngine};

    use super::*;

    fn indexed(id: &str, path: PathBuf, caps: Capabilities) -> IndexedModel {
        IndexedModel {
            id: id.to_owned(),
            format: ModelFormat::Gguf,
            path,
            parts: Vec::new(),
            annexes: Vec::new(),
            size_bytes: 0,
            origin: ModelOrigin::Import,
            sha256: None,
            origin_verified: false,
            category: ModelCategory::Language,
            parameters: None,
            modified_at: None,
            pinned_at: None,
            category_override: None,
            source_file_override: None,
            capabilities: Some(caps),
        }
    }

    fn row(name: &str, provider: LlmProvider, model: &str, is_default: bool) -> LlmBackendConfig {
        LlmBackendConfig {
            name: name.to_owned(),
            provider,
            model: model.to_owned(),
            config_json: serde_json::json!({}),
            is_default,
            enabled: true,
        }
    }

    struct Fixture {
        _dir: tempfile::TempDir,
        repo: LlmBackendRepository,
        models: PathBuf,
    }

    /// A machine with a Qwen3 GGUF that reads images, routed by the default
    /// row `local-qwen`, a gpt-oss GGUF known to the index only, and a remote
    /// server row.
    fn machine() -> Fixture {
        let dir = tempfile::tempdir().expect("tempdir");
        let models = apollia_core::config::default_models_dir(dir.path());
        std::fs::create_dir_all(&models).expect("mkdir");
        let qwen = models.join("qwen3.gguf");
        let toggle = Capabilities::from_chat_template(Some("{{ enable_thinking }}"))
            .with_vision(VisionEngine::LlamaServer);
        let levels = Capabilities::from_chat_template(Some("{{ reasoning_effort }}"));
        ModelIndex {
            models: vec![
                indexed("qwen3", qwen.clone(), toggle),
                indexed("gpt-oss", models.join("gpt-oss.gguf"), levels),
            ],
            ..ModelIndex::default()
        }
        .save(&models.join(INDEX_FILE_NAME))
        .expect("save index");
        let repo = LlmBackendRepository::open(&dir.path().join("system.db")).expect("open");
        let qwen_path = qwen.display().to_string();
        repo.save(&row("local-qwen", LlmProvider::LlamaCpp, &qwen_path, true))
            .expect("save local row");
        repo.save(&row("remote", LlmProvider::OpenAi, "gpt-4o", false))
            .expect("save remote row");
        Fixture {
            _dir: dir,
            repo,
            models,
        }
    }

    #[test]
    fn a_local_row_answers_the_capabilities_of_the_model_it_points_at() {
        // GIVEN the machine, and a conversation naming no model
        let m = machine();

        // WHEN the capabilities of the default model are read
        let found = model_capabilities(&m.repo, &m.models, None).expect("read");

        // THEN the default row resolves to the Qwen3 file it points at: a
        // switch, and vision with the formats of its engine
        assert_eq!(found.model.as_deref(), Some("local-qwen"));
        assert!(found.local);
        let caps = found.capabilities.expect("known capabilities");
        assert_eq!(caps.reasoning_profile, Some(ReasoningProfile::Toggle));
        assert!(caps.vision);
        assert!(!caps.image_formats.is_empty());
    }

    #[test]
    fn an_index_identifier_without_a_row_answers_its_entry() {
        // GIVEN the machine, and a conversation naming the gpt-oss model by
        // its index identifier
        let m = machine();

        // WHEN its capabilities are read
        let found = model_capabilities(&m.repo, &m.models, Some("gpt-oss")).expect("read");

        // THEN it is a local level model without vision
        assert!(found.local);
        let caps = found.capabilities.expect("known capabilities");
        assert_eq!(caps.reasoning_profile, Some(ReasoningProfile::Levels));
        assert!(!caps.vision);
    }

    #[test]
    fn a_remote_or_unknown_model_answers_no_capability() {
        // GIVEN the machine
        let m = machine();

        // WHEN a remote row and a name nothing knows are read
        let remote = model_capabilities(&m.repo, &m.models, Some("remote")).expect("read");
        let unknown = model_capabilities(&m.repo, &m.models, Some("srv/model")).expect("read");

        // THEN neither declares a capability, so no reasoning setting is shown
        assert!(!remote.local);
        assert_eq!(remote.capabilities, None);
        assert!(!unknown.local);
        assert_eq!(unknown.capabilities, None);
    }

    #[test]
    fn no_model_configured_answers_nothing() {
        // GIVEN an empty backend table and no index
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = LlmBackendRepository::open(&dir.path().join("system.db")).expect("open");

        // WHEN the default model is read
        let found = model_capabilities(&repo, dir.path(), None).expect("read");

        // THEN there is no model to describe
        assert_eq!(found, ModelCapabilities::default());
    }
}
