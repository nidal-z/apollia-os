use std::path::PathBuf;
use std::time::Duration;

use super::*;

/// A stand-in for `vllm-mlx serve`: answers `/health` on the port it is given,
/// records its arguments when `FAKE_MLX_ARGS` names a file, exits at once when
/// `FAKE_MLX_EXIT` is set, and answers a chat completion with a JSON object,
/// recording the request body when `FAKE_MLX_BODY` names a file.
const FAKE_ENGINE: &str = r#"
import http.server, os, sys
args = sys.argv[1:]
if os.environ.get("FAKE_MLX_ARGS"):
    with open(os.environ["FAKE_MLX_ARGS"], "a") as f:
        f.write(" ".join(args) + "\n")
if os.environ.get("FAKE_MLX_EXIT"):
    sys.exit(3)
host = args[args.index("--host") + 1]
port = int(args[args.index("--port") + 1])
class Health(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        body = b'{"status":"healthy"}'
        self.send_response(200)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)
    def do_POST(self):
        sent = self.rfile.read(int(self.headers["Content-Length"]))
        if os.environ.get("FAKE_MLX_BODY"):
            with open(os.environ["FAKE_MLX_BODY"], "wb") as f:
                f.write(sent)
        body = (b'{"id":"c","object":"chat.completion","created":0,"model":"m",'
                b'"choices":[{"index":0,"finish_reason":"stop","message":'
                b'{"role":"assistant","content":"{\\"answer\\":\\"ok\\"}"}}],'
                b'"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}}')
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)
    def log_message(self, *a):
        pass
http.server.HTTPServer((host, port), Health).serve_forever()
"#;

fn fast_timing() -> Timing {
    Timing {
        health_timeout: Duration::from_secs(20),
        health_poll: Duration::from_millis(50),
        supervision_poll: Duration::from_millis(100),
        min_backoff: Duration::from_millis(100),
        max_backoff: Duration::from_millis(400),
    }
}

/// A supervisor whose engine is [`FAKE_ENGINE`], with `env` on the child.
fn fake_supervisor(env: &[(&str, String)]) -> Arc<MlxEngineSupervisor> {
    let launch = MlxLaunch {
        program: PathBuf::from("python3"),
        prefix_args: vec!["-c".to_owned(), FAKE_ENGINE.to_owned()],
        env: env
            .iter()
            .map(|(k, v)| ((*k).to_owned(), v.clone()))
            .collect(),
    };
    Arc::new(MlxEngineSupervisor::with_timing(launch, fast_timing()))
}

/// The process id serving `model_path`, if one is resident.
async fn pid_of(sup: &MlxEngineSupervisor, model_path: &str) -> Option<u32> {
    sup.instances
        .lock()
        .await
        .iter()
        .find(|i| i.model_path == model_path)
        .and_then(|i| i.child.id())
}

#[tokio::test]
async fn a_model_is_served_on_loopback_once_the_engine_answers_health() {
    // GIVEN the MLX engine replaced by a stand-in process that records its
    // arguments
    let tmp = tempfile::tempdir().expect("tempdir");
    let record = tmp.path().join("args.txt");
    let sup = fake_supervisor(&[("FAKE_MLX_ARGS", record.display().to_string())]);

    // WHEN a model folder is asked for, twice
    let first = sup
        .ensure_model(
            "/models/qwen-mlx".to_owned(),
            None,
            LoadConsent::Unconfirmed,
        )
        .await
        .expect("the stand-in becomes healthy");
    let second = sup
        .ensure_model(
            "/models/qwen-mlx".to_owned(),
            None,
            LoadConsent::Unconfirmed,
        )
        .await
        .expect("still served");

    // THEN the process listens on 127.0.0.1 and a free port, answers there,
    // and the second request reuses it rather than launching another
    let base = first.base_url().to_owned();
    assert!(base.starts_with("http://127.0.0.1:"), "{base}");
    assert!(base.ends_with("/v1"), "{base}");
    assert_eq!(second.base_url(), base);
    let launches = std::fs::read_to_string(&record).expect("args recorded");
    assert_eq!(launches.lines().count(), 1, "{launches}");
    assert!(launches.starts_with("/models/qwen-mlx --host 127.0.0.1 --port "));
    let health = reqwest::get(base.replace("/v1", "/health"))
        .await
        .expect("health answers");
    assert!(health.status().is_success());
    sup.shutdown_in_place().await;
}

#[tokio::test]
async fn an_engine_that_exits_during_startup_is_reported_with_its_reason() {
    // GIVEN a stand-in engine that exits before answering
    let sup = fake_supervisor(&[("FAKE_MLX_EXIT", "1".to_owned())]);

    // WHEN a model is asked for
    let err = sup
        .ensure_model(
            "/models/broken-mlx".to_owned(),
            None,
            LoadConsent::Unconfirmed,
        )
        .await
        .expect_err("an engine that exits cannot serve");

    // THEN the error names the engine and says it exited, and nothing is
    // listed as loaded
    let message = err.to_string();
    assert!(message.starts_with("vllm-mlx: "), "{message}");
    assert!(message.contains("exited during startup"), "{message}");
    assert!(sup.loaded_models().await.is_empty());
}

