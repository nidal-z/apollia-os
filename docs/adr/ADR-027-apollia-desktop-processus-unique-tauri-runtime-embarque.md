# ADR-027 — apollia-desktop : processus unique Tauri + runtime embarqué

**Date :** 2026-03-13
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Sprint :** 14

---

## Contexte

Apollia OS a atteint la maturité fonctionnelle nécessaire pour être distribué à des
utilisateurs non-techniques : 13 sprints livrés, CLI complète, agents Python, HITL,
pipelines, observabilité. Le CLI reste l'interface principale pour les développeurs,
mais un utilisateur PME (cible prioritaire) ne devrait pas avoir à ouvrir un terminal.

Le Sprint 14 introduit `apollia-desktop`, une application desktop native qui permet
de lancer Apollia OS en double-cliquant sur une icône. La question architecturale
centrale est : **comment le frontend Tauri communique-t-il avec le runtime Rust ?**

### Contraintes

- **Principe #1 — Local-first** : un seul binaire auto-suffisant, distribuable comme
  `.dmg` (macOS) ou `.AppImage` (Linux). Pas de serveur externe.
- **Principe #2 — Zéro dépendance externe** : l'utilisateur ne doit pas installer
  de prérequis (Node.js, Python runtime, etc.) au-delà du binaire lui-même.
- **Principe #4 — Fail fast** : si le runtime ne démarre pas, l'application doit
  afficher une erreur immédiate — pas une fenêtre vide qui charge indéfiniment.
- **Principe #8 — CLI humaine, API machine** : le CLI existant (`apollia-os`) doit
  continuer à fonctionner en parallèle de l'application desktop, via le Unix socket
  existant `/tmp/apollia.sock`.
- Le runtime utilise déjà des handles Tokio (`AgentRegistryHandle`, `TaskRouterHandle`,
  `EventBusSender`) qui sont `Clone + Send + Sync` — disponibles pour un passage
  in-process sans sérialisation.

---

## Décision

Nous adoptons un **processus unique** : `apollia-desktop` est un binaire Tauri v2 qui
démarre le runtime Apollia OS en interne via `init_embedded()`, puis ouvre la WebView.

### Architecture de communication

Deux canaux complémentaires, sans duplication :

1. **Mutations ponctuelles** (start_agent, submit_task, resume_task) →
   commandes Tauri `#[tauri::command]` wrappant directement les handles Tokio
   du `RuntimeHandle`. Appel in-process, zéro sérialisation HTTP.

2. **Flux temps réel** (état agents, tâches, LLM stats, triggers) →
   SSE EventBus existant à `localhost:7771/api/v1/dashboard/stream`.
   Le frontend Svelte se connecte au même endpoint SSE que le dashboard HTMX
   existant. Pas de canal Tauri events en doublon.

### Séquence de démarrage

```
main() Tauri
  → init_embedded()
    → Spawn Tokio runtime thread
    → Supervisor.start() (séquence existante : EventBus → AgentRegistry → ... → APIServer)
    → Wait AllReady sur EventBus
    → Return RuntimeHandle
  → tauri::Builder::default()
    → .manage(RuntimeHandle)
    → .invoke_handler(generate_handler![...])
    → .run()
    → WebView ouvre sur localhost:5173 (dev) ou assets embarqués (prod)
```

### RuntimeHandle

```rust
pub struct RuntimeHandle {
    pub event_sender: EventBusSender,
    pub registry_handle: AgentRegistryHandle,
    pub tool_registry_handle: ToolRegistryHandle,
    pub router_handle: TaskRouterHandle<DynBackend>,
    pub api_handle: APIServerHandle,
    pub llm_router: Option<Arc<LlmRouter>>,
    pub trigger_engine: TriggerEngineHandle,
    pub pipeline_engine: Option<PipelineEngineHandle>,
    pub audit_trail: Option<AuditTrailHandle>,
    pub task_repository: Option<Arc<TaskRepository>>,
    pub pending_approvals: Option<Arc<PendingApprovals>>,
    pub notification_engine: Option<NotificationEngineHandle>,
    pub api_port: u16,
}
```

Le `Supervisor` existant ne change pas. `init_embedded()` est simplement une nouvelle
façon de le démarrer, alternative à la boucle CLI de `apollia-os start`.

---

## Alternatives considérées

### Option A — Deux processus séparés (rejetée)

