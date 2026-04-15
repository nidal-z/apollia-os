# ADR-071 — ContextBootstrap : convention de bootstrapping de contexte

**Date :** 2026-04-15
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Sprint :** Pré-implémentation (cible : Sprint 40, STORY-511 → STORY-514)

---

## Contexte

Sprint 39 a livré quatre assistants opérationnels. Chacun implémente un pattern mémoire
ad-hoc pour charger et persister les règles workspace en début de session :

```python
# Pattern répété dans spec-assistant, dev-assistant, review-assistant
rules = await ctx.memory.recall("project_rules")
if rules:
    inject_into_prompt(rules)
else:
    for path in _RULE_FILES:  # ["APOLLIA.md", ".apollia/rules.md", ...]
        content = await ctx.tools.call("file_read", {"path": path})
        if content: accumulated += content
    await ctx.memory.remember("project_rules", accumulated, confidence=0.9)
```

Ce pattern présente trois lacunes concrètes :

1. **Aucune détection de péremption** — si `APOLLIA.md` change entre deux sessions,
   l'agent continue à utiliser les anciennes règles cachées sans jamais les invalider.
2. **Duplication de code** — `load_project_rules()` est copié-collé dans trois agents
   avec des variantes mineures. Un bug dans la logique doit être corrigé trois fois.
3. **Contexte superficiel** — le bootstrap persiste uniquement le texte brut d'APOLLIA.md.
   L'architecture du projet, les patterns récurrents, les fichiers clés — tout est re-découvert
   à chaque session au prix de tokens supplémentaires.

Par ailleurs, `ctx.workspace` expose déjà `APOLLIA.md` parsé par le runtime, injecté
gratuitement à chaque session. Les agents Sprint 39 l'ignorent et re-lisent le fichier
via `file_read`, payant un coût inutile à chaque tour.

### Distinction conceptuelle manquante

Il manque une distinction claire entre deux types de contexte :

| | `ctx.workspace` | Bootstrapping ad-hoc actuel |
|---|---|---|
| Portée | Session courante (éphémère) | Persisté mais sans protocole |
| Contenu | APOLLIA.md parsé par le runtime | Texte brut uniquement |
| Fraîcheur | Toujours à jour | Jamais re-validé |
| Coût | Zéro | Payé à chaque session si cache miss |

---

## Décision

Nous adoptons **`ContextBootstrap`** comme convention officielle de bootstrapping de contexte
agent dans Apollia OS.

`ContextBootstrap` est un protocole Python abstrait distribué dans le SDK (`sdk/apollia/bootstrap.py`,
version 0.2.0+). Il définit un contrat minimal avec **2 méthodes abstraites** et
**4 méthodes d'infrastructure** par défaut.

### Contrat minimal

Un développeur externe implémente exactement **2 méthodes** :

```python
from apollia.bootstrap import ContextBootstrap

class MyBootstrap(ContextBootstrap):

    async def is_stale(self, ctx) -> bool:
        """Le snapshot existant est-il périmé ?
        Propre à chaque domaine. En cas de doute, retourner True."""
        ...

    async def run_bootstrap(self, ctx) -> dict:
        """Explorer le domaine, construire un snapshot, appeler self.persist().
        Doit être idempotent."""
        ...
```

### Infrastructure héritée (4 méthodes, override rare)

| Méthode | Comportement par défaut |
|---|---|
| `needs_bootstrap(ctx)` | Lit `bootstrap.status` → si None/missing/partial → True ; si complete → délègue à `is_stale()` |
| `load_snapshot(ctx)` | Lit `bootstrap.snapshot` → `json.loads()` ou None |
| `load_meta(ctx)` | Lit `bootstrap.meta` → `json.loads()` ou None |
| `persist(ctx, snapshot, *, staleness_marker, ...)` | Écrit snapshot + meta + status. Refuse de downgrader `complete` → `partial`. |

### Convention de clés mémoire

```
bootstrap.snapshot    # Le snapshot complet (JSON sérialisé)
bootstrap.meta        # {"version": int, "created_at": str, "staleness_marker": str}
bootstrap.status      # "complete" | "partial" | "missing"
```

Ces clés sont écrites dans le namespace effectif de l'agent (donc déjà isolées par projet
via ADR-070).

