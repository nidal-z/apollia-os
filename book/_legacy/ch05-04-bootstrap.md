# ContextBootstrap : mémoire cross-session

Un agent qui redécouvre son projet à chaque session gaspille des tokens et du temps. Le protocole `ContextBootstrap` résout ce problème : l'agent explore son contexte projet une seule fois, persiste un snapshot en mémoire sémantique, et détecte automatiquement quand ce snapshot est périmé.

---

## Le problème

Imaginez un agent `dev-assistant` qui travaille sur un projet Rust. À chaque session, il :
1. Lit `APOLLIA.md` pour récupérer les règles du projet
2. Explore `Cargo.toml` pour détecter la stack technique
3. Liste les crates pour comprendre l'architecture

C'est du gaspillage : rien n'a changé depuis la dernière session. Le `ContextBootstrap` élimine cette redondance.

## Le protocole en 2 méthodes

```python
from apollia.bootstrap import ContextBootstrap

class MonBootstrap(ContextBootstrap):
    async def is_stale(self, ctx) -> bool:
        """Le snapshot est-il périmé ?"""
        meta = await self.load_meta(ctx)
        if meta is None:
            return True
        # Comparer le commit actuel au commit stocké
        current = await self._get_commit(ctx)
        return current != meta.get("staleness_marker")

    async def run_bootstrap(self, ctx) -> dict:
        """Explorer et construire le snapshot."""
        commit = await self._get_commit(ctx)
        snapshot = {
            "commit_hash": commit,
            "workspace_rules": ctx.workspace.rules or "",
            "tech_stack": ["Cargo.toml"],
        }
        await self.persist(ctx, snapshot, staleness_marker=commit)
        return snapshot
```

L'agent intègre le bootstrap dans `run()` :

```python
class MonAssistant(ConversationalAgent):
    def __init__(self):
        self._bootstrap = MonBootstrap()

    async def run(self, task, ctx):
        # Bootstrap uniquement si nécessaire
        if await self._bootstrap.needs_bootstrap(ctx):
            await self._bootstrap.run_bootstrap(ctx)

        # Charger le snapshot (gratuit si déjà en mémoire)
        snapshot = await self._bootstrap.load_snapshot(ctx)
        rules = snapshot.get("workspace_rules", "") if snapshot else ""
        # ... utiliser rules dans le prompt ...
```

## Ce qui se passe sous le capot

**Session 1** — premier lancement :
1. `needs_bootstrap()` → pas de `bootstrap.status` en mémoire → `True`
2. `run_bootstrap()` → explore (git, fichiers, workspace), construit le snapshot
3. `persist()` → écrit 3 clés en mémoire sémantique : `bootstrap.snapshot`, `bootstrap.meta`, `bootstrap.status`

**Session 2** — même commit :
1. `needs_bootstrap()` → status = `"complete"` → appelle `is_stale()`
2. `is_stale()` → commit HEAD identique au marqueur stocké → `False`
3. `load_snapshot()` → lecture directe depuis SQLite → ~0 tokens consommés

**Session 3** — nouveau commit :
1. `needs_bootstrap()` → `is_stale()` détecte un commit différent → `True`
2. `run_bootstrap()` → re-explore, met à jour le snapshot

## `ProjectContextBootstrap` — pour les agents de développement

Les agents du pipeline dev (spec, dev, review) partagent les mêmes besoins de base. `ProjectContextBootstrap` implémente les scopes communs :

- **Commit hash** — marqueur de péremption
- **Workspace rules** — depuis `ctx.workspace.rules` (pas de `file_read`)
- **Tech stack** — détection de `Cargo.toml`, `package.json`, `pyproject.toml`, etc.
- **Hook `extra_scopes()`** — chaque sous-classe ajoute ses scopes spécifiques

```python
from agents.assistants.shared.project_bootstrap import ProjectContextBootstrap

class DevContextBootstrap(ProjectContextBootstrap):
    async def extra_scopes(self, ctx, base_snapshot):
        # Détecter l'architecture du projet
        result = await ctx.tools.call("bash_executor", {
            "command": "find . -maxdepth 3 -name 'Cargo.toml' | head -20"
        })
        modules = result.get("stdout", "").strip().split("\n") if result else []
        return {"architecture": modules}
```

## Variantes de péremption

Le marqueur de péremption dépend du domaine de l'agent :

| Agent | Marqueur | Logique |
|---|---|---|
| Dev/Spec/Review | Commit hash | Stale si HEAD a changé |
| Document | Timestamp | Stale après 7 jours |
| Comptabilité | TTL 7 jours | Identique |

---

> **Référence technique :** [Agents-ContextBootstrap-Guide](https://github.com/nidal-z/apollia-os/wiki/Agents-ContextBootstrap-Guide)
