//! The load guardrail as the supervisor applies it: before a launch, from the
//! `[llm.guardrails]` section read at that moment.

use super::*;
use crate::llama_server::test_support::live_instance;
use crate::local_engine::guardrails::BYPASS_EVENT_SITE;

/// A model file of one mebibyte, whose estimate the engine overhead alone puts
/// far above a ceiling of a thousandth of a gigabyte.
fn model_file(dir: &std::path::Path) -> String {
    let path = dir.join("qwen.gguf");
    std::fs::write(&path, vec![0u8; 1 << 20]).unwrap();
    path.display().to_string()
}

fn write_policy(file: &std::path::Path, section: &str) {
    std::fs::write(file, format!("[llm.guardrails]\n{section}\n")).unwrap();
}

#[tokio::test]
async fn test_a_refused_model_leaves_the_resident_one_and_the_key_is_read_at_every_launch() {
    // GIVEN a resident model, a stand-in engine, and an apollia.toml whose
    // guardrail holds launches to a ceiling of 0.001 GB
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("apollia.toml");
    write_policy(&config, "level = \"ceiling\"\nceiling_gb = 0.001");
    let mut sup = supervisor_with_ceiling(1);
    sup.bin_path = fake_engine(dir.path());
    sup.config_file = Some(config.clone());
    let resident = live_instance(&sup, "/models/other.gguf", 9301).await;
    sup.instances.lock().await.push(resident);
    let model = model_file(dir.path());

    // WHEN the model is asked for, then asked again once the key says
    // `disabled`, with no restart in between
    let refused = sup
        .ensure_model("qwen", model.clone(), None, LoadConsent::Unconfirmed)
        .await;
    let resident_after_refusal: Vec<String> = sup
        .loaded_models()
        .await
        .into_iter()
        .map(|m| m.model_path)
        .collect();
    write_policy(&config, "level = \"disabled\"");
    let admitted = sup
        .ensure_model("qwen", model.clone(), None, LoadConsent::Unconfirmed)
        .await;

    // THEN the first is refused with the estimate and the ceiling, nothing was
    // launched and the resident model stayed; the second is launched
    let refusal = match refused {
        Err(LlamaServerError::GuardrailRefused(refusal)) => Some(refusal),
        _ => None,
    }
    .expect("the first launch is refused by the guardrail");
    assert_eq!(refusal.budget_gb, 0.001);
    assert!(refusal.estimated_gb > 0.001, "{refusal:?}");
    assert_eq!(refusal.model, model);
    assert_eq!(
        resident_after_refusal,
        vec!["/models/other.gguf".to_owned()]
    );
    drop(admitted.unwrap());
    let lines = launches(dir.path());
    assert_eq!(lines.len(), 1, "{lines:#?}");
    assert!(lines[0].contains("qwen.gguf"), "{}", lines[0]);
    sup.shutdown_in_place().await;
}

#[tokio::test]
async fn test_a_confirmed_load_goes_past_a_refusal() {
    let _site = BYPASS_EVENT_SITE.lock().await;
    // GIVEN a stand-in engine and a guardrail that refuses the model
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("apollia.toml");
    write_policy(&config, "level = \"ceiling\"\nceiling_gb = 0.001");
    let mut sup = supervisor_with_ceiling(1);
    sup.bin_path = fake_engine(dir.path());
    sup.config_file = Some(config);
    let model = model_file(dir.path());

    // WHEN the load is asked with an explicit confirmation
    let lease = sup
        .ensure_model("qwen", model, None, LoadConsent::Confirmed)
        .await;

    // THEN the model is launched despite the refusal
    drop(lease.unwrap());
    assert_eq!(launches(dir.path()).len(), 1);
    sup.shutdown_in_place().await;
}
