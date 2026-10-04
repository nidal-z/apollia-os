//! Inference at boot: the runner sidecar, the local engines, the LLM router
//! and its call log, then speech to text.

use crate::supervisor::*;

use crate::supervisor::bootstrap::spawn_runner_supervisor;

/// The inference engines and the router built over them.
pub(super) struct Inference {
    pub(super) runner_supervisor: Option<Arc<crate::runner_supervisor::RunnerSupervisor>>,
    pub(super) system_db_path: std::path::PathBuf,
    pub(super) llama_server_supervisor: Option<Arc<crate::llama_server::LlamaServerSupervisor>>,
    pub(super) mlx_engine: Option<Arc<crate::mlx_engine::MlxEngineSupervisor>>,
    pub(super) llm_router: Option<Arc<LlmRouter>>,
    pub(super) llm_backend_repo: Option<Arc<std::sync::Mutex<LlmBackendRepository>>>,
    pub(super) llm_call_repository: Option<Arc<std::sync::Mutex<LlmCallRepository>>>,
}

/// Speech to text: its configuration store and the swappable cells the
/// reload route and the embedded runtime share.
pub(super) struct Stt {
    pub(super) config_repo: Option<Arc<std::sync::Mutex<SttConfigRepository>>>,
    pub(super) engine: crate::api::server::SharedSttEngine,
    pub(super) repository: crate::api::server::SharedSttRepository,
}

impl Supervisor {
    /// The runner sidecar and its supervision, the embedded llama-server, the
    /// MLX engine, then phases 4 and 4b: the LLM router and its call log.
    pub(super) async fn start_inference(
        &self,
        event_sender: &EventBusSender,
        org_policy: &Arc<apollia_permissions::PolicyVerdict>,
    ) -> Inference {
        // Spawn the sidecar runner for the local LLM and STT backends. Cloud
        // backends are still served directly by the daemon via `LlmRouter`.
        //
        // Wrapped in an `Arc` and kept inside `SupervisorHandles` (then
        // `RuntimeHandle`): the supervisor owns the child runner process with
        // `kill_on_drop(true)`. If it were dropped at the end of boot, the
        // runner would be killed and any later inference call would fail with
        // a connection-refused error.
        let runner_supervisor = spawn_runner_supervisor().await.map(Arc::new);

        // Supervise the runner: a fatal condition (e.g. a GGML_ASSERT abort in
        // llama.cpp on an unsupported model architecture) kills the child, after
        // which the cached handle would point at a dead port forever. The
        // supervision task detects the exit and respawns a fresh runner so the
        // system recovers once a supported model is selected.
        if let Some(supervisor) = &runner_supervisor {
            Arc::clone(supervisor).spawn_supervision();
        }

        // Embedded llama-server is the local LLM engine (the runner now serves
        // STT only). Created lazily: it locates the binary but does not launch a
        // process until a model is requested, so a fresh install with no default
        // model still yields a supervisor the router factory can capture. `None`
        // when the binary is absent, leaving local inference unconfigured.
        //
        // The base is the default configuration; the default preset of each
        // model is laid over it at every request, read from `system.db`.
        let system_db_path = self
            .config
            .data_dir
            .join(apollia_core::paths::DataFile::System.file_name());
        // `apollia.toml`, whose `[llm.guardrails]` both engines read at every
        // launch: the file the runtime was started with, else the one in its
        // data directory.
        let guardrails_file = self.config.config_path.clone().unwrap_or_else(|| {
            self.config
                .data_dir
                .join(apollia_core::paths::CONFIG_FILE_NAME)
        });
        let llama_server_supervisor = crate::llama_server::LlamaServerSupervisor::new(
            crate::llama_server::LlamaServerConfig::default(),
            Some(system_db_path.clone()),
            Some(event_sender.clone()),
            Some(guardrails_file.clone()),
            org_policy.imposed_settings(),
        )
        .ok();
        if let Some(supervisor) = &llama_server_supervisor {
            Arc::clone(supervisor).spawn_supervision();
        }
        crate::memory_vectors::start(&self.config.data_dir, llama_server_supervisor.clone());
        // The MLX engine, on a Mac with Apple Silicon whose build carries it.
        // An MLX model elsewhere is refused with its reason by the router.
        let mlx_engine = crate::mlx_engine::MlxEngineSupervisor::for_this_machine(
            Some(guardrails_file),
            org_policy.imposed_settings(),
        );
        if let Some(engine) = &mlx_engine {
            Arc::clone(engine).spawn_supervision();
        }

        // Phase 4 (pos 5): LlmRouter + LlmBackendRepository, loads backends from system.db
        let (llm_router, llm_backend_repo) = self
            .start_llm_router(
                &system_db_path,
                &llama_server_supervisor,
                &mlx_engine,
                &crate::governance::InferenceGovernance::new(Arc::clone(org_policy)),
            )
            .await;

        // Phase 4b: LlmCallRepository, an EventBus subscriber that persists LLM calls.
        let llm_call_repository = if llm_router.is_some() {
            self.spawn_llm_call_repository(event_sender)
        } else {
            None
        };

        Inference {
            runner_supervisor,
            system_db_path,
            llama_server_supervisor,
            mlx_engine,
            llm_router,
            llm_backend_repo,
            llm_call_repository,
        }
    }

    /// Phase 15: SttEngine, reads config from system.db, then conditionally
    /// starts the engine when `stt_config.enabled = true`.
    pub(super) async fn start_stt(
        &self,
        inference: &Inference,
        event_sender: &EventBusSender,
    ) -> Stt {
        // Opens `SttConfigRepository` against the same `system.db` used by the
        // LLM backend registry. On first boot an empty table is initialised with
        // defaults. The engine is only started when the persisted config has
        // `enabled = true` and the model file exists on disk.
        let stt_config_repo = match SttConfigRepository::open(&inference.system_db_path) {
            Ok(repo) => {
                info!("supervisor.stt_config.ready");
                Some(std::sync::Arc::new(std::sync::Mutex::new(repo)))
            }
            Err(e) => {
                warn!(
                    error = %e,
                    detail = "STT configuration disabled",
                    "supervisor.stt_config.failed"
                );
                None
            }
        };

        let stt_cfg: Option<SttConfigRow> = stt_config_repo.as_ref().and_then(|repo| {
            repo.lock()
                .ok()
                .and_then(|guard| guard.get_or_default().ok())
        });

        let (stt_engine, stt_repository) = self
            .start_stt_engine(stt_cfg.as_ref(), &inference.runner_supervisor, event_sender)
            .await;
        // Wrap the boot snapshot in shared, swappable cells so the STT reload
        // route (and the desktop reload command) can bring a model online
        // mid-session without restarting the daemon. AppState and the embedded
        // runtime handle share the same cells, so a swap is seen by every reader.
        Stt {
            config_repo: stt_config_repo,
            engine: crate::api::server::shared_stt_engine_from(stt_engine),
            repository: crate::api::server::shared_stt_repository_from(stt_repository),
        }
    }
}
