"""git-worker — Worker Agent for Git operations on Apollia OS.

Specialised agent that inspects repository state, diffs modifications, and
creates commits with conventional messages.  All Git commands are delegated
to ``bash_executor`` — no extra packages are required.

Guardrails built into this agent:
- Destructive operations are refused: ``git push --force``, ``git reset --hard``,
  ``git clean -fd``, ``git branch -D``, ``git checkout -- .``
- Commit messages must follow the conventional format: ``type(scope): description``
- ``git status`` is always run before any commit
- ``git add .`` / ``git add -A`` are forbidden without prior status inspection
- Remote operations (push, pull, fetch) require explicit user approval

Works on any LLM backend (7B+).
"""

from __future__ import annotations

from typing import Any

from apollia.agents import AIPResult, WorkerAgent

SYSTEM_PROMPT: str = """Tu es git-worker, un agent expert des opérations Git courantes.

## RÈGLES ABSOLUES (non-négociables)

1. N'exécute JAMAIS de commande destructive :
   - git push --force (ou -f)
   - git reset --hard
   - git clean -fd
   - git branch -D (suppression de branche)
   - git checkout -- . (discard all changes)
   Si l'utilisateur demande une de ces opérations, REFUSER et expliquer le risque.
   Répondre avec domain_error("destructive_operation", "Opération refusée : {command}").

2. Commits CONVENTIONNELS obligatoires (format Apollia) :
   git commit -m "type(scope): description"
   Types autorisés : feat, fix, refactor, test, docs, chore, perf
   Scope : nom du module ou de la crate concernée
   Exemples valides :
     feat(apollia-core): add AgentManifest type
     fix(apollia-runtime): handle timeout in supervisor
     docs(agents): update git-worker README

3. Toujours vérifier le status AVANT un commit :
   git status → analyser les fichiers modifiés → git add fichiers pertinents → git commit
   Ne jamais faire `git add .` ou `git add -A` sans vérifier ce qui sera ajouté.

4. Ne JAMAIS manipuler de branches distantes sans approbation explicite.
   Les opérations locales (status, diff, commit, branch, log) sont autorisées.
   Les opérations distantes (push, pull, fetch) nécessitent une confirmation de l'utilisateur.

## PATTERNS OBLIGATOIRES

### Status
```bash
git status --porcelain
```

### Diff
```bash
git diff              # Unstaged changes
git diff --staged     # Staged changes
git log --oneline -5  # Recent commits
```

### Commit sécurisé
```bash
git status --porcelain             # Vérifier d'abord
git add fichier1.py fichier2.rs    # Ajouter fichiers spécifiques — jamais git add .
git commit -m "feat(module): description de la modification"
git log --oneline -1               # Vérifier que le commit est bien créé
```

## GESTION DES ERREURS DOMAINE

- Pas un dépôt Git → domain_error("not_a_repo", "Le répertoire courant n'est pas un dépôt Git")
- Merge conflict → domain_error("merge_conflict", "Conflits de fusion détectés — résolution manuelle requise")
- Opération destructive demandée → domain_error("destructive_operation", "Opération refusée : {command}")
- Rien à committer → domain_error("nothing_to_commit", "Aucune modification à committer")

## FORMAT DE RÉPONSE

- Pour status : résumé clair (N fichiers modifiés, N staged, N untracked) avec liste des fichiers
- Pour diff : résumé des changements (fichiers touchés, lignes +/-)
- Pour commit : hash court du commit + message conventionnel + liste des fichiers inclus
"""


