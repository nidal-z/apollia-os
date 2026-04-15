# Agents — ContextBootstrap Guide — Apollia OS

> Protocole SDK pour explorer et persister un contexte projet cross-session. Un agent bootstrappé ne re-lit pas APOLLIA.md à chaque session — il détecte la péremption et re-bootstrap uniquement quand le projet a changé.
> Public cible : développeur d'agent Python

---

## Prérequis

- Apollia OS installé et fonctionnel (voir [Installation](./INSTALL-Quickstart))
- SDK `apollia-sdk >= 0.3.0` installé (`pip install -e ./sdk`)
- `memory_namespace` défini dans le manifest de l'agent (le bootstrap persiste en mémoire sémantique)

---

## Vue d'ensemble

### Le problème

Les agents Sprint 39 chargeaient leurs règles projet via `file_read("APOLLIA.md")` à chaque session — même quand rien n'avait changé. Ce pattern ad-hoc :
- Gaspille des tokens LLM à chaque démarrage
- N'a aucune détection de péremption (si APOLLIA.md change, le cache mémoire reste stale)
- Duplique la logique de chargement dans chaque agent

### La solution : `ContextBootstrap`

`ContextBootstrap` est un protocole SDK (classe abstraite Python) qui standardise :
1. **Exploration** — découverte du contexte projet (règles, architecture, stack technique)
2. **Persistance** — stockage du snapshot en mémoire sémantique SQLite
3. **Péremption** — détection automatique du changement (commit hash, timestamp, etc.)
4. **Réutilisation** — snapshot rechargé gratuitement en session N+1 si pas périmé

Le runtime n'injecte jamais le bootstrap — c'est l'agent qui le déclenche dans `run()` (Principe #6).

### Workspace vs Bootstrap

| | `ctx.workspace` | `ContextBootstrap` |
|---|---|---|
| **Scope** | Session (éphémère) | Cross-session (persistent SQLite) |
| **Contenu** | APOLLIA.md parsé par le runtime | Snapshot enrichi : architecture, patterns, deps |
| **Coût** | Zéro (injecté par runtime) | Payé une fois, réutilisé indéfiniment |
| **Fraîcheur** | Toujours actuel | Validé par marqueur de péremption |
| **Qui remplit** | Runtime Rust | L'agent lui-même via `run_bootstrap()` |

---

## 1. Le protocole — 2 méthodes à implémenter

```python
from apollia.bootstrap import ContextBootstrap

class MyBootstrap(ContextBootstrap):
    async def is_stale(self, ctx) -> bool:
        """Le snapshot existant est-il périmé ?
        Retourner True en cas de doute.
        """
        ...

    async def run_bootstrap(self, ctx) -> dict:
        """Explorer le domaine, construire un snapshot, appeler self.persist().
        Doit être idempotent — appels multiples = même état.
        """
        ...
```

### Méthodes héritées (infrastructure)

| Méthode | Comportement par défaut |
|---|---|
| `needs_bootstrap(ctx)` | Lit `bootstrap.status` → si None/missing/partial → True ; si complete → délègue à `is_stale()` |
| `load_snapshot(ctx)` | Lit `bootstrap.snapshot` → `json.loads()` ou None |
| `load_meta(ctx)` | Lit `bootstrap.meta` → `json.loads()` ou None |
| `persist(ctx, snapshot, *, staleness_marker, ...)` | Écrit snapshot + meta + status. Refuse le downgrade complete → partial |

### Clés mémoire convention

```
bootstrap.snapshot    # Snapshot JSON complet
bootstrap.meta        # {"version": 1, "created_at": str, "staleness_marker": str}
bootstrap.status      # "complete" | "partial" | "missing"
```

---

## 2. `ProjectContextBootstrap` — Base partagée agents dev

Pour les agents du pipeline de développement (spec, dev, review), une classe concrète `ProjectContextBootstrap` implémente les scopes communs :

