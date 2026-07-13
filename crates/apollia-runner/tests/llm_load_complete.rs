#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Test d'intégration : spawn le runner, charge un modèle GGUF si disponible
//! (`APOLLIA_TEST_GGUF`), envoie /llm/complete et vérifie la forme de la
//! réponse. Vérifie aussi la validation BAD_REQUEST sur paramètre invalide.
//!
//! Le test "réel" est skippé si la variable d'environnement n'est pas définie,
//! conformément aux conventions des tests Apollia OS (zéro dépendance externe
//! pour le pipeline CI par défaut).
//!
//! Gate : la feature `local-cpu` est requise (sinon les endpoints `/llm/*`
//! retournent 501 UnsupportedOperation et les assertions échouent).

#![cfg(feature = "local-cpu")]

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

/// Spawn le binaire compilé, parse `READY <port>\n`, retourne (child, port).
fn spawn_runner() -> (Child, u16) {
    let bin = env!("CARGO_BIN_EXE_apollia-runner");
    let mut child = Command::new(bin)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn runner");

    let stdout = child.stdout.take().expect("stdout pipe");
    let mut reader = BufReader::new(stdout);
    let mut first_line = String::new();
    reader.read_line(&mut first_line).expect("read READY line");

    let port: u16 = first_line
        .strip_prefix("READY ")
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or_else(|| panic!("malformed READY line: {first_line:?}"));

    (child, port)
}

fn shutdown(child: &mut Child, port: u16) {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("client");
    let _ = client
        .post(format!("http://127.0.0.1:{port}/shutdown"))
        .send();
    // Best-effort wait, kill if it overstays.
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    loop {
        if let Ok(Some(_)) = child.try_wait() {
            return;
        }
        if std::time::Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn complete_rejects_invalid_temperature() {
    let (mut child, port) = spawn_runner();

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("client");

    let body = serde_json::json!({
        "request_id": "11111111-1111-1111-1111-111111111111",
        "params": {
            "model_id": "any",
            "messages": [{"role": "user", "content": "hi"}],
            "max_tokens": 16,
            "temperature": 5.0
        }
    });

    let resp = client
        .post(format!("http://127.0.0.1:{port}/llm/complete"))
        .json(&body)
        .send()
        .expect("complete request");

    assert_eq!(resp.status(), 400, "temperature=5.0 must yield 400");
    let payload: serde_json::Value = resp.json().expect("parse JSON");
    assert_eq!(payload["ok"], false);
    assert_eq!(payload["error"]["code"], "BAD_REQUEST");

    shutdown(&mut child, port);
}

#[test]
fn complete_rejects_oversized_max_tokens() {
    let (mut child, port) = spawn_runner();

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("client");

    let body = serde_json::json!({
        "request_id": "22222222-2222-2222-2222-222222222222",
        "params": {
            "model_id": "any",
            "messages": [{"role": "user", "content": "hi"}],
            "max_tokens": 100_000,
            "temperature": 0.7
        }
    });

    let resp = client
        .post(format!("http://127.0.0.1:{port}/llm/complete"))
        .json(&body)
        .send()
        .expect("complete request");

    assert_eq!(resp.status(), 400);
    let payload: serde_json::Value = resp.json().expect("parse JSON");
    assert_eq!(payload["error"]["code"], "BAD_REQUEST");

    shutdown(&mut child, port);
}

#[test]
fn complete_without_loaded_model_returns_404() {
    let (mut child, port) = spawn_runner();

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("client");

    let body = serde_json::json!({
        "request_id": "33333333-3333-3333-3333-333333333333",
        "params": {
            "model_id": "not-loaded",
            "messages": [{"role": "user", "content": "hi"}],
            "max_tokens": 16,
            "temperature": 0.7
        }
    });

    let resp = client
        .post(format!("http://127.0.0.1:{port}/llm/complete"))
        .json(&body)
        .send()
        .expect("complete request");

    assert_eq!(resp.status(), 404, "model not loaded should yield 404");
    let payload: serde_json::Value = resp.json().expect("parse JSON");
    assert_eq!(payload["error"]["code"], "MODEL_NOT_LOADED");

    shutdown(&mut child, port);
}

#[test]
fn load_model_rejects_missing_path() {
    let (mut child, port) = spawn_runner();

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("client");

    let body = serde_json::json!({
        "request_id": "44444444-4444-4444-4444-444444444444",
        "params": {
            "model_id": "ghost",
            "model_path": "/tmp/this-file-does-not-exist-apollia.gguf",
            "n_ctx": 1024
        }
    });

    let resp = client
        .post(format!("http://127.0.0.1:{port}/llm/load_model"))
        .json(&body)
        .send()
        .expect("load_model request");

    assert_eq!(resp.status(), 400);
    let payload: serde_json::Value = resp.json().expect("parse JSON");
    assert_eq!(payload["error"]["code"], "BAD_REQUEST");

    shutdown(&mut child, port);
}

#[test]
fn load_and_complete_real_model() {
    let model_path = match std::env::var("APOLLIA_TEST_GGUF") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("skipping: set APOLLIA_TEST_GGUF=/path/to/model.gguf to enable");
            return;
        }
    };

    let (mut child, port) = spawn_runner();

    let client = reqwest::blocking::Client::builder()
        // Le chargement d'un GGUF de plusieurs GiB + l'inférence peuvent prendre
        // 30s+ sur CPU ; 120s reste large pour les modèles 7B-Q4.
        .timeout(Duration::from_secs(120))
        .build()
        .expect("client");

    // Load.
    let load_body = serde_json::json!({
        "request_id": "55555555-5555-5555-5555-555555555555",
        "params": {
            "model_id": "test-model",
            "model_path": model_path,
            "n_ctx": 2048,
            "n_gpu_layers": 0
        }
    });
    let load_resp = client
        .post(format!("http://127.0.0.1:{port}/llm/load_model"))
        .json(&load_body)
        .send()
        .expect("load_model request");
    assert_eq!(
        load_resp.status(),
        200,
        "load_model failed: {}",
        load_resp.text().unwrap_or_default()
    );

    // Complete.
    let body = serde_json::json!({
        "request_id": "66666666-6666-6666-6666-666666666666",
        "params": {
            "model_id": "test-model",
            "messages": [{"role": "user", "content": "Say only the word OK and nothing else."}],
            "max_tokens": 16,
            "temperature": 0.0
        }
    });
    let resp = client
        .post(format!("http://127.0.0.1:{port}/llm/complete"))
        .json(&body)
        .send()
        .expect("complete request");

    assert_eq!(resp.status(), 200);
    let payload: serde_json::Value = resp.json().expect("parse JSON");
    assert_eq!(payload["ok"], true);
    assert!(payload["data"]["text"].is_string());
    assert!(
        payload["data"]["usage"]["total_tokens"]
            .as_u64()
            .unwrap_or(0)
            > 0
    );

    shutdown(&mut child, port);
}
