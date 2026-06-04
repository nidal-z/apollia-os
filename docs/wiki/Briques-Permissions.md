# apollia-permissions - Moteur de Permissions 3 Couches

> *Evaluation ordonnée de chaque invocation d'outil : SafeList → PrefixRuleEngine → InjectionDetector. Réduction du bruit HITL sur les commandes sûres, blocage automatique des injections.*
>
> **Référence technique :** [Décision ADR-015](https://github.com/Apollia-OS/apollia-os/wiki/Decisions-Log)

---

## 1. Rôle dans l'architecture

`apollia-permissions` est la crate de contrôle d'accès d'Apollia OS. Elle s'interpose entre l'invocation d'un outil et son exécution réelle dans `ToolRegistry::invoke`.

**Problème résolu :** sans moteur de permissions, chaque invocation d'outil déclenche une demande HITL ou est auto-approuvée sans discernement. 50 `git status` par session = 50 popups pour l'opérateur.

**Principe fondamental :** la SafeList est **vide par défaut**. Aucune commande n'est auto-approuvée sans configuration explicite de l'opérateur. Principe de moindre privilège (OWASP ASVS V1.4, CWE-272).

```
apollia-tools (ToolRegistry::invoke)
  └── apollia-permissions (PermissionEngine::decide)
        ├── InjectionDetector (couche 3 - priorité absolue)
        ├── SafeList          (couche 1 - config opérateur)
        ├── PrefixRuleEngine  (couche 2 - règles SQLite)
        └── PermissionAuditLog (log immuable)
```

> **Référence technique :** [Sécurité - Guardrails](./Securite-Guardrails.md)

---

## 2. Les 3 couches - Ordre d'évaluation

### Couche 3 - InjectionDetector (priorité absolue)

Analyse structurelle du shell, pas regex naïf. S'active **en premier** quelle que soit la configuration.

```rust
pub struct StructuralInjectionDetector;

impl StructuralInjectionDetector {
    pub fn is_injection(command: &str) -> bool {
        Self::has_command_substitution(command)    // POSIX §2.6.3 - $() et backtick
            || Self::has_process_substitution(command) // bash §3.5.6 - >() et <()
            || Self::pipes_into_interpreter(command)   // CWE-78 - | bash, | sh, | python
            || Self::has_unsafe_eval(command)          // ShellCheck SC2046 - eval $VAR
    }
}
```

**Contexte de quoting respecté :** `echo '$(not_executed)'` → `false` (single-quote POSIX §2.2.2).

Patterns détectés :
- `$(...)` - substitution de commande
- `` `...` `` - backtick (ShellCheck SC2006)
- `>(...)`, `<(...)` - process substitution bash
- `| bash`, `| sh`, `| zsh`, `| python`, `| python3`, `| ruby`, `| perl`
- `eval $VAR` - eval avec variable non-quotée (ShellCheck SC2046)

Depuis l'analyse est **structurelle** : elle tient compte du contexte de quoting et des cas multi-lignes que les regex naïfs ratent.

### Couche 1 - SafeList

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
# La liste est VIDE par défaut - l'opérateur définit explicitement ce qui est sûr.
# Exemples (décommenter selon l'environnement) :
# safe_commands = [
#   "bash_executor(git status)",
#   "bash_executor(git log)",
#   "bash_executor(pwd)",
# ]
safe_commands = []
```

### Couche 2 - PrefixRuleEngine

Règles persistées en SQLite, mutables à chaud. Créées via le bouton **"Toujours autoriser"** de l'interface HITL desktop.

```rust
pub struct PrefixRule {
    pub id: i64,                                     // 0 pour une règle non persistée
    pub tool_name: String,
    pub arg_prefix: Option<String>,
    pub action: RuleAction,                          // Allow | Deny
    pub created_at: i64,
    pub created_by_agent: Option<String>,
    pub scope: PermissionScope,                      // Global | Project | Agent | Session
    pub project_path: Option<PathBuf>,               // renseigné si scope == Project
    pub agent_id: Option<String>,                    // renseigné si scope == Agent
    pub expires_at: Option<i64>,                     // Unix timestamp, None = permanent
}
// impl Default : id=0, scope=Global, action=Allow, autres champs vides

pub struct PrefixRuleEngine {
    db: rusqlite::Connection,
}

impl PrefixRuleEngine {
    /// Rétrocompatible : évalue project + global, ignore les règles expirées.
    pub fn check(&self, tool_name: &str, first_arg: Option<&str>) -> Result<Option<RuleAction>, PermissionError> { ... }
    /// Retourne l'id de la règle déclenchée (pour l'audit log).
    pub fn check_with_id(&self, tool_name: &str, first_arg: Option<&str>) -> Result<Option<(i64, RuleAction)>, PermissionError> { ... }
    /// Variante scope-aware : évalue Project (chemin) → Agent (agent_id) → Session (mémoire) → Global.
    pub fn check_with_scope(&self, tool_name: &str, first_arg: Option<&str>, ctx: &ScopeContext, session_rules: &[PrefixRule]) -> Result<Option<(i64, RuleAction)>, PermissionError> { ... }
    pub fn add_rule(&mut self, rule: &PrefixRule) -> Result<i64, PermissionError> { ... }
    pub fn list_rules(&self) -> Result<Vec<PrefixRule>, PermissionError> { ... }
    /// Filtre par scope et chemin projet (Session retourne toujours vide - règles mémoire-uniquement).
    pub fn list_rules_filtered(&self, scope: Option<PermissionScope>, project_path: Option<&Path>) -> Result<Vec<PrefixRule>, PermissionError> { ... }
    /// Supprime toutes les règles persistées correspondant à *scope*.
    /// Pour `Project`, `project_path = None` supprime toutes les règles projet.
    /// Pour `Agent`, supprime toutes les règles agent (tous agent_id confondus).
    /// Erreur si `scope == Session` (règles session non persistées).
    /// Retourne le nombre de lignes supprimées.
    pub fn remove_rules_by_scope(&mut self, scope: PermissionScope, project_path: Option<&Path>) -> Result<u32, PermissionError> { ... }
    /// Supprime toutes les règles `scope = 'agent'` correspondant à `agent_id`. Retourne le nombre supprimé.
    pub fn remove_rules_by_agent(&mut self, agent_id: &str) -> Result<u32, PermissionError> { ... }
    /// Liste les règles `scope = 'agent'` filtrées par `agent_id`.
    pub fn list_rules_for_agent(&self, agent_id: &str) -> Result<Vec<PrefixRule>, PermissionError> { ... }
    /// Liste les règles persistées dont `created_by` correspond. N'inclut pas les règles session (RAM).
    pub fn list_rules_by_creator(&self, created_by: &str) -> Result<Vec<PrefixRule>, PermissionError> { ... }
    /// Supprime toutes les règles persistées dont `created_by` correspond. Retourne le nombre supprimé.
    /// Utilisé pour les resets ciblés (ex : remplacer toutes les règles d'un agent avant réécriture).
    pub fn remove_rules_by_creator(&mut self, created_by: &str) -> Result<u32, PermissionError> { ... }
}
```

Exemple : règle `bash_executor(git:*)` → auto-approuve toutes les commandes `git *`.

### Types scope

```rust
/// Portée d'une règle de permission.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum PermissionScope {
    Session,    // mémoire uniquement - disparaît à l'arrêt du process
    Project,    // persisté SQLite, filtré par chemin canonique du projet
    Agent,      // persisté SQLite, filtré par agent_id (ex. "apollia:chat")
    #[default]
    Global,     // persisté SQLite, s'applique à tout projet et tout agent
}

