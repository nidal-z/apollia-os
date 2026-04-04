# apollia-permissions — Moteur de Permissions 3 Couches

> *Evaluation ordonnée de chaque invocation d'outil : SafeList → PrefixRuleEngine → InjectionDetector. Réduction du bruit HITL sur les commandes sûres, blocage automatique des injections.*
>
> **Référence technique :** [Décision ADR-061](https://github.com/nidal-z/apollia-os/wiki/Decisions-Log)

---

## 1. Rôle dans l'architecture

`apollia-permissions` est la crate de contrôle d'accès d'Apollia OS. Elle s'interpose entre l'invocation d'un outil et son exécution réelle dans `ToolRegistry::invoke()`.

**Problème résolu :** sans moteur de permissions, chaque invocation d'outil déclenche une demande HITL ou est auto-approuvée sans discernement. 50 `git status` par session = 50 popups pour l'opérateur.

**Principe fondamental :** la SafeList est **vide par défaut**. Aucune commande n'est auto-approuvée sans configuration explicite de l'opérateur. Principe de moindre privilège (OWASP ASVS V1.4, CWE-272).

```
apollia-tools (ToolRegistry::invoke)
  └── apollia-permissions (PermissionEngine::decide)
        ├── InjectionDetector (couche 3 — priorité absolue)
        ├── SafeList          (couche 1 — config opérateur)
        ├── PrefixRuleEngine  (couche 2 — règles SQLite)
        └── PermissionAuditLog (log immuable)
```

> **Référence technique :** [Sécurité — Guardrails](./Securite-Guardrails.md)

---

## 2. Les 3 couches — Ordre d'évaluation

### Couche 3 — InjectionDetector (priorité absolue)

Analyse structurelle du shell, pas regex naïf. S'active **en premier** quelle que soit la configuration.

```rust
pub struct StructuralInjectionDetector;

impl StructuralInjectionDetector {
    pub fn is_injection(command: &str) -> bool {
        Self::has_command_substitution(command)    // POSIX §2.6.3 — $() et backtick
            || Self::has_process_substitution(command) // bash §3.5.6 — >() et <()
            || Self::pipes_into_interpreter(command)   // CWE-78 — | bash, | sh, | python
            || Self::has_unsafe_eval(command)          // ShellCheck SC2046 — eval $VAR
    }
}
```

**Contexte de quoting respecté :** `echo '$(not_executed)'` → `false` (single-quote POSIX §2.2.2).

Patterns détectés :
- `$(...)` — substitution de commande
- `` `...` `` — backtick (ShellCheck SC2006)
- `>(...)`, `<(...)` — process substitution bash
- `| bash`, `| sh`, `| zsh`, `| python`, `| python3`, `| ruby`, `| perl`
- `eval $VAR` — eval avec variable non-quotée (ShellCheck SC2046)

Depuis STORY-490 (Sprint 36), l'analyse est **structurelle** : elle tient compte du contexte de quoting et des cas multi-lignes que les regex naïfs ratent.

### Couche 1 — SafeList

Liste des commandes auto-approuvées, configurée par l'opérateur dans `apollia.toml`. **Vide par défaut.**

```rust
pub struct SafeList {
    patterns: Vec<SafePattern>,
}

impl SafeList {
    pub fn from_config(config: &PermissionsConfig) -> Self { ... }
    pub fn matches(&self, tool_name: &str, first_arg: Option<&str>) -> bool { ... }
}
```

Principe : seules les commandes **lecture-seule**, sans accès réseau, sans écriture disque.

```toml
[permissions]
# La liste est VIDE par défaut — l'opérateur définit explicitement ce qui est sûr.
# Exemples (décommenter selon l'environnement) :
# safe_commands = [
#   "bash_executor(git status)",
#   "bash_executor(git log)",
#   "bash_executor(pwd)",
# ]
safe_commands = []
```

### Couche 2 — PrefixRuleEngine

Règles persistées en SQLite, mutables à chaud. Créées via le bouton **"Toujours autoriser"** de l'interface HITL desktop.

```rust
pub struct PrefixRule {
    pub id: i64,
    pub tool_name: String,
    pub arg_prefix: Option<String>,
    pub action: RuleAction,        // Allow | Deny
    pub created_at: i64,
    pub created_by_agent: Option<String>,
}

pub struct PrefixRuleEngine {
    db: rusqlite::Connection,
}

impl PrefixRuleEngine {
    pub fn check(&self, tool_name: &str, first_arg: Option<&str>) -> Result<Option<RuleAction>, PermissionError> { ... }
    pub fn add_rule(&mut self, rule: PrefixRule) -> Result<i64, PermissionError> { ... }
    pub fn list_rules(&self) -> Result<Vec<PrefixRule>, PermissionError> { ... }
}
```

Exemple : règle `bash_executor(git:*)` → auto-approuve toutes les commandes `git *`.

---

## 3. `PermissionEngine` — Point d'entrée

```rust
pub struct PermissionEngine {
    safe_list: SafeList,
    prefix_rules: PrefixRuleEngine,
    injection_detector: StructuralInjectionDetector,
    audit_log: PermissionAuditLog,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PermissionDecision {
    AutoAllowedSafeList,
    AutoAllowedPrefixRule { rule_id: i64 },
    AutoDeniedPrefixRule { rule_id: i64 },
    AutoDeniedInjection { pattern: String },
    NeedsApproval,
}

impl PermissionEngine {
    pub fn new(config: &PermissionsConfig, db_path: &std::path::Path) -> Result<Self, PermissionError> { ... }
    pub fn decide(
        &mut self,
        tool_name: &str,
        input: &serde_json::Value,
        agent_manifest: &AgentManifest,
    ) -> Result<PermissionDecision, PermissionError> { ... }
}
```

**Intégration dans `ToolRegistry::invoke()` :**
- `NeedsApproval` → émet `RuntimeEvent::PermissionRequired { tool_name, input, request_id }`
- `AutoDenied*` → retourne `ToolError::PermissionDenied { reason }`
- `AutoAllowed*` → exécution directe, zéro HITL

---

## 4. PermissionAuditLog

Log immuable de chaque décision, persisté en SQLite.

```rust
pub struct PermissionAuditEntry {
    pub id: i64,
    pub tool_name: String,
    pub first_arg: Option<String>,
    pub decision: String,
    pub decided_at: i64,
}

impl PermissionAuditLog {
    pub fn record(&mut self, tool_name: &str, first_arg: Option<&str>, decision: PermissionDecision) -> Result<(), PermissionError> { ... }
    pub fn query(&self, tool_name: Option<&str>, limit: u32, offset: u32) -> Result<Vec<PermissionAuditEntry>, PermissionError> { ... }
}
```

---

## 5. Schéma SQLite

```sql
-- Règles préfixe (couche 2)
CREATE TABLE IF NOT EXISTS permission_rules (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    tool_name   TEXT NOT NULL,
    arg_prefix  TEXT,
    action      TEXT NOT NULL,  -- 'allow' | 'deny'
    created_at  INTEGER NOT NULL,
    created_by  TEXT
);
CREATE INDEX IF NOT EXISTS idx_rules_tool ON permission_rules(tool_name);

-- Audit log (immuable)
CREATE TABLE IF NOT EXISTS permission_audit (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    tool_name   TEXT NOT NULL,
    first_arg   TEXT,
    decision    TEXT NOT NULL,
    decided_at  INTEGER NOT NULL
);
```

---

## 6. Configuration complète

```toml
[permissions]
# Commandes auto-approuvées sans HITL (vide par défaut — moindre privilège)
safe_commands = []

# TTL des règles préfixe SQLite
prefix_rule_ttl_hours = 168  # 7 jours

# Détection d'injection structurelle (désactiver uniquement en dev)
injection_detection = true

# Chemin du fichier SQLite des règles et de l'audit log
db_path = "~/.apollia/permissions.db"
```

---

## 7. Gestion des erreurs

```rust
#[derive(Debug, thiserror::Error)]
pub enum PermissionError {
    #[error("SQLite error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("regex compilation failed: {0}")]
    Regex(#[from] regex::Error),
    #[error("permission denied: {reason}")]
    Denied { reason: String },
    #[error("invalid rule format: {0}")]
    InvalidRule(String),
}
```

---

## 8. Structure de la crate

```
crates/apollia-permissions/src/
├── lib.rs                  ← exports publics
├── engine.rs               ← PermissionEngine::decide()
├── safe_list.rs            ← Couche 1 : SafeList
├── prefix_rule_engine.rs   ← Couche 2 : PrefixRuleEngine
├── injection_detector.rs   ← Couche 3 : StructuralInjectionDetector
├── audit_log.rs            ← PermissionAuditLog SQLite
└── error.rs                ← PermissionError
```

> **Voir aussi :** [Sécurité Sandbox Isolation](./Securite-Sandbox-Isolation.md) · [Tool Registry](./Briques-Tool-Registry.md) · [Guardrails](./Securite-Guardrails.md)