### Invariants

1. **Idempotence** : `run_bootstrap()` appelé N fois produit le même état mémoire final.
2. **Non-destructif** : `persist("partial")` ne remplace jamais un `"complete"` existant.
3. **Opt-in** : le protocole est un pattern Python SDK, pas un contrat AIP. Le runtime Rust
   ne connaît pas `ContextBootstrap`.
4. **Principe #6** : le bootstrap est à l'initiative de l'agent, jamais injecté par le runtime.
5. **Graceful degradation** : si `ctx.memory is None`, `needs_bootstrap()` retourne True,
   `load_snapshot()` retourne None — l'agent fonctionne en mode éphémère sans erreur.

### Pattern d'intégration dans `run()`

```python
class DevAssistant:
    def __init__(self):
        self._bootstrap = DevContextBootstrap()

    async def run(self, task, ctx):
        input_text, history = _extract_task_input(task)
        is_first_turn = not history

        # Bootstrap : 3 lignes
        if is_first_turn and await self._bootstrap.needs_bootstrap(ctx):
            await self._bootstrap.run_bootstrap(ctx)

        snapshot = await self._bootstrap.load_snapshot(ctx)

        # Logique métier normale
        rules = snapshot.get("rules_raw", "") if snapshot else ctx.workspace.rules or ""
        ...
```

### Distinction workspace / bootstrap

| | `ctx.workspace` | `ContextBootstrap` |
|---|---|---|
| **Portée** | Session courante (éphémère) | Cross-session (persistant SQLite) |
| **Contenu** | APOLLIA.md parsé par le runtime | Snapshot enrichi : architecture, patterns, deps, fichiers clés |
| **Coût** | Zéro (injecté par le runtime) | Payé une fois, réutilisé indéfiniment |
| **Fraîcheur** | Toujours à jour | Validé par staleness marker (git hash, timestamp) |
| **Qui remplit** | Le runtime Rust | L'agent lui-même via `run_bootstrap()` |

**Règle d'usage :** utiliser `ctx.workspace.rules` pour les règles de la session courante
(toujours fraîches, zéro coût). Utiliser `ContextBootstrap` pour le contexte enrichi
persistant (architecture, patterns) qui ne change pas à chaque session.

### Variantes de staleness marker par domaine

| Domaine | Staleness marker | Logique `is_stale()` |
|---|---|---|
| Agents dev (pipeline Apollia) | Hash du dernier commit git (`git rev-parse HEAD`) | `current_hash != meta["staleness_marker"]` |
| Agents document | Timestamp max des fichiers documents (`find -printf '%T@'`) | `latest_mtime > float(meta["staleness_marker"])` |
| Agents comptabilité | TTL de 7 jours | `datetime.now() - created_at > timedelta(days=7)` |

---

## Ce qui est hors scope (et pourquoi)

| Élément | Raison |
|---|---|
| `update_partial()` sur le snapshot | L'implémentation par défaut serait un re-bootstrap complet. Prématuré pour v1. Un agent qui en a besoin surcharge `run_bootstrap()` avec une logique conditionnelle. |
| Flag `interactive` dans le protocole | Le HITL est la responsabilité de `run()`, pas du protocole. Évite le couplage implicite. |
| Bootstrap cross-agents | Le namespace isolation (ADR-070) l'interdit par design. |
| Bootstrap distribué | Apollia OS est local-first (Principe #1). |
| Migration de schéma des snapshots | Un vieux snapshot déclenche simplement un re-bootstrap via `is_stale()`. Sur-ingénierie pour v1. |

---

## Alternatives considérées

### Option A — Nouveau hook `on_bootstrap()` dans le contrat AIP (rejetée)

Ajouter une troisième méthode au contrat AIP (`manifest()` + `run()` + `on_bootstrap()`).
**Contre :** Modifie le contrat AIP — viole le Principe #3 (contrat minimal). Impose le
bootstrapping à tous les agents, y compris les workers qui n'en ont pas besoin.

### Option B — Sous-commande CLI `apollia bootstrap` (rejetée)

Le bootstrap devient une opération externe déclenchée par l'utilisateur.
**Contre :** Concept externe à l'agent. Viole le Principe #6 (mémoire à initiative de l'agent).
L'agent ne peut plus décider lui-même si son contexte est périmé.