```python
from agents.assistants.shared.project_bootstrap import ProjectContextBootstrap

class MyDevBootstrap(ProjectContextBootstrap):
    async def extra_scopes(self, ctx, base_snapshot) -> dict:
        """Hook pour ajouter des scopes spécifiques au domaine."""
        return {"custom_key": "custom_value"}
```

### Snapshot commun

```python
{
    "commit_hash": str,          # HEAD commit hash ou "no-git"
    "has_git": bool,             # True si workspace git
    "workspace_rules": str,      # Depuis ctx.workspace.rules (pas file_read)
    "tech_stack": list[str],     # Marqueurs détectés : Cargo.toml, package.json, etc.
    **extra                       # Ajouté par extra_scopes()
}
```

### Péremption

Le snapshot est périmé quand `git rev-parse HEAD` diffère du `staleness_marker` stocké. Les workspaces sans git utilisent `"no-git"` comme marqueur stable.

### Marqueurs de stack technique détectés

`Cargo.toml`, `package.json`, `pyproject.toml`, `go.mod`, `pom.xml`, `build.gradle`

---

## 3. Sous-classes livrées

### `SpecContextBootstrap`

Extra scopes : `existing_specs` (liste des fichiers `.apollia/tasks/*.md`), `spec_count`.

```python
# agents/assistants/spec-assistant.py
class SpecContextBootstrap(ProjectContextBootstrap):
    async def extra_scopes(self, ctx, base_snapshot):
        result = await ctx.tools.call("bash_executor", {
            "command": "ls .apollia/tasks/*.md 2>/dev/null | head -50"
        })
        specs = []
        if result and result.get("stdout"):
            specs = [l.strip() for l in result["stdout"].split("\n") if l.strip()]
        return {"existing_specs": specs, "spec_count": len(specs)}
```

### `DevContextBootstrap`

Extra scopes : `architecture` (modules détectés), `recent_files` (fichiers modifiés HEAD~10).

```python
# agents/assistants/dev-assistant.py
class DevContextBootstrap(ProjectContextBootstrap):
    async def extra_scopes(self, ctx, base_snapshot):
        arch_result = await ctx.tools.call("bash_executor", {
            "command": "find . -maxdepth 3 \\( -name 'Cargo.toml' -o -name 'mod.rs' "
                       "-o -name '__init__.py' \\) 2>/dev/null | grep -v target | head -40 | sort"
        })
        modules = [l.strip() for l in (arch_result or {}).get("stdout", "").split("\n") if l.strip()]

        recent_result = await ctx.tools.call("bash_executor", {
            "command": "git diff --name-only HEAD~10 HEAD 2>/dev/null | head -30"
        })
        recent = [l.strip() for l in (recent_result or {}).get("stdout", "").split("\n") if l.strip()]

        return {"architecture": modules, "recent_files": recent}
```

### `ReviewContextBootstrap`

Aucun extra scope — le snapshot de base (rules + tech stack) suffit pour la validation.

```python
# agents/assistants/review-assistant.py
class ReviewContextBootstrap(ProjectContextBootstrap):
    pass  # ProjectContextBootstrap suffit
```

### `DocumentContextBootstrap`

Hérite directement de `ContextBootstrap` (pas de `ProjectContextBootstrap`). Péremption par timestamp (7 jours max).

```python
# agents/assistants/document-assistant.py
class DocumentContextBootstrap(ContextBootstrap):
    _MAX_AGE_SECS = 7 * 24 * 3600  # 7 jours

    async def is_stale(self, ctx) -> bool:
        meta = await self.load_meta(ctx)
        if meta is None:
            return True
        try:
            last = float(meta.get("staleness_marker", ""))
        except (ValueError, TypeError):
            return True
        return (time.time() - last) > self._MAX_AGE_SECS

    async def run_bootstrap(self, ctx) -> dict:
        snapshot = {
            "format_preferences": {},
            "recent_files": [],
            "available_workers": [],
        }
        # Découverte des workers A2A actifs
        try:
            skills = await ctx.a2a_list_skills()
            snapshot["available_workers"] = list({s["agent_name"] for s in skills})
        except Exception:
            pass
        await self.persist(ctx, snapshot, staleness_marker=str(time.time()),
                           source="bootstrap:document")
        return snapshot
```