/// Contexte d'évaluation passé à check_with_scope().
#[derive(Debug, Clone, Default)]
pub struct ScopeContext {
    pub scope: PermissionScope,
    pub project_path: Option<PathBuf>,  // None = hors projet
    pub agent_id: Option<String>,        // None = hors contexte agent
}
```

**Ordre d'évaluation scope-aware (du plus spécifique au plus large) :** Project (chemin exact) → Agent (agent_id exact) → Session (mémoire) → Global. Une règle Project prend toujours le dessus pour le même outil/préfixe.

---

## 3. `PermissionEngine` - Point d'entrée

```rust
pub struct PermissionEngine {
    safe_list: SafeList,
    prefix_rules: PrefixRuleEngine,
    injection_detector: StructuralInjectionDetector,
    audit_log: PermissionAuditLog,
    session_rules: Vec<PrefixRule>,        // règles mémoire-uniquement (scope Session)
    scope_context: Option<ScopeContext>,   // contexte pour check_with_scope
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
    /// Ajoute une règle Session en mémoire (jamais persistée en SQLite).
    /// Force scope = Session quel que soit le scope du PrefixRule reçu.
    pub fn add_session_rule(&mut self, rule: PrefixRule) { ... }
    pub fn clear_session_rules(&mut self) { ... }
    pub fn set_scope_context(&mut self, ctx: ScopeContext) { ... }
    pub fn scope_context(&self) -> Option<&ScopeContext> { ... }
    pub fn session_rules(&self) -> &[PrefixRule] { ... }
}
```

**Intégration dans `ToolRegistry::invoke` :**
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
    pub scope: Option<String>,     // scope de la règle qui a décidé
    pub rule_id: Option<i64>,      // id de la PrefixRule déclenchée (si couche 2)
    pub agent: Option<String>,     // agent_id de l'appelant
}

impl PermissionAuditLog {
    pub fn record(&mut self, tool_name: &str, first_arg: Option<&str>, decision: PermissionDecision) -> Result<(), PermissionError> { ... }
    pub fn query(&self, tool_name: Option<&str>, limit: u32, offset: u32) -> Result<Vec<PermissionAuditEntry>, PermissionError> { ... }
}
```

