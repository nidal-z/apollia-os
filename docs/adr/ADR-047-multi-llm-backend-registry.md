# ADR-047 — Multi-LLM Backend Registry : SQLite + binding par agent

**Date :** 2026-03-31
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Sprint :** 28

---

## Contexte

Apollia OS supporte aujourd'hui un seul backend LLM, configuré statiquement dans `[llm]`
de `apollia.toml`. Ce modèle a été suffisant durant le développement initial, mais pose
trois problèmes bloquants avant la v1 :

1. **Un seul LLM pour tous les agents.** Un agent d'analyse de code bénéficierait d'un
   modèle spécialisé (ex. LM Studio + qwen2.5-coder), tandis qu'un agent de messagerie
   préférerait Mistral Small pour la rédaction en français. Aujourd'hui, impossible sans
   changer le TOML et redémarrer.

2. **Config statique dans un fichier texte.** `[llm]` est lu au boot et ne peut plus
   être modifié sans éditer le fichier et recharger. L'application desktop ne peut pas
   gérer les backends LLM comme elle gère les agents, triggers, et serveurs MCP (tous en SQLite).

3. **Incohérence architecturale.** Toutes les autres entités avec un lifecycle runtime
   (agents, triggers, MCP servers) sont dans SQLite. Le backend LLM est la seule exception,
   sans justification technique.

**Contraintes :**
- Pas d'utilisateurs existants → suppression directe possible, pas de migration
- Le `LlmRouter` actuel est un wrapper autour d'un seul `Arc<dyn LlmBackend>`
- Les backends locaux (llama-cpp) sont coûteux en mémoire — V1 : tous chargés au boot.
  Lazy load / unload sera une story V2.
- `AgentManifest` est le contrat Python → tout champ ajouté doit être optionnel

---

## Décision

Nous adoptons un **registre de backends LLM en SQLite** avec les composants suivants :

1. **Table `llm_backends`** dans `~/.apollia/system.db` — enregistre n backends,
   un seul marqué `is_default = true`.

2. **`LlmBackendRepository`** dans `apollia-core` — CRUD synchrone (connexion réservée
   au démarrage), même pattern que `TriggerDefinitionRepository`.

3. **`AgentManifest.llm_backend: Option<String>`** — champ optionnel déclaré en Python.
   Si absent ou `None`, l'agent utilise le backend par défaut.

4. **`LlmRouter` multi-backend** — `HashMap<String, Arc<dyn LlmBackend>>` + champ
   `default: String`. La méthode `route(agent_manifest)` lit `llm_backend` du manifest
   et renvoie la référence au bon backend.

5. **API REST `/api/v1/llm/backends`** — CRUD complet (list, get, create, update, delete,
   set-default) accessible depuis l'app desktop et le CLI.

6. **Secrets par interpolation `${VAR}`** — même pattern que les MCP servers (ADR-044).
   Jamais de clé API en clair dans la DB. Les secrets peuvent aussi être stockés via
   le keyring (APOLLIA_SECRET: prefix — cf. ADR-045).

---

## Alternatives considérées

### Option A — Variable d'environnement par agent (rejetée)

Chaque agent lit `APOLLIA_LLM_BACKEND` ou `APOLLIA_LLM_API_KEY` au démarrage.

**Pour :** Zéro changement du runtime, décision entièrement chez l'agent.
**Contre :** L'agent devient responsable de la config d'infrastructure. Viole le
Principe #3 (contrat minimal). Impossible à gérer depuis l'app desktop. Pas de routing
centralisé.

### Option B — Fichier de config par agent (rejetée)

Chaque agent a un fichier `<name>.config.toml` avec son backend LLM et ses overrides.

**Pour :** Simple, indépendant.
**Contre :** Prolifération de fichiers. Même problème d'éditabilité runtime que `[llm]`.
Incohérent avec SQLite pour tous les autres objets.

### Option C — LLM dans AgentManifest uniquement, sans registre central (rejetée)

L'agent déclare directement la config complète du LLM dans son `manifest()`.

**Pour :** Tout dans le manifest, un seul endroit.
**Contre :** Les API keys dans le code Python → catastrophe sécurité. L'agent devient
responsable de la config d'infra. Impossible de changer de clé sans éditer le Python.

### Option retenue — Registre SQLite + champ `llm_backend` dans le manifest