Le runtime (`apollia-os start`) tourne en arrière-plan, Tauri communique uniquement via
HTTP sur `localhost:7771`.

**Pour :**
- Séparation des préoccupations nette
- Le runtime peut tourner sans le frontend (déjà le cas avec le CLI)
- Pas de risque d'interférence linker Tauri/PyO3

**Contre :**
- Synchronisation du démarrage complexe : comment l'app sait-elle que le runtime est
  prêt ? Polling HTTP ? Socket file watch ? Chaque solution ajoute de la complexité.
- Deux binaires à distribuer et coordonner dans le `.dmg` / `.AppImage`.
- Communication forcée via HTTP alors que les handles Tokio sont disponibles en mémoire
  — sérialisation/désérialisation inutile pour les mutations ponctuelles.
- L'utilisateur doit gérer deux processus (vérifier que le runtime tourne, le relancer
  si crash, etc.).

### Option B — WebView navigateur sans Tauri (rejetée)

L'application expose `localhost:7771/dashboard` (HTMX existant du Sprint 9) et
l'utilisateur ouvre un navigateur.

**Pour :**
- Zéro dépendance Tauri, build plus simple
- Le dashboard HTMX existe déjà (Sprint 9, STORY-077)

**Contre :**
- Friction inacceptable pour utilisateur non-technique : ouvrir navigateur, taper URL,
  bookmarker. Personne ne fait ça pour un outil desktop.
- Pas de packaging natif (`.dmg`, `.AppImage`) — pas de double-clic.
- Pas de tray icon, pas de notifications natives, pas de file picker natif.
- HTMX insuffisant pour les interactions complexes du Sprint 14 : HITL real-time
  (compteur en direct), timeline interactive, file picker natif.

### Option retenue — Processus unique Tauri + runtime embarqué

**Pour :**
- Un seul binaire distribué — expérience utilisateur optimale
- Communication in-process via handles Tokio — zéro overhead sérialisation
- Le CLI continue de fonctionner via le Unix socket existant
- Fail fast : si `init_embedded()` échoue, l'erreur est immédiate
- Tauri v2 fournit tray icon, notifications natives, file picker, packaging

**Compromis acceptés :**
- Le binaire est plus gros (~30-50MB avec Tauri + WebView engine)
- Risque de conflit linker PyO3 + Tauri sur macOS (à valider dans STORY-135)
- Le runtime meurt si l'application desktop est fermée (acceptable : même
  comportement que `apollia-os start` + Ctrl+C)

---

## Conséquences

**Positives :**
- Distribution simplifiée : un seul `.dmg` / `.AppImage` à télécharger
- Expérience utilisateur native : double-clic → fenêtre → agents visibles
- Les handles Tokio existants sont réutilisés sans modification architecturale
- Le CLI reste fonctionnel en parallèle via le Unix socket

**Négatives / Compromis :**
- Le binaire desktop inclut toute la stack (Tauri + WebView + runtime Rust + PyO3)
  → taille ~50MB estimée
- Fermer la fenêtre arrête le runtime — pas de mode "tray only" pour l'instant
  (prévu Sprint 15+)
- Conflit potentiel PyO3 linker sur macOS — à diagnostiquer dès STORY-135

**Neutres / À surveiller :**
- La taille du binaire devra être surveillée à chaque sprint
- Le mode "headless" (runtime sans fenêtre) reste disponible via `apollia-os start`
- L'auto-update (Tauri updater) n'est pas dans le scope du Sprint 14

---

## Principes architecturaux impactés

- **Principe #1 — Local-first** : renforcé — binaire unique auto-suffisant,
  zéro donnée transmise à l'extérieur
- **Principe #2 — Zéro dépendance externe** : respecté — Tauri WebView utilise
  le moteur natif de l'OS (WebKit sur macOS, WebKitGTK sur Linux)
- **Principe #4 — Fail fast** : respecté — `init_embedded()` retourne une erreur
  immédiate si le runtime ne démarre pas
- **Principe #8 — CLI humaine, API machine** : étendu — le desktop devient une
  troisième interface (CLI / API REST / Desktop), toutes sur le même runtime

---

## Liens

- Stories associées : STORY-135 à STORY-142
- ADR précédent lié : ADR-017 (hyper-util Unix socket — le socket reste actif
  quand le desktop tourne)
- ADR précédent lié : ADR-018 (CLI bootstrap sans Supervisor — `init_embedded()`
  est l'équivalent pour le desktop)