La table `permission_audit` est **append-only** : des triggers SQLite (`no_update_audit`, `no_delete_audit`) bloquent toute tentative de modification ou suppression d'entrées existantes.

---

## 5. Schéma SQLite

Ces tables résident dans `~/.apollia/governance.db` (base consolidée gérée par `GovernanceDb` dans `apollia-tools`). Au premier démarrage, une éventuelle ancienne `permissions.db` est migrée automatiquement et renommée `permissions.db.bak`.

```sql
-- Règles préfixe (couche 2) - scope-aware
CREATE TABLE IF NOT EXISTS permission_rules (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    tool_name    TEXT NOT NULL,
    arg_prefix   TEXT,
    action       TEXT NOT NULL,                     -- 'allow' | 'deny'
    created_at   INTEGER NOT NULL,
    created_by   TEXT,
    scope        TEXT NOT NULL DEFAULT 'global',    -- 'global' | 'project' | 'agent' | 'session'
    project_path TEXT,                              -- chemin projet si scope='project'
    agent_id     TEXT,                              -- identifiant agent si scope='agent' (ex. 'apollia:chat')
    expires_at   INTEGER                            -- Unix ts, NULL = permanent
);
CREATE INDEX IF NOT EXISTS idx_rules_tool ON permission_rules(tool_name);

-- Audit log (immuable - triggers no_update_audit / no_delete_audit)
CREATE TABLE IF NOT EXISTS permission_audit (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    tool_name   TEXT NOT NULL,
    first_arg   TEXT,
    decision    TEXT NOT NULL,
    decided_at  INTEGER NOT NULL,
    scope       TEXT,       -- scope de la règle déclenchée
    rule_id     INTEGER,    -- id de la PrefixRule (couche 2), NULL sinon
    agent       TEXT        -- agent_id de l'appelant
);
CREATE INDEX IF NOT EXISTS idx_audit_tool ON permission_audit(tool_name, decided_at);
```

---

## 6. Configuration complète

