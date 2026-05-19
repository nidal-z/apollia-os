# ADR-096 — Tool execution paths convergence

**Status:** Accepté — 2026-05-18
**Decision-makers:** Nidal
**Supersedes:** —
**Superseded-by:** —
**Related:** ADR-088 (architecture hybride connecteurs natifs + MCP), ADR-090 (Connector trait), ADR-082 (Tool governance).

## Contexte

Trois modes d'exécution d'outils coexistent dans Apollia OS :

1. **Chat Libre** — boucle ReAct Rust native dans `BuiltInChatAgent` ; passe par `NativeChatToolInvoker` (`apollia-runtime/src/chat/builtin_agent.rs`), un trait `apollia_llm::ToolInvoker` implémenté par un `match tool_name` hardcodé pour les 16 outils natifs.
2. **Chat Agent (Python)** — agent custom via PyO3 ; passe par `BridgeRunner` (`apollia-cli/src/commands/start.rs`) → `ToolProxy` → `DispatcherExecutor` → `ToolDispatcher` (`apollia-tools/src/executor.rs`).
3. **Triggers** (cron / interval / filewatch / webhook) — soumet une `AIPTask` au `TaskRouter` ; partage le pipeline du Chat Agent (même `BridgeRunner`, même `ToolDispatcher`).

Conséquence avant cette ADR : **toute nouvelle famille d'outils** (MCP, Google connector, Microsoft connector, futurs Slack/Notion natifs…) devait être câblée à deux endroits :

- `NativeChatToolInvoker` → ajout d'un champ optionnel (`mcp_handle`, `connector_invoker`) + ligne dans le `match tool_name`.
- `build_dispatcher_with` (`apollia-tools/src/native_dispatcher.rs`) → ajout d'un `Box<dyn ToolExecutor>` dans le Vec d'extras.

Dette observée pendant l'intégration Google :

- `apollia-runtime` ne dépendait pas de `apollia-connectors` → impossible d'enregistrer les `ToolDescriptor`s connecteur dans le `ToolRegistry` au boot sans bricolage.
- Quand l'utilisateur a connecté Google et appelé `gcal.create_event` depuis Chat Libre, le LLM voyait l'outil dans son catalogue (descriptor enregistré) mais le `NativeChatToolInvoker` retournait `unknown tool: gcal.create_event` parce que son hardcoded match ne le connaissait pas.
- Le wiring de Phase 0 a corrigé Chat Libre via un nouveau champ `connector_invoker`. Cette ADR documente la décision de **converger** les trois paths plutôt que d'accumuler les champs spéciaux.

## Décision

Établir un **unique point de dispatch pour les outils non-natifs** dans Chat Libre :

```rust
pub struct NativeChatToolInvoker {
    // Fast path : les 16 outils natifs gardent leur match hardcodé
    // (préserve l'HITL filesystem inline, le sandboxing, l'ask_user
    // pending registry — comportements UX spécifiques que la migration
    // briserait à coût élevé).
    sandbox_root, workspace_path, event_bus, …,

    // MCP : routage existant via `mcp_handle.call_tool` (gardé pour
    // l'instant — le pipeline MCP a sa propre logique de re-essai et
    // d'image content que le dispatcher ne reproduit pas trivialement).
    mcp_handle: Option<McpClientManagerHandle>,

    // Slot unique pour tout le reste — connecteurs natifs (Google,
    // Microsoft, futurs) + toute extension future. Une seule attache,
    // un seul point d'enregistrement.
    fallback_dispatcher: Option<Arc<dyn ToolInvoker>>,
}
```

### Précédence dans `invoke()`

1. **MCP** si `tool_name` matche `mcp:<server>/<tool>` → `mcp_handle.call_tool()`.
2. **Native fast path** : match sur les 16 outils natifs hardcodés.
3. **Fallback dispatcher** : tout le reste (Google `gmail.*` / `gcal.*` / `gdrive.*`, Microsoft à venir, futurs natifs SaaS).

### Adapter `DispatcherToolInvoker`

Nouveau module `apollia-tools/src/dispatcher_invoker.rs` :

```rust
pub struct DispatcherToolInvoker { dispatcher: Arc<ToolDispatcher> }
impl ToolInvoker for DispatcherToolInvoker { ... }
```

Convertit `Result<Value, ToolExecutionError>` (le contrat dispatcher) en `Result<String, String>` (le contrat ToolInvoker), avec mapping d'erreurs préservant les codes stables (`unknown_tool`, `invalid_input`, etc.). 3 tests unitaires couvrent : echo, unknown_tool, sortie pure-string sans escape JSON.

### Convergence des descripteurs