#[tokio::test]
async fn a_crashed_engine_is_respawned_by_the_supervision() {
    // GIVEN a served model and the supervision running
    let sup = fake_supervisor(&[]);
    sup.ensure_model(
        "/models/qwen-mlx".to_owned(),
        None,
        LoadConsent::Unconfirmed,
    )
    .await
    .expect("served");
    Arc::clone(&sup).spawn_supervision();
    let first_pid = pid_of(&sup, "/models/qwen-mlx").await.expect("a pid");

    // WHEN its process dies
    std::process::Command::new("kill")
        .args(["-9", &first_pid.to_string()])
        .status()
        .expect("kill runs");

    // THEN a new process serves the same model shortly after
    let mut respawned = None;
    for _ in 0..100 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if let Some(pid) = pid_of(&sup, "/models/qwen-mlx").await {
            if pid != first_pid && sup.loaded_models().await.len() == 1 {
                respawned = Some(pid);
                break;
            }
        }
    }
    let respawned = respawned.expect("the model is served again");
    assert!(process_exists(respawned));
    sup.shutdown_in_place().await;
}

#[tokio::test]
async fn shutdown_stops_the_engine_and_no_respawn_follows() {
    // GIVEN a served model under supervision
    let sup = fake_supervisor(&[]);
    sup.ensure_model(
        "/models/qwen-mlx".to_owned(),
        None,
        LoadConsent::Unconfirmed,
    )
    .await
    .expect("served");
    Arc::clone(&sup).spawn_supervision();
    let pid = pid_of(&sup, "/models/qwen-mlx").await.expect("a pid");

    // WHEN the supervisor shuts down
    sup.shutdown_in_place().await;
    tokio::time::sleep(Duration::from_millis(400)).await;

    // THEN the process is gone and nothing came back
    assert!(!process_exists(pid), "the engine must be stopped");
    assert!(sup.instances.lock().await.is_empty());
}

#[tokio::test]
async fn the_resident_mlx_models_carry_their_engine_and_unload_like_gguf_ones() {
    // GIVEN one resident MLX model answering a request
    let sup = MlxEngineSupervisor::with_stand_in_models(&["/models/qwen-mlx"]).await;
    let lease = sup
        .touch_live_instance("/models/qwen-mlx")
        .await
        .expect("live");

    // WHEN it is listed, then unloaded while in use and once the answer ends
    let listed = sup.loaded_models().await;
    let refused = sup.unload_model("/models/qwen-mlx").await;
    drop(lease);
    let stopped = sup.unload_model("/models/qwen-mlx").await;

    // THEN the list names the MLX engine and the folder, the unload waits for
    // the answer, then stops the process
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].engine, Engine::VllmMlx);
    assert_eq!(listed[0].model, "qwen-mlx");
    assert_eq!(listed[0].active_requests, 1);
    assert!(matches!(refused, Err(UnloadError::InUse { .. })));
    assert_eq!(stopped, Ok(1));
    assert!(sup.loaded_models().await.is_empty());
}

#[test]
fn the_window_is_capped_at_what_the_folder_was_trained_for() {
    // GIVEN an MLX folder whose config declares a 4096-token training length
    // under `text_config`, as a multimodal conversion does
    let tmp = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        tmp.path().join("config.json"),
        r#"{"text_config":{"max_position_embeddings":4096}}"#,
    )
    .expect("write");
    let path = tmp.path().display().to_string();

    // WHEN the window is chosen for no request and for a small request
    let default = MlxEngineSupervisor::window(&path, None);
    let small = MlxEngineSupervisor::window(&path, Some(2048));

    // THEN the default is capped at the training length, a smaller one kept
    assert_eq!(trained_context(tmp.path()), Some(4096));
    assert_eq!(default, 4096);
    assert_eq!(small, 2048);
    assert_eq!(
        MlxEngineSupervisor::window("/nonexistent", None),
        DEFAULT_N_CTX
    );
}

