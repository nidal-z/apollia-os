"""code-worker — Worker Agent for code generation, refactoring, and review on Apollia OS.

Specialised agent that reads, analyses, and modifies source files via
bash_executor and file tools. Supports Python and Rust in V1. Works on any
LLM backend (7B+) thanks to an explicit SYSTEM_PROMPT that encodes all
guardrails and verification patterns.

Required tools (resolved at agent initialisation):
  bash_executor, file_read, file_write, file_edit
"""

from __future__ import annotations

from typing import Any

from apollia.agents import AIPResult, WorkerAgent

SYSTEM_PROMPT: str = """Tu es code-worker, un agent expert de la manipulation de code source.

## RÈGLES ABSOLUES (non-négociables)

1. TOUJOURS lire le fichier existant avec file_read AVANT toute modification.
   file_read AVANT file_write — sans exception.
   Ne JAMAIS écrire un fichier sans avoir lu l'original d'abord.
   RAISON : écraser un fichier sans le lire = perte potentielle de code non récupérable.

2. Pour les modifications partielles, utiliser file_edit (pas file_write).
   file_edit préserve le contexte et ne modifie que les lignes ciblées.
   file_write ne sert qu'à créer un nouveau fichier qui n'existait pas encore.

3. Après toute génération de code Python, vérifier la syntaxe :
   bash_executor("python -c 'import ast; ast.parse(open(\"output.py\").read())'")
   Si le code de retour est non-nul : analyser le message d'erreur et corriger avant de livrer.

4. Après toute génération de code Rust, vérifier la compilation :
   bash_executor("cargo check --message-format=short 2>&1 | head -20")
   Si le code de retour est non-nul : analyser les erreurs de compilation et corriger.

5. Format de revue de code OBLIGATOIRE :
   - 🟢 LGTM : le code est correct, aucun problème détecté
   - 🟡 SUGGESTION : amélioration optionnelle (performance, lisibilité, idiome)
   - 🔴 ISSUE : bug potentiel ou violation de convention à corriger impérativement
   Ne jamais porter de jugement subjectif (« ce code est laid »). Se limiter aux faits.

6. Langages supportés en V1 : Python et Rust uniquement.
   Pour tout autre langage : retourner domain_error("unsupported_language", ...) et
   donner des conseils génériques sans tentative de vérification syntaxique.

## PATTERNS OBLIGATOIRES

### Lire avant d'écrire (toujours)
```python
# Ce pattern est obligatoire pour toute modification
content = await file_read(path)
# ... préparer le contenu modifié ...
await file_write(path, modified_content)
```

### Vérification syntaxe Python
```python
result = await bash_executor(
    "python -c 'import ast; ast.parse(open(\"output.py\").read())'"
)
if result["exit_code"] != 0:
    # Lire le message d'erreur et corriger le fichier
    pass
```

### Vérification compilation Rust
```python
result = await bash_executor(
    "cargo check --message-format=short 2>&1 | head -20"
)
if result["exit_code"] != 0:
    # Analyser les erreurs de compilation et corriger
    pass
```

### Modification partielle avec file_edit
```python
# Pour remplacer seulement quelques lignes dans un grand fichier
await file_edit(path, old_snippet, new_snippet)
```

## GESTION DES ERREURS DOMAINE

- `FileNotFoundError` → domain_error("file_not_found", "Fichier introuvable : {path}")
- Syntaxe Python invalide → domain_error("syntax_error", {"language": "python", "errors": ...})
- Compilation Rust échouée → domain_error("compilation_error", {"language": "rust", "errors": ...})
- Langage non supporté → domain_error("unsupported_language", "Langage non supporté en V1. Langages disponibles : Python, Rust.")

## FORMAT DE RÉPONSE

- generate : retourner le chemin du fichier créé + résumé (nombre de lignes, fonctions, classes)
- refactor : retourner le résumé des modifications (lignes modifiées, ajoutées, supprimées)
- review : retourner la liste structurée LGTM / SUGGESTION / ISSUE avec numéro de ligne et explication
"""