---

## 4. Intégration dans `run()`

Pattern commun aux 4 assistants Sprint 40 :

```python
class MonAssistant(ConversationalAgent):
    def __init__(self):
        self._bootstrap = MonBootstrap()

    async def run(self, task, ctx):
        input_text, history = _extract_task_input(task)
        is_first_turn = not history

        # Bootstrap uniquement au premier tour de conversation
        if is_first_turn and await self._bootstrap.needs_bootstrap(ctx):
            await self._bootstrap.run_bootstrap(ctx)

        # Charger le snapshot (gratuit si déjà en mémoire)
        snapshot = await self._bootstrap.load_snapshot(ctx)

        # Construire le prompt avec le contexte persisté
        workspace_rules = (
            snapshot.get("workspace_rules", "") if snapshot
            else (ctx.workspace.rules or "" if ctx.workspace else "")
        )
        # ... reste de run() inchangé ...
```

**Points clés :**
- Le bootstrap ne s'exécute qu'au **premier tour** de conversation
- `needs_bootstrap()` vérifie d'abord le status, puis la péremption
- Si le snapshot est frais (même commit), le coût est ~0 tokens (lecture mémoire seule)
- `ctx.workspace.rules` sert de fallback si le snapshot est absent

---

## 5. Créer son propre bootstrap

Pour un agent custom, implémenter `is_stale()` et `run_bootstrap()` suffit :

```python
from apollia.bootstrap import ContextBootstrap
import time

class AccountingBootstrap(ContextBootstrap):
    """Bootstrap pour agents comptables — staleness par TTL 7 jours."""

    _MAX_AGE = 7 * 24 * 3600

    async def is_stale(self, ctx) -> bool:
        meta = await self.load_meta(ctx)
        if meta is None:
            return True
        try:
            return (time.time() - float(meta["staleness_marker"])) > self._MAX_AGE
        except (KeyError, ValueError):
            return True

    async def run_bootstrap(self, ctx) -> dict:
        # Découvrir le contexte comptable
        snapshot = {
            "fiscal_year": "2026",
            "chart_of_accounts": await self._load_chart(ctx),
            "recent_entries": await self._load_recent(ctx),
        }
        await self.persist(
            ctx, snapshot,
            staleness_marker=str(time.time()),
            source="bootstrap:accounting",
        )
        return snapshot
```

### Variantes de staleness marker

| Domaine | Marqueur | Logique |
|---|---|---|
| Dev agents (pipeline Apollia) | Commit hash (`git rev-parse HEAD`) | `current_hash != meta["staleness_marker"]` |
| Document agents | Timestamp epoch | `time.time() - float(marker) > 7 jours` |
| Comptabilité | TTL 7 jours | Identique aux documents |
| CRM | Hash de la base contacts | `current_hash != marker` |

---

## Limitations connues

- **Latence premier lancement** : 3-5s de discovery lors du premier bootstrap (git, fichiers, détection stack)
- **Pas de `update_partial()`** : modifier un champ du snapshot nécessite un re-bootstrap complet
- **Pas de cross-agent** : le namespace mémoire isole les snapshots (ADR-070). Deux agents ne partagent pas un bootstrap
- **Snapshots obsolètes** : si `is_stale()` ne retourne jamais `True`, les données vieillissent sans limite. Recommandation : toujours inclure un marqueur temporel

---

## Voir aussi

- [Agents SDK Guide](./Agents-SDK-Guide) — référence complète du SDK Python
- [Agents RuntimeContext Guide](./Agents-RuntimeContext-Guide) — référence `ctx.*`
- [Briques Memory Engine](./Briques-Memory-Engine) — architecture mémoire sous-jacente
- [ADR-070](../adr/ADR-070-memory-namespace-project-scoped.md) — Memory namespace project-scoped
- [ADR-071](../adr/ADR-071-context-bootstrap-convention.md) — ContextBootstrap convention
