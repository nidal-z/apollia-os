# ADR-054 — Consolidation mémoire épisodique : report justifié post-v1

**Date :** 2026-04-03
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Sprint :** 34 — Beta Hardening

---

## Contexte

La mémoire épisodique enregistre chaque événement de la vie d'un agent (tâches exécutées, décisions,
résultats) avec un TTL optionnel. Depuis le Sprint 3, elle croît sans mécanisme de consolidation :
les épisodes similaires s'accumulent, les patterns récurrents ne sont pas résumés, et la base
SQLite grossit linéairement avec le temps.

La littérature sur les agents autonomes (MemGPT, Letta, etc.) propose des mécanismes de consolidation
automatique — résumé LLM des épisodes anciens, extraction de faits vers la mémoire sémantique,
fusion des épisodes redondants. Ces approches sont techniquement intéressantes mais introduisent
plusieurs risques pour Apollia OS en beta :

1. **Coût LLM non maîtrisé** : la consolidation automatique implique des appels LLM non initiés
   par l'agent. Sur un modèle local lent, la consolidation peut bloquer le runtime ; sur un backend
   cloud, elle génère des coûts invisibles pour l'utilisateur.

2. **Comportement imprévisible** : un agent qui retrouve ses souvenirs consolidés peut avoir un
   comportement différent d'un agent sans consolidation. Difficile à débugger, impossible à reproduire.

3. **Risque de perte de données** : la consolidation par résumé LLM est destructive — les épisodes
   originaux sont perdus au profit d'un résumé. Une erreur de consolidation (hallucination, troncature)
   ne peut pas être annulée.

4. **Violation du Principe #6** : "Mémoire à initiative de l'agent" — la consolidation automatique
   injecte de la mémoire sans que l'agent en ait fait la demande.

STORY-448 a investigué ce sujet pendant le Sprint 34 et a conclu que la consolidation automatique
ne doit pas être implémentée avant v1.

---

## Décision

### 1. Pas de consolidation automatique pour la beta

La mémoire épisodique reste une append-only log avec TTL explicite. Aucun processus background
ne consolide, résume, ou fusionne des épisodes sans action explicite de l'agent ou de l'opérateur.

### 2. Mécanisme de purge manuelle fourni (existe déjà)

La purge des épisodes expirés (TTL atteint) au démarrage reste le seul mécanisme automatique :

```toml
[memory]
purge_on_startup = true   # défaut
```

La CLI `apollia-os memory purge <namespace>` permet une purge manuelle des épisodes expirés.
La CLI `apollia-os memory clear <namespace>` permet de vider entièrement un namespace.

### 3. Troncature de l'output mémorisé

Pour limiter la croissance des épisodes sans consolidation, la taille de la sortie d'un step
mémorisée dans la mémoire épisodique est bornée à `STEP_MEMORY_OUTPUT_MAX_CHARS = 200` caractères.
Au-delà, le contenu est tronqué avec un suffixe `[truncated]`. Cette constante est configurable
dans `apollia.toml` (`memory.step_output_max_chars`).

### 4. Consolidation opt-in post-v1

La consolidation automatique est différée à post-v1. Le design retenu respecte le Principe #6 : c'est l'agent qui décide quand et comment consolider, pas le runtime.

**Activation par manifest (opt-in par agent) :**

```json
{
  "memory_consolidation": {
    "enabled": true,
    "interval": "24h",
    "min_episodes": 100
  }
}
```

- `enabled` : désactivé par défaut — l'agent déclare explicitement vouloir la consolidation.
- `interval` : fréquence minimale entre deux consolidations (ex. `"6h"`, `"24h"`, `"7d"`).
- `min_episodes` : seuil de déclenchement — aucune consolidation si la base contient moins d'épisodes.

**API runtime (post-v1) :**

