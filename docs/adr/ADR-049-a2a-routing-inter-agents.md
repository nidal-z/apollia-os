# ADR-049 - Routing A2A inter-agents : discovery + invocation

**Date :** 2026-04-01
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Sprint :** 30 (Pré-implémentation)

---

## Contexte

Apollia OS dispose, depuis Sprint 29, d'une infrastructure Worker Agent (ADR-048) : des agents Python
dotés de `supports_a2a: True` et d'une liste de `skills` dans leur manifest. Cette déclaration est
intentionnelle mais incomplète - le routing inter-agents n'est pas encore implémenté.

Le sprint actuel (Sprint 30) doit permettre à un **Director Agent** d'invoquer un Worker Agent par
compétence (`skill_id`) sans connaître son nom d'agent, son chemin Python, ni son état d'exécution.
Quatre questions architecturales restaient ouvertes :

1. **Résolution des conflits de skills** - deux agents peuvent-ils déclarer le même `skill_id` ?
2. **Mode d'invocation** - synchrone (attente du résultat) ou asynchrone (feu et oubli) ?
3. **Format du résultat** - comment le Director Agent reçoit-il la réponse du Worker ?
4. **Trust model** - le Worker Agent a-t-il accès à la mémoire du Director ?
5. **Récursivité A2A** - un Worker Agent peut-il déléguer à son tour ?

Avant d'implémenter les composants Rust (STORY-394 à STORY-400), ces cinq questions doivent être
formalisées pour éviter des divergences d'implémentation entre stories.

---

## Décision

### 1. Conflit de skills - erreur au register, premier enregistré gagne

La fonction `resolve_skill(entries, skill_id)` parcourt les `AgentEntry` avec `supports_a2a = true`
et en état `Active | Degraded`. Si le `skill_id` est déclaré par plusieurs agents simultanément,
la résolution retourne `A2aError::AmbiguousSkill { skill_id, agents: Vec<String> }`. Le
runtime n'applique pas de round-robin ni de priorité silencieuse - l'ambiguïté est une erreur
explicite que le Director Agent doit traiter.

