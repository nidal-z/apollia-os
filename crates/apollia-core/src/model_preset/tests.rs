use super::*;
use crate::paths::DataFile;
use crate::system_db::{migrate_v1, migrate_v2, migrate_v3, migrate_v4};
use crate::InferenceServer;
use rusqlite::Connection;

fn db_path(dir: &tempfile::TempDir) -> std::path::PathBuf {
    dir.path().join(DataFile::System.file_name())
}

fn open_repo(dir: &tempfile::TempDir) -> LlmBackendRepository {
    LlmBackendRepository::open(&db_path(dir)).unwrap()
}

fn is_constraint(result: &Result<usize, rusqlite::Error>) -> bool {
    matches!(result, Err(rusqlite::Error::SqliteFailure(e, _))
        if e.code == rusqlite::ErrorCode::ConstraintViolation)
}

fn draft(name: &str, is_model_default: bool) -> PresetDraft {
    PresetDraft {
        name: name.into(),
        load: serde_json::Map::new(),
        request: RequestSettings::default(),
        is_model_default,
    }
}

fn local_backend(name: &str) -> LlmBackendConfig {
    LlmBackendConfig {
        name: name.into(),
        provider: LlmProvider::LlamaCpp,
        model: format!("/models/{name}.gguf"),
        config_json: serde_json::json!({}),
        enabled: true,
        is_default: false,
    }
}

fn declare_server(repo: &LlmBackendRepository, name: &str) {
    repo.save_server(&InferenceServer {
        name: name.into(),
        base_url: "http://10.0.0.5:8000/v1".into(),
        has_key: false,
        api_key_env: None,
        enabled: true,
    })
    .unwrap();
}

fn defaults(conn: &Connection) -> Vec<String> {
    let mut stmt = conn
        .prepare("SELECT name FROM llm_backends WHERE is_default = 1 ORDER BY name")
        .unwrap();
    stmt.query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

// GIVEN a v3 database holding two backends both flagged default
// WHEN it is opened at the current version, then the v4 step replayed
// THEN the two tables exist and are empty, the backends are kept, the first
//      default by name alone keeps its flag, the version reads 4, and the
//      replay changes nothing
#[test]
fn test_v4_creates_the_tables_and_keeps_one_default() {
    // GIVEN
    let dir = tempfile::TempDir::new().unwrap();
    {
        let conn = Connection::open(db_path(&dir)).unwrap();
        migrate_v1(&conn).unwrap();
        migrate_v2(&conn).unwrap();
        migrate_v3(&conn).unwrap();
        conn.pragma_update(None, "user_version", 3).unwrap();
        conn.execute_batch(
            "INSERT INTO llm_backends (name, provider, model, is_default)
                 VALUES ('b-local', 'llama-cpp', '/m/b.gguf', 1),
                        ('a-local', 'llama-cpp', '/m/a.gguf', 1);",
        )
        .unwrap();
    }

    // WHEN
    drop(open_repo(&dir));
    let conn = Connection::open(db_path(&dir)).unwrap();
    migrate_v4(&conn).unwrap();

    // THEN
    let count = |table: &str| -> i64 {
        conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    };
    assert_eq!(count("model_presets"), 0);
    assert_eq!(count("agent_model"), 0);
    assert_eq!(count("llm_backends"), 2);
    assert_eq!(defaults(&conn), vec!["a-local".to_string()]);
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        version,
        i64::from(crate::system_db::SYSTEM_DB_SCHEMA_VERSION)
    );
}

// GIVEN a model that already has a default preset, written straight to the base
// WHEN a second default preset for the same model is inserted, then a preset
//      whose settings carry a system prompt
// THEN both inserts are refused by the base itself
#[test]
fn test_base_refuses_a_second_default_and_a_system_prompt() {
    // GIVEN
    let dir = tempfile::TempDir::new().unwrap();
    drop(open_repo(&dir));
    let conn = Connection::open(db_path(&dir)).unwrap();
    conn.execute(
        "INSERT INTO model_presets (name, model_source, model_id, is_model_default)
         VALUES ('fast', 'local', 'qwen3-8b', 1)",
        [],
    )
    .unwrap();

    // WHEN
    let second_default = conn.execute(
        "INSERT INTO model_presets (name, model_source, model_id, is_model_default)
         VALUES ('precise', 'local', 'qwen3-8b', 1)",
        [],
    );
    let other_model = conn.execute(
        "INSERT INTO model_presets (name, model_source, model_id, is_model_default)
         VALUES ('fast', 'server', 'vllm/qwen3-8b', 1)",
        [],
    );
    let in_request = conn.execute(
        "INSERT INTO model_presets (name, model_source, model_id, request_json)
         VALUES ('chatty', 'local', 'qwen3-8b', '{\"system_prompt\":\"Be nice\"}')",
        [],
    );
    let in_load = conn.execute(
        "INSERT INTO model_presets (name, model_source, model_id, load_json)
         VALUES ('chatty', 'local', 'qwen3-8b', '{\"system\":\"Be nice\"}')",
        [],
    );

    // THEN
    assert!(is_constraint(&second_default), "{second_default:?}");
    assert_eq!(other_model.unwrap(), 1, "another model has its own default");
    assert!(is_constraint(&in_request), "{in_request:?}");
    assert!(is_constraint(&in_load), "{in_load:?}");
}