```python
# L'agent déclenche la consolidation à l'initiative de sa propre logique
async def run(self, task: AIPTask, ctx: RuntimeContext) -> AIPResult:
    stats = await ctx.memory.stats()
    if stats.episodic_count > 500:
        consolidated = await ctx.memory.consolidate(
            older_than=timedelta(days=30),
            target_namespace="crm-dupont",
        )
        ctx.log.info("memory_consolidated", episodes_merged=consolidated)
```

Le runtime expose `ctx.memory.consolidate()` — l'agent contrôle quand consolider, avec quels paramètres, et peut inspecter le résultat. Aucun processus background ne se déclenche sans cet appel explicite.

Les conditions d'implémentation requises avant livraison :
- Préservation des épisodes originaux pendant une période de rétention configurable avant suppression
- Log d'audit des épisodes consolidés et des prompts LLM utilisés
- ADR dédiée avant implémentation (cette ADR est le design préliminaire)

---

## Alternatives considérées

| Option | Raison du rejet |
|---|---|
| **Consolidation automatique par LLM (type MemGPT)** | Coût LLM non maîtrisé, comportement imprévisible, risque de perte de données. Viole Principe #6 (mémoire à initiative de l'agent). |
| **Consolidation par règles (sans LLM)** | Peut être implémenté sans coût LLM, mais "règles" = heuristiques fragiles qui deviennent vite des cas particuliers. Complexité sans bénéfice clair pour la beta. |
| **Limit dur sur la taille de la base** | Reject FIFO des épisodes anciens si la base dépasse N Mo. Trop brutal — les épisodes importants peuvent être les plus anciens (configuration initiale, décisions critiques). |
| **Consolidation manuelle par l'agent** | L'agent peut déjà appeler `ctx.memory.record()` pour synthétiser. C'est le Principe #6 appliqué — à documenter comme pattern recommandé, pas à automatiser. |

---

## Conséquences

**Positives :**
- Comportement prévisible et débuggable — la mémoire ne change pas sans action explicite.
- Zéro coût LLM caché — aucun appel LLM non initié par l'agent.
- Données souveraines — les épisodes originaux ne sont jamais détruits automatiquement.
- Troncature `STEP_MEMORY_OUTPUT_MAX_CHARS = 200` limite la croissance sans heuristiques complexes.

**Négatives / Compromis :**
- Dette technique acceptée : la base épisodique croît linéairement pour les agents long-running.
  Pour un agent CRM actif 1 an, la base peut atteindre plusieurs centaines de Mo.
- Pas de "mémoire intelligente" pour la beta — les agents doivent gérer explicitement leur historique.
  C'est un compromis voulu pour la cible PME simple.

**Neutres / À surveiller :**
- Mesurer la taille des bases épisodiques des agents bundled après 30 jours d'usage en beta.
  Si > 50 Mo, reconsidérer la priorité de la consolidation post-v1.
- Documenter le pattern "consolidation à initiative agent" dans le wiki — `ctx.memory.record()`
  peut être utilisé pour synthétiser des épisodes passés avant de les purger.

---

## Principes architecturaux impactés

- **Principe #6 — Mémoire à initiative de l'agent** : Aucune injection automatique de contexte
  mémoriel, aucune consolidation automatique. La mémoire évolue uniquement sur appel explicite
  de `ctx.memory.*`. Renforcé.
- **Principe #7 — Garde-fous non-négociables** : La troncature `STEP_MEMORY_OUTPUT_MAX_CHARS`
  borne la taille des épisodes sans action de l'agent. C'est le seul garde-fou automatique
  acceptable — il ne modifie pas la sémantique des données, seulement leur volume. Conforme.

---

## Liens

- Story d'implémentation : STORY-448 (Sprint 34)
- Constante documentée dans : `docs/wiki/Briques-Memory-Engine.md` §7
- Config exposée dans : `crates/apollia-core/src/config.rs` — section `[memory]`
- ADR Memory fondateur : [ADR-007](ADR-007-memoire-initiative-agent.md)
- ADR observabilité timeline : [ADR-026](ADR-026-observabilite-complete-persistance-timeline-troncature.md)