def manifest() -> dict[str, Any]:
    """Return the AIP agent manifest for code-worker."""
    return {
        "name": "code-worker",
        "version": "0.2.0",
        "description": (
            "Agent spécialisé pour la génération, le refactoring et la revue de code. "
            "Lit, analyse et modifie des fichiers source via file tools et bash_executor. "
            "Supporte Python et Rust en V1. "
            "Fonctionne sur tous les modèles LLM (7B+)."
        ),
        "execution_mode": "direct",
        "agent_type": "worker",
        "tools_required": ["bash_executor", "file_read", "file_write", "file_edit"],
        "tools_optional": ["file_list", "file_glob", "file_grep"],
        "tools_requiring_approval": [],
        "packages": [],
        "memory_namespace": "code-worker",
        "supports_a2a": True,
        "skills": [
            {
                "id": "generate-code",
                "name": "Générer du code",
                "description": (
                    "Génère un fichier source à partir d'une description fonctionnelle. "
                    "Vérifie la syntaxe (Python) ou la compilation (Rust) avant de livrer."
                ),
                "input_modes": ["text"],
                "output_modes": ["text"],
                "input_schema": {
                    "description": {
                        "type": "string",
                        "description": "Description fonctionnelle du code à générer",
                        "required": True,
                    },
                    "language": {
                        "type": "string",
                        "description": "Langage cible : 'python' ou 'rust' (défaut : python)",
                        "required": False,
                    },
                    "output_path": {
                        "type": "string",
                        "description": "Chemin du fichier à créer (défaut : généré automatiquement)",
                        "required": False,
                    },
                },
            },
            {
                "id": "refactor-code",
                "name": "Refactorer du code",
                "description": (
                    "Améliore la structure d'un fichier source sans changer le comportement. "
                    "Lit toujours l'original avant de modifier."
                ),
                "input_modes": ["text"],
                "output_modes": ["text"],
                "input_schema": {
                    "file_path": {
                        "type": "string",
                        "description": "Chemin du fichier source à refactorer",
                        "required": True,
                    },
                    "instructions": {
                        "type": "string",
                        "description": "Instructions de refactoring (ex. 'extraire en fonctions', 'ajouter des types')",
                        "required": False,
                    },
                },
            },
            {
                "id": "review-code",
                "name": "Revue de code",
                "description": (
                    "Analyse un fichier source et retourne des observations structurées "
                    "(🟢 LGTM / 🟡 SUGGESTION / 🔴 ISSUE) avec numéros de ligne."
                ),
                "input_modes": ["text"],
                "output_modes": ["text"],
                "input_schema": {
                    "file_path": {
                        "type": "string",
                        "description": "Chemin du fichier source à analyser",
                        "required": True,
                    },
                    "focus": {
                        "type": "string",
                        "description": "Aspect particulier à examiner (ex. 'sécurité', 'performance', 'idiomes')",
                        "required": False,
                    },
                },
            },
        ],
        "tags": ["code", "python", "rust", "refactor", "review", "generate", "worker"],
        "max_concurrent_tasks": 1,
        "dangerous_tools_allowed": False,
    }


class CodeWorkerAgent(WorkerAgent):
    """Worker Agent specialised for code generation, refactoring, and review.

    The expertise (read-before-write enforcement, syntax verification via
    ast.parse, compilation check via cargo check, structured review format)
    is compiled into SYSTEM_PROMPT and this class — not injected at runtime.
    This makes the agent reliable on small LLMs (7B+) that cannot discover
    these patterns on their own and would otherwise write files destructively.
    """

    SYSTEM_PROMPT = SYSTEM_PROMPT
    MAX_STEPS = 10
    TEMPERATURE = 0.1

    def manifest(self) -> dict[str, Any]:
        """Return the AIP agent manifest for code-worker."""
        return manifest()

    async def run(self, task: dict[str, Any], ctx: Any) -> dict[str, Any]:
        """Execute the code task using the ReAct loop.

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


agent = CodeWorkerAgent()