#[tokio::test]
async fn a_structured_call_on_an_mlx_model_sends_the_schema_as_response_format() {
    // GIVEN an MLX model served by the stand-in engine, which records the body
    // of the request it receives, behind the local backend the router builds
    let tmp = tempfile::tempdir().expect("tempdir");
    let body_file = tmp.path().join("body.json");
    let model_dir = tmp.path().join("qwen-mlx");
    std::fs::create_dir_all(&model_dir).expect("mkdir");
    let sup = fake_supervisor(&[("FAKE_MLX_BODY", body_file.display().to_string())]);
    let backend = crate::local_engine::LocalModelBackend::new(
        Arc::clone(&sup),
        "qwen-mlx".to_owned(),
        "qwen-mlx".to_owned(),
        model_dir.display().to_string(),
        None,
    );
    let schema = serde_json::json!({
        "type": "object",
        "properties": {"answer": {"type": "string"}},
        "required": ["answer"]
    });

    // WHEN `complete` is called with a schema, as `ctx.llm.complete(schema=...)` does
    let response = apollia_llm::types::CompletionModel::complete(
        backend.as_ref(),
        apollia_llm::types::CompletionRequest {
            messages: vec![apollia_llm::types::ChatMessage::user("answer ok")],
            response_schema: Some(schema),
            ..Default::default()
        },
    )
    .await
    .expect("the engine answers");

    // THEN the request carries `response_format` of type `json_schema` with
    // the schema, no llama.cpp grammar, and names the folder the engine serves
    let sent: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&body_file).expect("body recorded")).expect("json");
    assert_eq!(sent["response_format"]["type"], "json_schema");
    assert_eq!(
        sent["response_format"]["json_schema"]["schema"]["required"][0],
        "answer"
    );
    assert!(sent.get("grammar").is_none(), "{sent}");
    assert_eq!(sent["model"], model_dir.display().to_string());
    // AND the JSON answer is handed back as is
    let answer: serde_json::Value = serde_json::from_str(&response.content).expect("json answer");
    assert_eq!(answer["answer"], "ok");
    sup.shutdown_in_place().await;
}

fn process_exists(pid: u32) -> bool {
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

impl MlxEngineSupervisor {
    /// A supervisor whose resident models are stand-in processes, for tests
    /// outside this module that need MLX models in the list.
    pub(crate) async fn with_stand_in_models(models: &[&str]) -> Arc<Self> {
        let sup = Self::with_timing(
            MlxLaunch {
                program: PathBuf::from("/nonexistent/python3"),
                prefix_args: Vec::new(),
                env: Vec::new(),
            },
            fast_timing(),
        );
        for (offset, model) in (0_u16..).zip(models) {
            let child = Command::new("sleep")
                .arg("60")
                .kill_on_drop(true)
                .spawn()
                .expect("spawning sleep must succeed");
            let now = Utc::now();
            let instance = Instance {
                model_path: (*model).to_owned(),
                n_ctx: DEFAULT_N_CTX,
                port: 9101 + offset,
                child,
                last_used: sup.next_tick(),
                loaded_at: now,
                last_used_at: now,
                in_flight: Arc::new(AtomicUsize::new(0)),
            };
            sup.instances.lock().await.push(instance);
        }
        Arc::new(sup)
    }
}

/// An MLX folder whose `config.json` carries what the cache formula needs.
fn mlx_folder(root: &std::path::Path) -> String {
    let dir = root.join("qwen-mlx");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("config.json"),
        r#"{"hidden_size": 2048, "num_attention_heads": 16,
            "num_hidden_layers": 28, "num_key_value_heads": 8,
            "max_position_embeddings": 32768}"#,
    )
    .unwrap();
    std::fs::write(dir.join("model.safetensors"), vec![0u8; 1 << 20]).unwrap();
    dir.display().to_string()
}

#[tokio::test]
async fn an_mlx_model_over_the_guardrail_is_refused_until_the_key_allows_it() {
    // GIVEN a resident MLX model, an MLX folder, and an apollia.toml holding
    // launches to a ceiling of 0.001 GB
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("apollia.toml");
    std::fs::write(
        &config,
        "[llm.guardrails]\nlevel = \"ceiling\"\nceiling_gb = 0.001\n",
    )
    .unwrap();
    let args = dir.path().join("args.log");
    let launch = MlxLaunch {
        program: PathBuf::from("python3"),
        prefix_args: vec!["-c".to_owned(), FAKE_ENGINE.to_owned()],
        env: vec![("FAKE_MLX_ARGS".to_owned(), args.display().to_string())],
    };
    let mut engine = MlxEngineSupervisor::with_timing(launch, fast_timing());
    engine.config_file = Some(config.clone());
    let sup = Arc::new(engine);
    let resident = MlxEngineSupervisor::with_stand_in_models(&["/models/other-mlx"]).await;
    let stand_in = resident.instances.lock().await.pop().unwrap();
    sup.instances.lock().await.push(stand_in);
    let model = mlx_folder(dir.path());

    // WHEN it is asked for, then asked again once the key says `disabled`
    let refused = sup
        .ensure_model(model.clone(), None, LoadConsent::Unconfirmed)
        .await;
    let still_resident = pid_of(&sup, "/models/other-mlx").await.is_some();
    std::fs::write(&config, "[llm.guardrails]\nlevel = \"disabled\"\n").unwrap();
    let admitted = sup
        .ensure_model(model.clone(), None, LoadConsent::Unconfirmed)
        .await;

    // THEN the first is refused before the resident model is stopped, and the
    // second is launched without a restart
    assert!(
        matches!(refused, Err(LocalEngineError::GuardrailRefused(ref r)) if r.budget_gb == 0.001),
        "{refused:?}"
    );
    assert!(still_resident);
    drop(admitted.unwrap());
    assert!(pid_of(&sup, &model).await.is_some());
    sup.shutdown_in_place().await;
}
