//! The registry workflow runs this before it publishes: the two indexes it
//! signed must hold against the distribution key this tree compiles, or no
//! installed application would accept them.

use std::path::PathBuf;

use apollia_llm::model_cards::verify_index;

fn published_dir() -> PathBuf {
    PathBuf::from(std::env::var("APOLLIA_REGISTRY_DIR").expect("APOLLIA_REGISTRY_DIR is set"))
}

fn compiled_key() -> &'static str {
    apollia_signing::distribution::distribution_public_key().expect(
        "this tree compiles no distribution public key: no installed application could verify the indexes",
    )
}

#[test]
#[ignore = "requires APOLLIA_REGISTRY_DIR, a directory written by the registry workflow"]
fn test_published_indexes_are_signed_by_the_compiled_key() {
    // GIVEN the directory about to be published and the distribution key
    // compiled into this tree
    let dir = published_dir();
    let key = compiled_key();
    let models = std::fs::read(dir.join("models-stable-index.json"))
        .expect("the model index is in the directory");
    let models_signature = std::fs::read_to_string(dir.join("models-stable-index.json.minisig"))
        .expect("its signature is beside it");
    let connections = std::fs::read(dir.join("mcp-stable-index.json"))
        .expect("the connection catalogue is in the directory");
    let connections_signature = std::fs::read_to_string(dir.join("mcp-stable-index.json.minisig"))
        .expect("its signature is beside it");

    // WHEN each is verified as an installed application will
    let index = verify_index(&models, &models_signature, Some(key))
        .expect("the model index is not signed by the distribution key this tree compiles");
    apollia_signing::distribution::verify_minisign(&connections, &connections_signature, key)
        .expect(
            "the connection catalogue is not signed by the distribution key this tree compiles",
        );
    let catalogue: serde_json::Value =
        serde_json::from_slice(&connections).expect("the connection catalogue is JSON");

    // THEN the model index lists cards, and the catalogue is in the format
    //      the application reads, with entries
    assert!(!index.cards.is_empty());
    assert_eq!(catalogue["schema_version"], 1);
    assert!(catalogue["entries"]
        .as_array()
        .is_some_and(|e| !e.is_empty()));
}