Tous les `ToolDescriptor` connecteurs sont enregistrés au boot du supervisor (`apollia-runtime/src/supervisor.rs` Phase 3b), pas par les modules consommateurs. Source unique : `apollia-runtime/src/connectors_bridge.rs::all_connector_descriptors()`.

## Alternatives considérées

### A — Status quo : N champs spéciaux par famille d'outils

Garder `mcp_handle`, `connector_invoker`, ajouter `microsoft_invoker`, etc. au fil de l'eau.

**Rejeté** : chaque nouveau provider exige 3 patches synchrones (descriptor, fast path Chat Libre, executor Agent mode). Risque garanti de divergence à la prochaine itération.

### B — Tout fusionner dans `ToolDispatcher`, supprimer `NativeChatToolInvoker`

Migrer les 16 outils natifs en `ToolExecutor` (déjà existants dans `apollia-tools::tools::*`), supprimer `NativeChatToolInvoker` complètement. Chat Libre construit un dispatcher, le wrappe en `DispatcherToolInvoker`, l'utilise comme son `ToolInvoker`.

**Reporté à Phase 2** (post-v0.1.0) : le coût immédiat est la migration de l'HITL filesystem inline (`check_fs_hitl`) et l'`ask_user` pending registry vers un mécanisme d'events EventBus. Faisable mais hors fenêtre release du 20 mai 2026.

### C — Unifier sur `ToolInvoker` (strings), abandonner `ToolDispatcher`

Inverse de B : éliminer le contrat JSON-typé au profit du contrat string-only.

**Rejeté** : `ToolDispatcher` apporte des fonctionnalités absentes de `ToolInvoker` — batch parallel exécution (réutilisé par ORIA), permission engine 3-couches (ADR-082), audit trail, session tool filter. Sacrifier ces capacités pour simplifier Chat Libre est un mauvais trade.

## Conséquences

### Positives

- **Un seul point d'attache** pour les nouveaux outils SaaS → `with_fallback_dispatcher(Arc::new(MyProviderInvoker))` côté Chat Libre, executor dans le dispatcher côté Agent. Adding Microsoft = 2 lignes de wiring.
- **Descripteurs enregistrés une fois** au supervisor boot → garanti que tous les modes voient le même catalogue.
- **Path Phase 2 clair** : remplacer `Arc<dyn ToolInvoker>` du `fallback_dispatcher` par `Arc<DispatcherToolInvoker>` wrappant le dispatcher Agent partagé → un seul dispatcher pour tous les modes.

### Négatives / Trade-offs assumés

- **Deux paths persistent en Phase 1** : Agent/Triggers (vrai `ToolDispatcher` avec permission engine actif) vs Chat Libre (fast path + fallback). Les outils natifs en Chat Libre **n'ont pas** le permission engine — c'était déjà le cas avant cette ADR.
- **MCP garde sa logique séparée** (`mcp_handle.call_tool`) parce que le pipeline gère du contenu image + retries spécifiques mal modélisés par `ToolDispatcher`. À unifier en Phase 2.
- **HITL filesystem inline** vit toujours dans `NativeChatToolInvoker.invoke_file_*`. Migration vers events Phase 2.

### Risques

- Si un nouveau provider connector oublie d'enregistrer son descriptor au boot, ses outils seront invisibles au LLM même si l'executor existe → pas de régression silencieuse, l'agent dira "outil inconnu".
- Si quelqu'un ajoute un nouvel outil natif sans le mettre dans le match hardcodé, le fallback dispatcher prendra le relais → si l'outil n'a pas non plus d'`ToolExecutor` enregistré, l'erreur sera claire ("unknown tool: X"). Pas de comportement obscur.

## Plan de migration (Phases)