### Option C — Injection automatique par le runtime (rejetée)

Le runtime détecte le premier tour de session et injecte automatiquement un appel de bootstrap.
**Contre :** Viole explicitement le Principe #6. Le runtime ne peut pas savoir quelle logique
de staleness est pertinente pour chaque domaine d'agent.

### Option D — Classe utilitaire sans protocole (rejetée)

Une simple fonction `load_or_bootstrap(ctx, fn)` sans classe abstraite.
**Contre :** Empêche la surcharge partielle et rend impossible la composition (ex. `ProjectContextBootstrap`
comme base partagée pour plusieurs agents spécialisés).

### Option retenue — Protocole abstrait SDK, lazy inline dans `run()`

**Pour :**
- Zéro modification du contrat AIP.
- L'agent reste maître de son cycle de vie.
- Extensible : `ProjectContextBootstrap` peut être une base partagée pour les agents dev.
- Testable isolément via `MockMemory` sans runtime Rust.
- Compatible avec n'importe quel SDK externe (`pip install apollia-sdk`).

---

## Conséquences

**Positives :**
- Suppression du pattern copié-collé dans spec-assistant, dev-assistant, review-assistant.
- Détection automatique de péremption — APOLLIA.md modifié → re-bootstrap au prochain tour.
- Contexte enrichi persistant : architecture, patterns, deps interdites, fichiers clés.
- Économie de tokens : la phase de découverte n'est payée qu'une fois, pas à chaque session.
- Un développeur externe peut implémenter son propre bootstrap avec seulement 2 méthodes.

**Négatives / Compromis :**
- Premier lancement sur un projet vierge : latence 3-5s le temps du bootstrap.
- Les agents Sprint 39 doivent être migrés (STORY-511 → STORY-513). Migration non-rétrocompatible
  sur la structure des clés mémoire (`bootstrap.*` vs `project_rules`).
- Les snapshots obsolètes s'accumulent en mémoire si `is_stale()` ne retourne jamais True
  (cas pathologique — la convention est de retourner True en cas de doute).

**Neutres / À surveiller :**
- Taille des snapshots : tronquer les contenus bruts à 8K max, stocker des chemins plutôt que
  des contenus complets.
- Le snapshot ne se substitue pas à `ctx.workspace` — les deux coexistent et se complètent.

---

## Principes architecturaux impactés

- **Principe #3 — Contrat minimal** : Le protocole est une couche SDK, pas AIP. Aucun agent
  existant n'est cassé.
- **Principe #6 — Mémoire à initiative de l'agent** : Renforcé. Le bootstrap est déclenché
  explicitement par l'agent dans `run()`, jamais par le runtime.
- **Principe #1 — Local-first** : Renforcé. Les snapshots sont stockés dans SQLite local,
  zéro octet ne sort de la machine.

---

## Liens

- Spec détaillée du protocole : [`docs/specs/context-bootstrapping-spec.md`](../specs/context-bootstrapping-spec.md)
- Spec sprint d'implémentation : [`docs/specs/sprint-40-spec.md`](../specs/sprint-40-spec.md)
- ADR connexe : [ADR-007 — Mémoire à initiative de l'agent](ADR-007-memoire-initiative-agent.md)
- ADR connexe : [ADR-070 — Memory namespace project-scoped](ADR-070-memory-namespace-project-scoped.md)
- Implementation SDK : `sdk/apollia/bootstrap.py` (version 0.2.0+)
- Tests unitaires : `sdk/tests/test_bootstrap.py` (15 tests, 100% coverage sur la classe de base)
- Stories d'implémentation :
  - [STORY-511](../internal/STORIES/sprint-40/story-511-project-context-bootstrap.md) — `ProjectContextBootstrap`
  - [STORY-512](../internal/STORIES/sprint-40/story-512-bootstrap-spec-dev.md) — adoption spec-assistant + dev-assistant
  - [STORY-513](../internal/STORIES/sprint-40/story-513-bootstrap-review-document.md) — adoption review-assistant + document-assistant
  - [STORY-514](../internal/STORIES/sprint-40/story-514-integration-tests.md) — tests d'intégration