def manifest() -> dict[str, Any]:
    """Return the AIP agent manifest for git-worker."""
    return {
        "name": "git-worker",
        "version": "0.1.0",
        "description": (
            "Agent spécialisé pour les opérations Git courantes. "
            "Status, diff, commit avec messages conventionnels. "
            "Bloque les opérations destructives (force push, reset hard, clean -fd). "
            "Fonctionne sur tous les modèles LLM (7B+)."
        ),
        "execution_mode": "direct",
        "agent_type": "worker",
        "tools_required": ["bash_executor", "file_read"],
        "tools_optional": ["file_list"],
        "tools_requiring_approval": [],
        "packages": [],
        "memory_namespace": "git-worker",
        "supports_a2a": True,
        "step_budget": {
            "max_steps": 30,
            "max_tool_calls": 60,
            "wall_clock_secs": 900,
        },
        "skills": [
            {
                "id": "git-status",
                "name": "Status du dépôt",
                "description": "Affiche l'état du dépôt : fichiers modifiés, staged, untracked",
                "input_modes": ["text"],
                "output_modes": ["text"],
                "input_schema": {
                    "repo_path": {
                        "type": "string",
                        "description": "Chemin vers le dépôt Git (défaut : répertoire courant)",
                        "required": False,
                    },
                    "text": {
                        "type": "string",
                        "description": "Instruction en langage naturel",
                        "required": False,
                    },
                },
            },
            {
                "id": "git-diff",
                "name": "Diff des modifications",
                "description": "Affiche le diff détaillé des modifications en cours (unstaged et staged)",
                "input_modes": ["text"],
                "output_modes": ["text"],
                "input_schema": {
                    "repo_path": {
                        "type": "string",
                        "description": "Chemin vers le dépôt Git (défaut : répertoire courant)",
                        "required": False,
                    },
                    "staged_only": {
                        "type": "boolean",
                        "description": "Afficher uniquement les changements staged (défaut : false)",
                        "required": False,
                    },
                    "text": {
                        "type": "string",
                        "description": "Instruction en langage naturel",
                        "required": False,
                    },
                },
            },
            {
                "id": "git-commit",
                "name": "Commit des modifications",
                "description": (
                    "Stage les fichiers indiqués et crée un commit avec un message conventionnel. "
                    "Vérifie le status avant de committer. Refuse les opérations destructives."
                ),
                "input_modes": ["text"],
                "output_modes": ["text"],
                "input_schema": {
                    "files": {
                        "type": "array",
                        "description": "Liste des fichiers à inclure dans le commit (obligatoire)",
                        "required": True,
                    },
                    "message": {
                        "type": "string",
                        "description": "Message de commit au format conventionnel : type(scope): description",
                        "required": False,
                    },
                    "text": {
                        "type": "string",
                        "description": "Instruction en langage naturel décrivant les modifications",
                        "required": False,
                    },
                },
            },
        ],
        "tags": ["git", "version-control", "commit", "diff", "worker"],
        "max_concurrent_tasks": 1,
        "dangerous_tools_allowed": False,
    }


class GitWorkerAgent(WorkerAgent):
    """Worker Agent specialised for Git operations.

    The expertise (conventional commit format, destructive command blocking,
    status-before-commit pattern, domain error codes) is compiled into
    SYSTEM_PROMPT and this class.  This makes the agent reliable on small
    LLMs (7B+) that cannot reliably discover safe Git patterns on their own.
    """

    SYSTEM_PROMPT = SYSTEM_PROMPT
    MAX_STEPS = 30
    TEMPERATURE = 0.1

    def manifest(self) -> dict[str, Any]:
        """Return the AIP agent manifest for git-worker."""
        return manifest()

    async def run(self, task: dict[str, Any], ctx: Any) -> dict[str, Any]:
        """Execute the Git task using the ReAct loop.

        Extracts the user message from the task input, delegates to the
        inherited ReAct loop, and wraps the result in an AIPResult dict.
        """
        user_message = (
            task.get("input", {}).get("text", "")
            if isinstance(task.get("input"), dict)
            else str(task.get("input", ""))
        )
        result = await self.react(task, ctx, user_message)
        if isinstance(result, dict):
            return result
        return AIPResult.completed(result)


agent = GitWorkerAgent()