// GIVEN request settings written with a system prompt, and load settings
//       holding one
// WHEN they are read as request settings, and a preset is created with them
// THEN both are refused before reaching the base
#[test]
fn test_a_preset_never_takes_a_system_prompt() {
    // GIVEN
    let dir = tempfile::TempDir::new().unwrap();
    let repo = open_repo(&dir);
    let request = r#"{"temperature":0.2,"system_prompt":"Be nice"}"#;
    let mut with_prompt = draft("chatty", false);
    with_prompt
        .load
        .insert("system_prompt".into(), serde_json::json!("Be nice"));

    // WHEN
    let parsed = serde_json::from_str::<RequestSettings>(request);
    let created = repo.create_preset(ModelSource::Local, "qwen3-8b", &with_prompt);

    // THEN
    assert!(parsed.is_err());
    assert!(matches!(created, Err(LlmBackendError::InvalidPreset(_))));
    assert!(repo.list_presets(None).unwrap().is_empty());
}

// GIVEN a model with a default preset
// WHEN a second preset is created as default, then renamed, then a third is
//      created under a name the model already has
// THEN the default moves to the second, the rename is kept with its
//      settings, and the duplicate name is a conflict
#[test]
fn test_presets_keep_one_default_per_model() {
    // GIVEN
    let dir = tempfile::TempDir::new().unwrap();
    let repo = open_repo(&dir);
    let first = repo
        .create_preset(ModelSource::Local, "qwen3-8b", &draft("fast", true))
        .unwrap();

    // WHEN
    let mut precise = draft("precise", true);
    precise.request.temperature = Some(0.1);
    precise
        .load
        .insert("context_length".into(), serde_json::json!(16384));
    let second = repo
        .create_preset(ModelSource::Local, "qwen3-8b", &precise)
        .unwrap();
    precise.name = "very precise".into();
    let renamed = repo.update_preset(second.id, &precise).unwrap();
    let duplicate = repo.create_preset(ModelSource::Local, "qwen3-8b", &draft("fast", false));

    // THEN
    let presets = repo
        .list_presets(Some((ModelSource::Local, "qwen3-8b")))
        .unwrap();
    let flags: Vec<(String, bool)> = presets
        .iter()
        .map(|p| (p.name.clone(), p.is_model_default))
        .collect();
    assert_eq!(
        flags,
        vec![("fast".into(), false), ("very precise".into(), true)]
    );
    assert_eq!(renamed.request.temperature, Some(0.1));
    assert_eq!(renamed.load["context_length"], 16384);
    assert!(
        !repo
            .find_preset(first.id)
            .unwrap()
            .unwrap()
            .is_model_default
    );
    assert!(matches!(duplicate, Err(LlmBackendError::PresetConflict(_))));
}

// GIVEN an agent assigned a server model with a preset of that model
// WHEN the assignment is read, a preset of another model is offered, and the
//      chosen preset is deleted
// THEN the assignment reads back whole, the foreign preset is refused, and
//      the deletion leaves the agent on its model with no preset
#[test]
fn test_agent_model_holds_the_model_and_a_preset_of_it() {
    // GIVEN
    let dir = tempfile::TempDir::new().unwrap();
    let repo = open_repo(&dir);
    declare_server(&repo, "vllm");
    let preset = repo
        .create_preset(ModelSource::Server, "vllm/qwen3-32b", &draft("long", false))
        .unwrap();
    let other = repo
        .create_preset(ModelSource::Local, "qwen3-8b", &draft("fast", false))
        .unwrap();
    let assignment = AgentModel {
        agent: "mail-triage".into(),
        model_source: ModelSource::Server,
        model_id: "vllm/qwen3-32b".into(),
        preset_id: Some(preset.id),
    };
    repo.set_agent_model(&assignment).unwrap();

    // WHEN
    let read = repo.agent_model("mail-triage").unwrap();
    let foreign = repo.set_agent_model(&AgentModel {
        preset_id: Some(other.id),
        ..assignment.clone()
    });
    repo.delete_preset(preset.id).unwrap();

    // THEN
    assert_eq!(read, Some(assignment.clone()));
    assert!(matches!(foreign, Err(LlmBackendError::InvalidPreset(_))));
    assert_eq!(
        repo.agent_model("mail-triage").unwrap(),
        Some(AgentModel {
            preset_id: None,
            ..assignment
        })
    );
}

