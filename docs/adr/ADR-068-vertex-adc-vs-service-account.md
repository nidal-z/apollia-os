# ADR-068 — Google Vertex AI : ADC vs Clé de Service JSON

**Date :** 2026-04-04
**Statut :** Accepté
**Décideur :** Nidal
**Sprint :** 37 (planifié)

---

## Contexte

Google Vertex AI donne accès aux modèles Gemini et Claude via l'infrastructure cloud de Google. L'API nécessite une authentification OAuth2 ou une clé de service.

**Deux mécanismes d'authentification sont disponibles :**
1. **Application Default Credentials (ADC)** — chaîne de credentials standard Google Cloud
2. **Clé de service JSON** — fichier de credentials téléchargé depuis la console GCP

---

## Décision

**Choix : Application Default Credentials (ADC).**

**Chaîne ADC (ordre de résolution) :**
1. Variable d'environnement `GOOGLE_APPLICATION_CREDENTIALS` → pointe vers un fichier de clé de service JSON
2. Credentials utilisateur `gcloud auth application-default login` → `~/.config/gcloud/application_default_credentials.json`
3. Service Account attaché à l'instance (GCE, Cloud Run, GKE)

**Implémentation :**

```rust
// Résolution des credentials ADC
use gcp_auth::AuthenticationManager;

let auth_manager = AuthenticationManager::new().await?;
let token = auth_manager.get_token(&["https://www.googleapis.com/auth/cloud-platform"]).await?;

// Requête à Vertex AI avec le token
let response = client
    .post(&url)
    .bearer_auth(token.as_str())
    .json(&request_body)
    .send()
    .await?;
```

**Configuration :**
```toml
[[llm.backends]]
type = "api"
name = "vertex-gemini"
provider = "vertex"
model = "gemini-2.0-flash-001"
project_id = "my-gcp-project"
region = "us-central1"
# Credentials : ADC (GOOGLE_APPLICATION_CREDENTIALS ou gcloud auth)
```

### Rejet de la clé de service JSON comme mécanisme primaire

La clé de service JSON est un fichier statique contenant une clé privée RSA. Elle est rejetée comme mécanisme primaire car :
1. **Risque de compromission** : un fichier de clé dans `~/.apollia/` peut être exfiltré avec le reste de la configuration. Les clés de service sont des secrets à vie très longue (pas d'expiration automatique).
2. **Mauvaises pratiques** : les meilleures pratiques Google Cloud recommandent ADC pour les applications locales et IAM pour les déploiements cloud.
3. **Rotation difficile** : une clé de service compromisse nécessite une révocation manuelle dans la console GCP.

ADC délègue la gestion des credentials à `gcloud` ou aux métadonnées d'instance — meilleure séparation des responsabilités.

**La clé de service JSON reste supportée** via `GOOGLE_APPLICATION_CREDENTIALS` (premier élément de la chaîne ADC) — pour les cas d'usage légitimes (CI/CD, scripts automatisés).

---

## Conséquences

**Positives :**
- ADC est le standard recommandé par Google pour les applications locales
- Le token OAuth2 expire et est renouvelé automatiquement par `gcp-auth` — pas de gestion manuelle
- Compatible avec les workflows `gcloud auth application-default login` déjà utilisés par les développeurs GCP

**Négatives / Compromis :**
- Requiert `gcloud` installé ou la variable `GOOGLE_APPLICATION_CREDENTIALS` configurée — une étape de setup supplémentaire vs une clé API simple
- La crate `gcp-auth` ajoute une dépendance — justifiée par la complexité de la rotation de tokens OAuth2

---

## Principes architecturaux impactés

- **Principe #1 — Local-first** : Les credentials sont locaux (`~/.config/gcloud/`). Conforme.
- **Principe #4 — Fail fast** : Si ADC échoue (aucune source de credentials) → `LlmError::ApiKeyMissing` au démarrage avec un message indiquant comment configurer ADC. Conforme.

---

## Liens

- Story d'implémentation : STORY-495 (Sprint 37)
- Implémenté dans : `crates/apollia-llm/src/backends/vertex.rs`