```toml
[permissions]
# Commandes auto-approuvées sans HITL (vide par défaut - moindre privilège)
safe_commands = []

# TTL des règles préfixe SQLite
prefix_rule_ttl_hours = 168  # 7 jours

# Détection d'injection structurelle (désactiver uniquement en dev)
injection_detection = true

# Chemin de la base SQLite consolidée (governance.db)
# Migration automatique depuis permissions.db au premier démarrage.
db_path = "~/.apollia/governance.db"
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

## 8. ADR-015 - Source unique `governance.db` & permissions agent-driven

Depuis ADR-015, `~/.apollia/governance.db` est la **source de vérité unique** lue par
`PermissionEngine.decide()` à chaque invocation. Tous les producteurs convergent vers
cette table via le même point d'entrée logique :

```
              ┌──────────────────────────────────────────┐
              │  governance.db (table permission_rules)   │
              │  source de vérité unique en lecture       │
              └──────────────────┬───────────────────────┘
                                 ▲
                  écrit via PrefixRuleEngine::add_rule()
                                 │
        ┌─────────────────┬──────┴───────────────┬─────────────────────┐
        │                 │                      │                     │
   Agents ReAct       UI HITL                UI Settings          Couche système
   (ctx.tools.call    (« toujours            (édition manuelle)   (migration boot,
    permission_       autoriser »)                                  imports CLI)
    rule_*)
   created_by =       created_by =           created_by =          created_by =
   "<agent_name>"     "user-hitl"            "user-settings"       "config-import"
```

Le champ `created_by` (`permission_rules.created_by`) discrimine systématiquement
l'auteur. Il est consommé par :

- l'UI Settings → Permissions (colonne « Auteur ») pour audit ;
- la CLI `apollia permissions list` ;
- l'outil natif `permission_rule_list(created_by="…")` pour qu'un agent puisse
  inspecter ses propres règles avant d'en proposer de nouvelles.

### 8.1 Outils natifs `permission_rule_*`

Trois outils exposent l'API CRUD aux agents :

| Outil | Paramètres clés | HITL | `is_read_only` |
|---|---|---|---|
| `permission_rule_add` | `tool_name`, `action`, `arg_prefix?`, `scope`, `project_path?`, `agent_id?`, `expires_at?` | **Oui** (ADR-015) | `false` |
| `permission_rule_remove` | `rule_id` | **Oui** | `false` |
| `permission_rule_list` | `tool_name?`, `created_by?`, `scope?` | Non | `true` |

L'écriture passe par le HITL standard : l'utilisateur valide chaque règle dans le
dialogue desktop. Le bouton « toujours accepter pour cette session » couvre les
séries (`PermissionEngine::add_session_rule`).

### 8.2 Migration `safe_list` → `governance.db` au boot

Au démarrage de `PermissionEngine::new()`, les entrées de
`PermissionsConfig.safe_commands` (TOML opérateur historique) sont ingérées en
règles `RuleAction::Allow`, `scope=Global`, `created_by="config-import"`.
Idempotent : un marqueur (présence de règles avec ce `created_by`) court-circuite
l'import. La couche 1 SafeList runtime reste branchée 1-2 sprints le temps de
valider l'absence de régression, puis sera supprimée.

> **Référence ADR :** [ADR-015](../adr/ADR-015-permission-tool-governance.md)

---

## 9. Structure de la crate

```
crates/apollia-permissions/src/
├── lib.rs                  ← exports publics
├── engine.rs               ← PermissionEngine::decide()
├── safe_list.rs            ← Couche 1 : SafeList
├── prefix_rule_engine.rs   ← Couche 2 : PrefixRuleEngine
├── injection_detector.rs   ← Couche 3 : StructuralInjectionDetector
├── audit_log.rs            ← PermissionAuditLog SQLite
├── migrations.rs           ← add_column_if_missing / column_exists (helpers idempotents)
└── error.rs                ← PermissionError
```

> **Voir aussi :** [Sécurité Sandbox Isolation](./Securite-Sandbox-Isolation.md) · [Tool Registry](./Briques-Tool-Registry.md) · [Guardrails](./Securite-Guardrails.md)