// GIVEN no declared server named `ghost`
// WHEN an agent is assigned `ghost/model`, then a local model, then cleared
// THEN the server model is refused, the local one kept, and the clear removes it
#[test]
fn test_agent_model_refuses_an_undeclared_server() {
    // GIVEN
    let dir = tempfile::TempDir::new().unwrap();
    let repo = open_repo(&dir);
    let local = AgentModel {
        agent: "notes".into(),
        model_source: ModelSource::Local,
        model_id: "qwen3-8b".into(),
        preset_id: None,
    };

    // WHEN
    let ghost = repo.set_agent_model(&AgentModel {
        model_source: ModelSource::Server,
        model_id: "ghost/model".into(),
        ..local.clone()
    });
    repo.set_agent_model(&local).unwrap();
    let kept = repo.agent_model("notes").unwrap();
    let cleared = repo.clear_agent_model("notes").unwrap();

    // THEN
    assert!(matches!(ghost, Err(LlmBackendError::InvalidPreset(_))));
    assert_eq!(kept, Some(local));
    assert!(cleared);
    assert_eq!(repo.agent_model("notes").unwrap(), None);
}

// GIVEN two local engine rows and a declared server
// WHEN the default is set to a local model, then to a model of the server,
//      then to that same server model again
// THEN each designation reads back, the server model got one row naming its
//      server and reused it the second time, and one row alone is the default
#[test]
fn test_default_model_designates_a_local_or_a_server_model() {
    // GIVEN
    let dir = tempfile::TempDir::new().unwrap();
    let repo = open_repo(&dir);
    repo.save(&local_backend("qwen-local")).unwrap();
    repo.save(&local_backend("gemma-local")).unwrap();
    declare_server(&repo, "vllm");

    // WHEN
    let local = repo
        .set_default_model(ModelSource::Local, "gemma-local")
        .unwrap();
    let server = repo
        .set_default_model(ModelSource::Server, "vllm/Qwen/Qwen3-32B")
        .unwrap();
    let again = repo
        .set_default_model(ModelSource::Server, "vllm/Qwen/Qwen3-32B")
        .unwrap();

    // THEN
    assert_eq!(
        local,
        DefaultModel {
            backend: "gemma-local".into(),
            model_source: ModelSource::Local,
            model_id: "gemma-local".into(),
        }
    );
    assert_eq!(server.model_source, ModelSource::Server);
    assert_eq!(server.model_id, "vllm/Qwen/Qwen3-32B");
    assert_eq!(server.backend, "vllm-qwen-qwen3-32b");
    assert_eq!(again, server);
    let row = repo.find_by_name(&server.backend).unwrap().unwrap();
    assert_eq!(row.provider, LlmProvider::OpenAi);
    assert_eq!(row.model, "Qwen/Qwen3-32B");
    assert_eq!(row.config_json[SERVER_REF_FIELD], "vllm");
    assert_eq!(row.config_json["base_url"], "http://10.0.0.5:8000/v1");
    assert_eq!(repo.list().unwrap().len(), 3);
    let conn = Connection::open(db_path(&dir)).unwrap();
    assert_eq!(defaults(&conn), vec![server.backend.clone()]);
    let second = conn.execute(
        "UPDATE llm_backends SET is_default = 1 WHERE name = 'qwen-local'",
        [],
    );
    assert!(is_constraint(&second), "{second:?}");
}

// GIVEN a local engine row and no declared server
// WHEN the default is set to the local row as a server model, to an
//      undeclared server, and to an absent backend
// THEN each is refused and the default is unchanged
#[test]
fn test_default_model_refuses_a_wrong_designation() {
    // GIVEN
    let dir = tempfile::TempDir::new().unwrap();
    let repo = open_repo(&dir);
    repo.save(&local_backend("qwen-local")).unwrap();

    // WHEN
    let wrong_kind = repo.set_default_model(ModelSource::Server, "qwen-local");
    let undeclared = repo.set_default_model(ModelSource::Server, "ghost/model");
    let absent = repo.set_default_model(ModelSource::Local, "nowhere");

    // THEN
    assert!(matches!(wrong_kind, Err(LlmBackendError::InvalidPreset(_))));
    assert!(matches!(undeclared, Err(LlmBackendError::InvalidPreset(_))));
    assert!(matches!(absent, Err(LlmBackendError::NotFound(_))));
    assert_eq!(
        repo.default_model().unwrap().map(|d| d.backend),
        Some("qwen-local".to_string())
    );
}