| Phase | Statut | Contenu |
|---|---|---|
| **Phase 0** | ✅ 2026-05-18 | Drive folder user-selectable (`apollia-auth/src/drive_prefs.rs` + UI wizard step + Settings card). |
| **Phase 1** | ✅ 2026-05-18 | `DispatcherToolInvoker` adapter (`apollia-tools/src/dispatcher_invoker.rs`), champ `fallback_dispatcher` sur `NativeChatToolInvoker`, attache dans `chat::manager::resolve_workspace_for_session`, suppression du champ `connector_invoker` redondant. |
| **Phase 2** | ✅ 2026-05-18 | Vrai `ToolDispatcher` construit dans `chat::manager` contenant : MCP executors (un par tool des serveurs connectés), Google connector executors (14 ops), 5 natifs read-only (file_read, file_list, file_glob, file_grep, notebook_read). Suppression complète du champ `mcp_handle` et de son routage inline. Suppression des 5 méthodes `invoke_file_read/list/glob/grep` + `invoke_notebook_read`. **Permission engine + audit trail désormais actifs pour MCP/connecteurs/read-only natifs dans Chat Libre** (étaient bypass auparavant). |
| **Phase 3** | ✅ 2026-05-18 | (Step 0 — fix critique) Bloc temporel/environnement (`apollia_core::temporal_context`) injecté au sommet de tout system prompt — Chat Libre via `build_system_prompt`, Chat Agent + Triggers via `ctx.llm.chat()` / `ctx.llm.complete()` côté PyO3. Format ISO 8601 + weekday + timezone + UTC + instruction explicite « overrides training data cutoff ». Fini les agents qui créent des événements en octobre 2023. (Convergence dispatcher) `ChatToolsConfig` plumbé du supervisor au chat manager, permet à `resolve_workspace_for_session` de construire un `ToolDispatcher` complet via `build_dispatcher_with` avec gouvernance, brave_api_key, web cfg, venv_base. `web_search` + `web_read` migrés depuis le fast path. Les 5 outils restants (`bash`, `python_executor`, `file_write`, `file_edit`, `notebook_edit`) explicitement marqués HITL inline dans la liste `disabled_tools` du dispatcher. |
| **Phase 4** | ✅ 2026-05-18 | **Full convergence.** Nouveau module `chat::native_wrappers` : (a) `HitlFilesystemGuard<E>` wrappe n'importe quel `ToolExecutor` en ajoutant la classification de risque + l'event `HitlFilesystemRequired` + l'attente du `FsHitlDecision` (5 min timeout). Wrappe `file_write`, `file_edit`, `notebook_edit`, `bash_executor`, `python_executor`. (b) `DynamicAllowlistHttpFetch` reconstruit `HttpFetch` par call avec l'allowlist injectée depuis le host de l'URL — préserve l'UX Chat Libre. (c) `memory_search` reçoit `memory_namespace = apollia:chat:{session_id}` dans le dispatcher cfg. (d) `ask_user` câblé via `pending_user_inputs` passé au dispatcher builder. **Résultat** : `NativeChatToolInvoker.invoke()` collapse en une délégation pure au `fallback_dispatcher`. Plus aucun match arm, aucun champ HITL utilisé. Tous les `invoke_*` (bash/file_write/edit/python/notebook_edit/http_fetch/memory_search/ask_user) marqués `#[allow(dead_code)]` en attendant le cleanup final (suppression physique en follow-up). |

## Vérification

- `cargo test -p apollia-tools dispatcher_invoker` — 3/3 ✅
- `cargo test -p apollia-auth drive_prefs::` — 8/8 ✅
- `cargo test -p apollia-runtime connectors_bridge::` — 5/5 ✅
- `cargo test -p apollia-runtime native_wrappers::` — 3/3 ✅ (Phase 4)
- `cargo test -p apollia-runtime supervisor::tests` — 39/39 ✅
- `cargo test -p apollia-core temporal_context::` — 6/6 ✅
- `cargo test --workspace` — 1342 passed, 0 failed après Phase 4.
- Test manuel Chat Libre attendu :
  - `gcal.create_event` route via le dispatcher (Google executor) avec bearer token résolu via `AuthManager`.
  - `mcp:<server>/<tool>` route via le dispatcher (McpToolExecutor) — audit trail persistant.
  - `file_read/list/glob/grep`, `notebook_read`, `web_search`, `web_read`, `memory_search`, `ask_user`, `permission_rule_*` routent via le dispatcher avec `sandbox_root` + `memory_namespace` + `governance_db_path` + `pending_user_inputs` injectés par `make_invoker`.
  - `file_write`, `file_edit`, `notebook_edit`, `bash_executor`, `python_executor` routent via le dispatcher **wrappés dans `HitlFilesystemGuard`** — l'utilisateur voit le même modal d'approbation qu'auparavant. Sur "Approve" l'executor inner s'exécute, sur "Deny" le call retourne `ExecutionFailed { code: "user_denied" }`.
  - `http_fetch` route via `DynamicAllowlistHttpFetch` — chaque call construit un `HttpFetch` frais avec le host de l'URL injecté dans l'allowlist.

## Références

- `apollia-runtime/src/connectors_bridge.rs` — `GoogleChatToolInvoker` + descripteurs
- `apollia-tools/src/dispatcher_invoker.rs` — adapter
- `apollia-runtime/src/chat/builtin_agent.rs:82-91, 152-160, ~665-672` — `fallback_dispatcher` plumbing
- `apollia-runtime/src/chat/manager.rs:2324-2330` — attache automatique
- ADR-088 (architecture hybride), ADR-090 (Connector trait), ADR-082 (Tool governance)