Au moment de l'enregistrement d'un agent (`AgentRegistry::register`), le premier agent enregistré
avec un `skill_id` donné est accepté sans condition. Tout agent ultérieur déclarant le même
`skill_id` est accepté (l'enregistrement réussit) mais la résolution sera en erreur tant que les
deux agents sont actifs simultanément. Ce comportement est délibéré : il est préférable de détecter
le conflit à la résolution (avec contexte complet) plutôt qu'à l'enregistrement (sans savoir si
l'agent précédent sera encore actif).

### 2. Invocation synchrone pour V1 - timeout configurable (défaut 120 s)

La délégation A2A est synchrone pour V1 : `ctx.delegate(skill_id, payload)` bloque jusqu'au retour
du résultat ou à l'expiration du timeout. Le timeout par défaut est **120 secondes**, paramétrable
via un troisième argument `timeout_secs: u64`.

L'invocation asynchrone (fire-and-forget + callback) est rejetée pour V1 : elle nécessite un
mécanisme de corrélation et une API de rappel que ni le runtime ni le SDK Python ne gèrent
aujourd'hui. La synchronicité simplifie le modèle mental ("appel de fonction") et couvre 95 % des
cas d'usage attendus (Excel, CSV, requête BDD). Le mode asynchrone est reporté à V2.

Techniquement, `A2aDelegateFn` est défini comme :

```rust
pub type A2aDelegateFn = Arc<
    dyn Fn(String, serde_json::Value, u64)
        -> Pin<Box<dyn Future<Output = Result<A2aDelegateResult, A2aError>> + Send>>
        + Send + Sync,
>;
```

Ce type alias contourne la contrainte PyO3 (`#[pyclass]` sans paramètre générique).
`make_delegate_fn(registry, router, event_bus)` capture les handles et retourne une closure conforme.

L'anti-race condition est garantie par l'ordre : `delegate_inner` souscrit à l'`EventBus` **avant**
de soumettre la tâche via le `TaskRouter`. En cas de `RecvError::Lagged`, le résultat est récupéré
via `router.get_output(task_id)`.

### 3. Format du résultat - `A2aDelegateResult` aligné sur `AIPResult`

Le résultat d'une délégation est encapsulé dans `A2aDelegateResult`, un wrapper cohérent avec le
format de sortie standard des agents (`AIPResult`) :

```rust
pub struct A2aDelegateResult {
    /// Identifiant de la tâche exécutée par le Worker Agent.
    pub task_id: TaskId,
    /// Sortie JSON produite par le Worker Agent, alignée sur AIPResult.
    pub output: serde_json::Value,
}
```

Le Director Agent reçoit le champ `output` tel que retourné par le Worker (structure libre JSON).
Aucun wrapping supplémentaire n'est ajouté - c'est le Worker Agent qui est responsable du format
de sa réponse. Cette décision conserve la flexibilité du duck typing AIP (ADR-003).

### 4. Trust model - user memory en lecture globale, namespace propre en écriture

Conformément au Principe #6 (ADR-007), le runtime n'injecte jamais automatiquement de contexte
mémoriel. Ce principe s'applique aussi à la délégation A2A.

Pour V1 :
- Le Worker Agent invoqué **n'a pas accès** à la mémoire du Director Agent par défaut. Il reçoit
  uniquement le `payload` transmis explicitement par le Director.
- Si le Director veut partager du contexte mémoriel, il doit le sérialiser dans le `payload`
  avant d'appeler `ctx.delegate()`.
- Le Worker Agent écrit sa mémoire dans son propre namespace (son `agent_id`) - jamais dans le
  namespace du Director.

Ce modèle est le plus simple et le plus sûr pour V1. Une API de partage de contexte mémoriel entre
agents (opt-in, explicite) est envisagée pour V2 après observation des patterns d'usage réels.

### 5. Profondeur de récursivité - non limitée en V1, garde-fous Sprint 32

Un Worker Agent invoqué via A2A peut lui-même appeler `ctx.delegate()` - la récursivité est
autorisée en V1 sans limite de profondeur. Le runtime ne maintient pas de compteur de profondeur
inter-agents.

Ce choix est délibéré pour ne pas sur-contraindre les cas d'usage (pipelines d'agents en chaîne).
Le risque de récursion infinie est réel mais accepté pour V1 :
- La protection principale reste le `StepBudget` de chaque agent (Principe #7) - une boucle
  infinie épuise son budget et échoue proprement.
- Le timeout A2A (décision #2) cassera les délégations bloquées après 120 s.
- Des garde-fous de profondeur explicites (`max_delegation_depth`) sont planifiés pour Sprint 32.

### 6. Surface API REST

Deux routes sont ajoutées au serveur API (axum) :

- `GET  /api/v1/a2a/agents`   - liste les agents A2A actifs avec leurs skills déclarés
- `POST /api/v1/a2a/delegate` - délègue une tâche par `skill_id` avec un payload JSON

### 7. Filtre CLI

`apollia-os agent list --supports-a2a` interroge `/api/v1/a2a/agents` et affiche les agents A2A
actifs avec leurs skills. L'implémentation est dans STORY-397.

---

## Alternatives écartées

| Option | Raison du rejet |
|---|---|
| Résolution par nom d'agent | Couplage fort Director → Worker - viole l'encapsulation |
| Routing via EventBus seul | Pas de mécanisme request/response natif sur broadcast |
| Génériciser `RuntimeContext<B>` | Impossible avec `#[pyclass]` (PyO3 n'accepte pas les paramètres génériques) |
| Résolution statique au démarrage | Ne supporte pas l'enregistrement d'agents à chaud |
| Invocation asynchrone pour V1 | Complexité corrélation/callback injustifiée pour les cas d'usage V1 |
| Erreur bloquante à l'enregistrement (conflit skill) | Empêche le redémarrage partiel d'agents - détection à la résolution préférable |
| Injection automatique de mémoire vers le Worker | Viole Principe #6 (ADR-007), opacité comportementale |
| Limite de récursivité fixe en V1 | Sur-contrainte les pipelines multi-agents légitimes |

---

## Conséquences

**Positives :**
- Un Director Agent peut déléguer à un Worker via `ctx.delegate(skill_id, payload)` sans couplage direct.
- Les skills dupliqués entre agents actifs déclenchent `A2aError::AmbiguousSkill` - l'invariante est enforced au runtime avec message d'erreur explicite.
- Le modèle synchrone simplifie le raisonnement pour les agents Python : c'est un appel de fonction.
- Le trust model explicite évite toute fuite de contexte mémoriel involontaire.
- Le desktop backend (`apollia-desktop`) ne participe pas au routing A2A (`a2a_delegate = None`) - pas d'impact sur l'implémentation desktop.

**Négatives / Compromis :**
- Invocation synchrone : une délégation longue bloque le step en cours du Director Agent jusqu'au timeout.
- Pas de partage de mémoire automatique : le Director doit sérialiser explicitement le contexte partagé dans le payload.
- Récursivité illimitée en V1 : risque théorique de boucle infinie si `StepBudget` et timeout ne suffisent pas.

**Neutres / À surveiller :**
- Patterns d'usage réels de la récursivité A2A (pour calibrer les garde-fous Sprint 32).
- Performance du `resolve_skill` sous charge (N agents actifs × M skills chacun) - O(N×M) acceptable pour V1.

---

## Principes architecturaux impactés

- **Principe #1 - Local-first** : Toute invocation A2A reste intra-processus. Aucune donnée ne sort de la machine. Conforme.
- **Principe #5 - Un acteur, une responsabilité** : `SkillIndex` est intégré dans `AgentRegistry` (pas un acteur séparé). `A2AInvoker` est un composant synchrone, pas un acteur Tokio. Conforme.
- **Principe #6 - Mémoire à initiative de l'agent** : Le Worker ne reçoit que le payload explicitement transmis. Aucune injection automatique de contexte mémoriel. Renforcé (extension du principe aux délégations inter-agents).
- **Principe #7 - Garde-fous non-négociables** : Le timeout A2A (120 s par défaut) est appliqué par le runtime et non contournable depuis le code Python. Le `StepBudget` de chaque agent reste actif. Conforme.

---

## Liens

- ADR fondateur : [ADR-048 - Worker Agents](ADR-048-worker-agents-expertise-domaine.md)
- Mémoire à initiative de l'agent : [ADR-007 - Mémoire à l'initiative de l'agent](ADR-007-memoire-initiative-agent.md)
- Duck typing AIP : [ADR-003 - Duck typing AIP](ADR-003-duck-typing-aip.md)
- Story A2A fondatrice : STORY-392 (Sprint 29 - spec A2A inter-agents)
- Document d'idéation : `docs/internal/strategy/capabilities-architecture-ideation.md` §4 - Routing A2A
- Stories d'implémentation : STORY-394 (AgentDiscovery), STORY-395 (A2AInvoker), STORY-396 (Trust Model), STORY-397 (CLI), STORY-400 (Tests intégration)
