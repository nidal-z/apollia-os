# ADR-067 — AWS Bedrock : aws-sigv4 Natif vs SDK Complet

**Date :** 2026-04-04
**Statut :** Accepté
**Décideur :** Nidal
**Sprint :** 37 (planifié)

---

## Contexte

AWS Bedrock est le service d'IA managé d'Amazon, qui donne accès à Claude (Anthropic), Llama (Meta), Mistral et d'autres modèles via une API compatible Messages API.

Pour intégrer Bedrock dans `apollia-llm`, deux approches sont possibles :
1. **aws-sdk-rust** — le SDK officiel AWS pour Rust (aws-sdk-bedrockruntime)
2. **aws-sigv4 natif** — signature manuelle des requêtes HTTP avec la crate `aws-sigv4`

---

## Décision

**Choix : aws-sigv4 natif** via la crate `aws-sigv4` + `reqwest` pour les requêtes HTTP.

**Justification :**

| Critère | aws-sdk-rust | aws-sigv4 natif |
|---------|-------------|-----------------|
| Dépendances ajoutées | ~50 crates | ~5 crates |
| Temps de compilation | +45s | +3s |
| Binary size | +8 MB | +0.5 MB |
| Fonctionnalités utilisées | ~2% du SDK | 100% |
| Maintenance | AWS officielle | Crate tierces bien maintenues |

**Implémentation :**

```rust
// Signature des requêtes Bedrock
use aws_sigv4::http_request::{sign, SigningSettings, SigningParams};

let signing_params = SigningParams::builder()
    .access_key(&credentials.access_key_id)
    .secret_key(&credentials.secret_access_key)
    .region(&region)
    .service_name("bedrock-runtime")
    .time(SystemTime::now())
    .settings(SigningSettings::default())
    .build()?;

let (signed_headers, _) = sign(request, &signing_params)?;
```

**Format de requête :** Bedrock accepte le format Anthropic Messages API avec l'URL `https://bedrock-runtime.{region}.amazonaws.com/model/{model_id}/invoke`.

**Configuration :**
```toml
[[llm.backends]]
type = "api"
name = "bedrock-claude"
provider = "bedrock"
model = "anthropic.claude-3-sonnet-20241022-v2:0"
region = "us-east-1"
# Credentials via environment variables : AWS_ACCESS_KEY_ID + AWS_SECRET_ACCESS_KEY
# ou via ~/.aws/credentials (résolution automatique)
```

---

## Conséquences

**Positives :**
- Binaire significativement plus petit (Principe #2 — zéro dépendance excessive)
- Compilation plus rapide en CI
- La signature SigV4 est un standard stable — `aws-sigv4` est peu susceptible de changer

**Négatives / Compromis :**
- Pas de support automatique des credentials avancés (SSO, IAM roles sur EC2) — implémentation manuelle si nécessaire
- Si AWS change le format de signature (très improbable — SigV4 est stable depuis 2012), une mise à jour manuelle est requise

---

## Principes architecturaux impactés

- **Principe #2 — Zéro dépendance externe** : Minimise les dépendances au strict nécessaire. Conforme.
- **Principe #4 — Fail fast** : Credentials manquants → `LlmError::ApiKeyMissing` au démarrage. Conforme.

---

## Liens

- Story d'implémentation : STORY-494 (Sprint 37)
- Implémenté dans : `crates/apollia-llm/src/backends/bedrock.rs`