**Pour :** Cohérent avec l'architecture existante (tous les entités en SQLite), séparation
claire entre "quel backend" (manifest) et "comment ce backend est configuré" (registre),
gestion centralisée des secrets, éditable depuis le desktop sans redémarrage.

**Compromis acceptés :**
- `LlmRouter` doit être refactorisé (multi-backend au lieu de single)
- Les backends locaux (llama-cpp) sont tous chargés au boot en V1 → consommation mémoire
  si plusieurs modèles lourds sont enregistrés. V2 ajoutera le lazy load.
- `AgentManifest` gagne un nouveau champ → les agents Python existants doivent être
  mis à jour (ou ignorer le champ — il est optionnel)

---

## Schéma SQLite

```sql
-- Dans ~/.apollia/system.db (nouvelle DB dédiée aux configs système)
CREATE TABLE IF NOT EXISTS llm_backends (
    name         TEXT PRIMARY KEY,
    provider     TEXT NOT NULL,
    model        TEXT NOT NULL,
    config_json  TEXT NOT NULL DEFAULT '{}',
    enabled      BOOLEAN NOT NULL DEFAULT 1,
    is_default   BOOLEAN NOT NULL DEFAULT 0,
    created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    CHECK (provider IN ('llama-cpp', 'openai', 'mistral', 'anthropic', 'ollama'))
);
```

`config_json` contient les paramètres provider-spécifiques :
- llama-cpp : `{ "model_path": "/path/to/model.gguf", "n_gpu_layers": 35 }`
- openai : `{ "base_url": "https://api.openai.com/v1", "api_key": "${OPENAI_API_KEY}" }`
- mistral : `{ "api_key": "${MISTRAL_API_KEY}" }`
- anthropic : `{ "api_key": "${ANTHROPIC_API_KEY}" }`
- ollama : `{ "base_url": "http://localhost:11434" }`

---

## Types Rust principaux

```rust
// crates/apollia-core/src/llm_backend.rs

/// Registre d'un backend LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmBackendConfig {
    pub name: String,
    pub provider: LlmProvider,
    pub model: String,
    pub config_json: serde_json::Value,
    pub enabled: bool,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum LlmProvider {
    LlamaCpp,
    OpenAi,
    Mistral,
    Anthropic,
    Ollama,
}

// crates/apollia-core/src/manifest.rs (ajout)
pub struct AgentManifest {
    // ... champs existants ...
    /// Backend LLM à utiliser. None = backend par défaut du runtime.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub llm_backend: Option<String>,
}
```

---

## Conséquences

**Positives :**
- Plusieurs agents peuvent utiliser des LLMs différents simultanément
- Config LLM éditable depuis l'app desktop sans redémarrage du runtime
- Cohérence complète : toutes les entités runtime sont dans SQLite
- Les API keys ne quittent jamais la machine (keyring ou env vars)
- Rollback facile : changer `is_default` dans la DB suffit

**Négatives / Compromis :**
- LlmRouter refactorisé — risque de régression sur le routing existant
- Backends locaux tous chargés au boot en V1 — consommation mémoire si plusieurs GGUF
- `AgentManifest` s'agrandit (breaking change mineur du contrat Python, optionnel)

**À surveiller :**
- Comportement quand le backend nommé dans le manifest n'est pas dans la DB
  (→ fallback sur le défaut avec warning, pas d'erreur)
- Performance du routing multi-backend vs single backend actuel

---

## Principes architecturaux impactés

- **Principe #1 — Local-first :** Respecté. Les API keys restent locales (env vars, keyring).
  Les appels LLM peuvent être distants si le provider est distant — c'est un choix explicite
  de l'utilisateur qui configure le backend.
- **Principe #2 — Zéro dépendance externe :** Respecté. Pas de nouvelle crate. Le LlmRouter
  existant est étendu, pas remplacé.
- **Principe #3 — Contrat minimal :** Respecté. `llm_backend` est optionnel dans le manifest.
  Les agents existants sans ce champ continuent de fonctionner.
- **Principe #4 — Fail fast :** Respecté. Si un backend nommé est introuvable au démarrage
  de l'agent → warning immédiat + fallback sur le défaut.

---

## Liens

- Stories associées : STORY-374, STORY-375, STORY-376, STORY-377, STORY-378
- ADR précédent sur LLM : ADR-020 (LLM moteur embarqué — llama.cpp + feature flags)
- ADR remplacé partiellement : ADR-020 (la configuration statique TOML est remplacée)
- Sprint : 28
