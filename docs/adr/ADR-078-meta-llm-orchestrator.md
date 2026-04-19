# ADR-078 — MetaLlmOrchestrator : service de transparence partagé

> Note : la story US-SP42-036 référençait historiquement "ADR-073". Le numéro
> 073 ayant déjà été utilisé pour la signature de code macOS, cette décision
> est formalisée sous ADR-078. Les commentaires de code pointent vers cet ADR.

- **Date** : 2026-04-19
- **Statut** : Accepté
- **Sprint** : 42 — frontend redressement
- **Story** : US-SP42-036

## Contexte

Le frontend souhaite afficher des artefacts de transparence (rationale d'un
appel d'outil, résumé d'un thinking, titre de session, explication d'erreur,
conséquences d'une question AskUser, évaluation de risque, branches
alternatives, vérification d'hallucination). Ces artefacts doivent être
générés à la volée par un LLM mais ne relèvent d'aucun acteur existant
(ORIA, Chat, Pipelines) — c'est une couche horizontale de narration.

Deux options se sont dégagées :

1. **Modèle dédié** : configurer un backend LLM secondaire (petit modèle rapide)
   uniquement pour la narration, indépendant du LLM principal.
2. **Réutilisation du LLM user-configured** : interroger le `LlmRouter` déjà en
   place via un service partagé avec cache et budget.

## Décision

Option **2** retenue : un nouveau service `MetaLlmOrchestrator` dans
`apollia-llm` réutilise le `LlmRouter` configuré par l'utilisateur.

Le service est un acteur Tokio :
- `enum MetaCmd { Run, GetSettings, SetSettings, GetBudget }` ;
- `mpsc::channel` + handle clonable (`MetaOrchestratorHandle`) ;
- cache `LruCache<String, CacheEntry>` taille 512, TTL 15 min, keyed sur
  `(routine, SHA-256(canonical_json(inputs)))` ;
- tracker budget par session : `AtomicU64` dans `Arc<Mutex<BudgetTracker>>`,
  défaut 10 000 tokens/session, event `RuntimeEvent::MetaLlmBudgetExceeded`
  émis une fois ;
- timeout appel LLM : 10 s, fallback `Ok(None)` (l'UI affiche un texte statique) ;
- `MetaLlmSettings { enabled: false, per_routine, session_budget_tokens }` —
  opt-in strict, master toggle + overrides par routine.

10 routines : `GenerateToolCallRationale`, `GenerateThinkingSummary`,
`GenerateSessionSummary`, `GenerateNextSteps`, `GenerateSessionTitle`,
`GenerateErrorExplanation`, `GenerateAskUserConsequences`,
`GenerateAlternativeBranches`, `GenerateRiskAssessment`,
`GenerateHallucinationCheck`. Templates versionnés en `prompts/meta/*.md`
embarqués via `include_str!`.

## Politique de coût

- **Aucun backend LLM dédié** : le coût est déjà engagé pour le LLM principal.
- Budget par défaut : 10 k tokens/session (configurable).
- Objectif cache hit : ≥ 60 % sur sessions répétées grâce à la clé SHA-256
  canonique (les mêmes inputs JSON produisent la même clé indépendamment de
  l'ordre des champs).
- Enveloppe estimée : ~7 k tokens/session si toutes les routines tirent une
  fois — reste sous le budget par défaut.

## Alternative rejetée : modèle LLM dédié

Raisons du rejet :
- Complexifie la config utilisateur : deuxième backend à paramétrer, deuxième
  clé API éventuelle, deuxième quota à surveiller.
- Incohérence stylistique : un petit modèle peut produire des rationales moins
  précises que le modèle principal qui a déjà le contexte complet.
- Coût additionnel : même si le modèle est bon marché, l'utilisateur paye deux
  fois pour la même interaction.
- Pas de gain de latence avéré : le cache LRU absorbe la plupart des redondances,
  et le timeout 10 s + fallback statique garantissent déjà la réactivité UI.

## Conséquences

- **Positives** : zéro nouvelle config, pas de nouveau quota, cohérence de ton,
  toggle opt-in offre un contrôle fin à l'utilisateur, cache évite le re-coût.
- **Négatives** : budget du LLM principal partagé entre travail utile et
  narration méta — d'où le budget par session + l'événement
  `MetaLlmBudgetExceeded` qui permet à l'UI de masquer proactivement les
  artefacts restants.
- **Suite** : US-SP42-037 (intégration `Chat`), US-SP42-038 (intégration
  `ORIA`), US-SP42-041 (panneau Settings), US-SP42-045 (widget budget),
  US-SP42-048 (i18n templates).

## Fichiers introduits

- `crates/apollia-llm/src/meta_orchestrator.rs` — acteur, cache, budget, tests.
- `crates/apollia-llm/prompts/meta/*.md` — 10 templates versionnés.
- `crates/apollia-core/src/events.rs` — variant `MetaLlmBudgetExceeded`.
- `crates/apollia-llm/Cargo.toml` — dépendances `lru` et `sha2`.
- `Cargo.toml` — dépendance `lru` ajoutée au workspace.
