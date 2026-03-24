//! Tests e2e — `EmbeddedBackend` avec un mini-modèle GGUF local.
//!
//! Nécessite :
//! - `feature = "local"` — active l'inférence in-process via mistralrs.
//! - `feature = "python-tests"` — environnement Python disponible.
//! - `APOLLIA_TEST_MODEL_PATH` — chemin vers un fichier `.gguf` de test.
//!
//! Ces tests ne s'exécutent PAS en CI (feature `local` exclue du workflow).
//! À exécuter en dev avec :
//!   APOLLIA_TEST_MODEL_PATH=/chemin/vers/modele.gguf \
//!   PYO3_PYTHON=/opt/homebrew/bin/python3.13 \
//!   cargo test -p apollia-e2e-tests --features python-tests,local -- --nocapture

#[cfg(feature = "local")]
mod embedded_tests {
    use apollia_llm::{
        backends::embedded::{AcceleratorDevice, EmbeddedBackend, EmbeddedBackendConfig},
        CompletionModel, CompletionRequest,
    };

    /// Récupère le chemin du modèle depuis `$APOLLIA_TEST_MODEL_PATH`.
    ///
    /// Retourne `None` si la variable d'environnement n'est pas définie,
    /// auquel que le test est ignoré.
    fn model_path() -> Option<std::path::PathBuf> {
        std::env::var("APOLLIA_TEST_MODEL_PATH")
            .ok()
            .map(std::path::PathBuf::from)
    }

    /// `EmbeddedBackend::load()` charge un modèle GGUF depuis le chemin fourni.
    ///
    /// Test ignoré si `$APOLLIA_TEST_MODEL_PATH` n'est pas défini.
    #[tokio::test]
    async fn test_embedded_backend_load_from_env_path() {
        // GIVEN APOLLIA_TEST_MODEL_PATH défini et pointant vers un .gguf valide
        let path = match model_path() {
            Some(p) => p,
            None => {
                eprintln!("[SKIP] test_embedded_backend_load_from_env_path : APOLLIA_TEST_MODEL_PATH non défini");
                return;
            }
        };

        assert!(
            path.exists(),
            "APOLLIA_TEST_MODEL_PATH doit pointer vers un fichier existant : {}",
            path.display()
        );

        let config = EmbeddedBackendConfig {
            name: "test-local".to_owned(),
            model_path: path,
            device: AcceleratorDevice::Cpu,
        };

        // WHEN EmbeddedBackend::load() est appelé
        let result = EmbeddedBackend::load(&config).await;

        // THEN le backend est chargé sans erreur
        assert!(
            result.is_ok(),
            "EmbeddedBackend::load() doit réussir avec un .gguf valide : {result:?}"
        );

        let backend = result.unwrap();
        assert!(
            backend.is_available(),
            "le backend doit être disponible après chargement"
        );
    }

    /// `backend.complete()` retourne une réponse non vide avec `cost_usd == None`.
    ///
    /// Test ignoré si `$APOLLIA_TEST_MODEL_PATH` n'est pas défini.
    #[tokio::test]
    async fn test_embedded_backend_complete_returns_response() {
        // GIVEN un backend chargé depuis le chemin de test
        let path = match model_path() {
            Some(p) => p,
            None => {
                eprintln!("[SKIP] test_embedded_backend_complete_returns_response : APOLLIA_TEST_MODEL_PATH non défini");
                return;
            }
        };

        let config = EmbeddedBackendConfig {
            name: "test-local".to_owned(),
            model_path: path,
            device: AcceleratorDevice::Cpu,
        };

        let backend = EmbeddedBackend::load(&config)
            .await
            .expect("EmbeddedBackend::load() doit réussir");

        let req = CompletionRequest {
            messages: vec![apollia_llm::ChatMessage::user("Réponds 'ok' uniquement.")],
            max_tokens: Some(16),
            ..Default::default()
        };

        // WHEN complete() est appelé
        let response = backend.complete(req).await;

        // THEN Ok(CompletionResponse) est retourné avec du contenu non vide
        assert!(
            response.is_ok(),
            "backend.complete() doit retourner Ok : {response:?}"
        );

        let resp = response.unwrap();
        assert!(
            !resp.content.is_empty(),
            "le contenu de la réponse ne doit pas être vide"
        );
        // ET cost_usd == None (backend local — pas de coût cloud)
        assert_eq!(
            resp.usage.cost_usd, None,
            "cost_usd doit être None pour un backend local"
        );
    }
}
