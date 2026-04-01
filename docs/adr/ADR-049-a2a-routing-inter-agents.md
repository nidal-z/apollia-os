# ADR-049 — Routing A2A inter-agents : résolution par skill_id

**Date :** 2026-04-01  
**Statut :** Accepté  
**Décideur :** Nidal  

---

## Contexte

Apollia doit permettre à un Director Agent d'invoquer un Worker Agent par compétence (`skill_id`)
sans connaître son nom d'agent, son chemin Python, ni son état d'exécution. La résolution doit être
dynamique (basée sur les agents actifs au runtime), sûre (erreur structurée sur skill absent ou
ambigu), et compatible avec le principe #1 (local-first) et le principe #5 (un acteur, une
responsabilité).

---

## Décision

### 1. Résolution par `skill_id` depuis l'`AgentRegistry`

La fonction `resolve_skill(entries, skill_id)` parcourt les `AgentEntry` avec `supports_a2a = true`
et en état `Active | Degraded`. Elle retourne une erreur structurée `A2aError` si le skill est
absent ou déclaré par plusieurs agents (duplicata interdit).

### 2. Type-erasure pour l'injection dans `RuntimeContext`

`RuntimeContext` est un `#[pyclass]` sans paramètre générique. Pour injecter la logique de
délégation (qui dépend du backend générique `B: ExecutionBackend`), on utilise le type alias :

```rust
pub type A2aDelegateFn = Arc<
    dyn Fn(String, serde_json::Value, u64)
        -> Pin<Box<dyn Future<Output = Result<A2aDelegateResult, A2aError>> + Send>>
        + Send + Sync,
>;
```

`make_delegate_fn(registry, router, event_bus)` capture les handles et retourne une closure conforme
à ce type. L'agent Python appelle `ctx.delegate(skill_id, payload, timeout_secs)`.

### 3. Souscription avant soumission (anti-race condition)

`delegate_inner` souscrit à l'`EventBus` **avant** de soumettre la tâche via le `TaskRouter`. Cela
garantit que l'événement `TaskCompleted` n'est pas manqué même si l'exécution est instantanée.
En cas de `RecvError::Lagged`, on repasse par `router.get_output(task_id)`.

### 4. OnceLock dans `ProductionBackendFactory`

Le `TaskRouterHandle<DynBackend>` et l'`AgentRegistryHandle` sont créés à l'intérieur de
`supervisor.start()`. La factory est construite avant. On utilise le même pattern `OnceLock`
déjà présent pour `EventBusSender` : deux nouveaux champs `registry` et `router` sont ajoutés
et populés immédiatement après le retour de `supervisor.start()`.

### 5. Surface API REST

- `GET  /api/v1/a2a/agents`   — liste les agents A2A actifs avec leurs skills
- `POST /api/v1/a2a/delegate` — délègue une tâche par skill ID

### 6. Filtre CLI

`apollia-os agent list --supports-a2a` interroge `/api/v1/a2a/agents` et affiche les agents
A2A actifs avec leurs skills déclarés.

---

## Alternatives écartées

| Option | Raison du rejet |
|---|---|
| Résolution par nom d'agent | Couplage fort Director → Worker — viole l'encapsulation |
| Routing via EventBus seul | Pas de mécanisme request/response natif sur broadcast |
| Génériciser `RuntimeContext<B>` | Impossible avec `#[pyclass]` (PyO3 n'accepte pas les paramètres génériques) |
| Résolution statique au démarrage | Ne supporte pas l'enregistrement d'agents à chaud |

---

## Conséquences

- Un agent Director peut déléguer à un Worker via `ctx.delegate(skill_id, payload)` sans couplage direct.
- Les skills dupliqués entre agents actifs déclenchent une erreur `A2aError::AmbiguousSkill` — la
  déclaration unique par skill est une invariante enforced au runtime.
- Le desktop backend (`apollia-desktop`) ne participe pas au routing A2A (`a2a_delegate = None`).
- Les tests unitaires de `a2a::mod` couvrent : résolution trouvée, skill absent (avec liste
  disponible), skill ambigu, exclusion des agents non-A2A, exclusion des agents stoppés, acceptation
  des agents dégradés, structure des réponses d'erreur.
