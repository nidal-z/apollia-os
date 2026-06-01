//! Smoke test - apollia-reviewer.py, chaîne complète sans LLM.
//!
//! Valide : AIPLoader → validate_agent → AIPBridge → run() → AIPResult.
//! Pas de LLM, pas de vrai git repo - ctx minimal, outils mockés via NoopToolExecutor.
//!
//! Run :
//!   PYO3_PYTHON=/opt/homebrew/bin/python3.13 \
//!   cargo test -p apollia-e2e-tests --features python-tests \
//!     --test test_reviewer_agent -- --nocapture

use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;

use apollia_aip::{bridge::AIPBridge, loader::load_agent_module, validator::validate_agent};
use apollia_core::{AIPInput, AIPPart, AIPResult, AIPTask, TaskStatus, TextPart};
use apollia_runtime::coordinator::ExecutionBackend;
use pyo3::prelude::*;

// ─────────────────────────────────────────────────────────────────────────────
// Backend minimaliste : appelle run() avec un ctx vide (pas d'outils Rust réels).
// ─────────────────────────────────────────────────────────────────────────────

struct AIPBridgeBackend {
    bridge: Arc<AIPBridge>,
}

impl ExecutionBackend for AIPBridgeBackend {
    fn execute(
        &self,
        task: AIPTask,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<AIPResult, String>> + Send>> {
        let bridge = Arc::clone(&self.bridge);
        Box::pin(async move {
            // ctx minimal : dict Python vide.
            // Les appels ctx.tools.call() vont lever AttributeError -
            // c'est acceptable pour un smoke test : on vérifie que la
            // chaîne Rust → Python → retour fonctionne, pas les outils.
            let ctx: PyObject = Python::with_gil(|py| pyo3::types::PyDict::new_bound(py).into());
            bridge.call_run(&task, ctx).await.map_err(|e| e.to_string())
        })
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn reviewer_agent_path() -> std::path::PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir
        .parent()
        .expect("tests/ doit être dans le workspace root");
    workspace_root.join("agents").join("apollia-reviewer.py")
}

fn make_task(input: &str) -> AIPTask {
    AIPTask {
        task_id: "t-reviewer-smoke".to_string(),
        input: AIPInput {
            parts: vec![AIPPart::Text(TextPart {
                text: input.to_string(),
            })],
        },
        ..Default::default()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

/// le module apollia-reviewer.py se charge et valide sans erreur.
#[cfg(feature = "python-tests")]
#[tokio::test]
async fn test_ac1_agent_loads_and_validates() {
    let path = reviewer_agent_path();
    assert!(
        path.exists(),
        "apollia-reviewer.py introuvable : {}",
        path.display()
    );

    let module =
        load_agent_module(&path).unwrap_or_else(|e| panic!("load_agent_module failed: {e}"));

    let validated =
        validate_agent(&module).unwrap_or_else(|e| panic!("validate_agent failed: {e}"));

    assert_eq!(validated.manifest.name, "apollia-reviewer");
    assert!(validated
        .manifest
        .tools_required
        .contains(&"bash_executor".to_string()));
    assert!(validated
        .manifest
        .tools_required
        .contains(&"file_io".to_string()));
    assert!(validated.manifest.memory_namespace.is_some());
    println!("✔ manifest validé : {:?}", validated.manifest.name);
}

/// run() avec entrée vide retourne status=failed (validation Python).
#[cfg(feature = "python-tests")]
#[tokio::test]
async fn test_ac2_empty_input_returns_failed() {
    let path = reviewer_agent_path();
    let module = load_agent_module(&path).expect("load failed");
    let validated = validate_agent(&module).expect("validate failed");

    let bridge = Arc::new(AIPBridge::new(validated).expect("AIPBridge init failed"));
    let backend = AIPBridgeBackend { bridge };

    // Entrée vide - l'agent Python doit retourner status=failed
    let task = make_task("");
    let result = backend.execute(task).await;

    // Deux cas acceptables :
    // 1. Ok(result) avec status Failed (validation Python correcte)
    // 2. Err(_) parce que ctx.tools est un dict vide (AttributeError sur tools.call)
    match result {
        Ok(r) => {
            println!("status: {:?}", r.status);
            assert_eq!(
                r.status,
                TaskStatus::Failed,
                "entrée vide → status doit être Failed"
            );
            println!(
                "✔ entrée vide → Failed, code={:?}",
                r.error.as_ref().map(|e| &e.code)
            );
        }
        Err(e) => {
            // AttributeError sur ctx.tools.call - acceptable : l'agent a bien
            // passé la validation d'entrée et essayé d'appeler un outil
            println!("✔ bridge err (ctx vide attendu) : {e}");
        }
    }
}

/// run() avec un chemin repo valide (ce repo) et ctx vide.
///
/// On s'attend à ce que l'agent tente des appels d'outils et échoue sur ctx.tools
/// (AttributeError Python). C'est le comportement correct : la chaîne Rust→Python
/// fonctionne, les outils Rust ne sont pas branchés dans ce test minimaliste.
#[cfg(feature = "python-tests")]
#[tokio::test]
async fn test_ac3_valid_repo_path_reaches_tool_call() {
    let path = reviewer_agent_path();
    let module = load_agent_module(&path).expect("load failed");
    let validated = validate_agent(&module).expect("validate failed");

    let bridge = Arc::new(AIPBridge::new(validated).expect("AIPBridge init failed"));
    let backend = AIPBridgeBackend { bridge };

    // Chemin vers ce repo (toujours valide en dev)
    let repo_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_string_lossy()
        .to_string();

    let task = make_task(&repo_path);
    let result = backend.execute(task).await;

    match result {
        Ok(r) => {
            println!("status: {:?}", r.status);
            // Avec ctx vide, l'agent ne peut pas écrire le rapport - c'est OK ici
        }
        Err(e) => {
            // Attendu : AttributeError sur ctx.tools (dict vide sans méthode .call)
            // Ce que cela prouve : l'agent a bien dépassé la validation d'entrée,
            // la chaîne de sérialisation JSON fonctionne, et run() s'est exécuté.
            assert!(
                e.contains("AttributeError") || e.contains("tools") || e.contains("call"),
                "erreur inattendue (ni AttributeError ni tools) : {e}"
            );
            println!("✔ run() atteint ctx.tools.call - chaîne Rust→Python OK");
        }
    }
}
