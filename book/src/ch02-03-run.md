# Implémenter run()

Maintenant qu'on a le manifest, écrivons la logique métier. On va construire `run()` étape par étape, en expliquant chaque décision.

---

## Étape 1 — Extraire le chemin du fichier

L'utilisateur envoie : `"Résume /data/rapport.txt"`

On doit extraire `/data/rapport.txt` de ce texte. Voici une heuristique simple :

```python
import re

def _extract_path(self, text: str) -> str | None:
    """Extrait le premier chemin de fichier trouvé dans le texte."""
    # Cherche un token qui commence par /, ./, ou ~/ et a une extension
    match = re.search(r'[~/.]?/[\w./-]+\.\w+', text)
    return match.group(0) if match else None
```

Cette méthode privée est appelée au début de `run()` :

```python
async def run(self, task, ctx):
    # Extraire l'entrée texte
    parts = task["input"]["parts"]
    if not parts or parts[0].get("type") != "text":
        return {
            "task_id": task["task_id"],
            "status": "failed",
            "error": {"code": "MISSING_INPUT", "message": "Une entrée texte est requise"},
        }

    user_text = parts[0]["text"]

    # Extraire le chemin
    file_path = self._extract_path(user_text)
    if not file_path:
        return {
            "task_id": task["task_id"],
            "status": "failed",
            "error": {
                "code": "NO_FILE_PATH",
                "message": f"Aucun chemin de fichier trouvé dans : {user_text!r}",
            },
        }
```

> **Pourquoi une regex et pas le LLM ?** Pour une opération aussi simple et déterministe, une regex est plus rapide, moins coûteuse, et ne consomme pas de budget LLM. Pour des instructions plus complexes (`"Résume le rapport le plus récent dans /data/"`), on utiliserait `ctx.llm` pour l'extraction — c'est une amélioration naturelle pour la V2.

---

## Étape 2 — Lire le fichier

```python
    # Lire le fichier
    read_result = await ctx.tools.call("file_read", {"path": file_path})

    if read_result.get("error"):
        return {
            "task_id": task["task_id"],
            "status": "failed",
            "error": {
                "code": "FILE_NOT_FOUND",
                "message": f"Impossible de lire {file_path} : {read_result['error']}",
            },
        }

    file_content = read_result["content"]
    total_lines = read_result.get("total_lines", 0)
```

`ctx.tools.call("file_read", ...)` retourne un dict Python avec le contenu du fichier. Si le fichier n'existe pas ou si le chemin tente un traversal hors sandbox, le dict contient un champ `"error"` avec un code machine.

> **Qu'est-ce que le sandbox ?** `file_read` valide que le chemin reste dans le répertoire sandbox de l'agent (`~/.apollia/sandboxes/<agent_id>/`). Un chemin absolu vers `/data/rapport.txt` est autorisé si ce chemin est accessible — la protection est contre les traversals (`../../etc/passwd`). Voir le chapitre 4 pour les détails.

---

## Étape 3 — Résumer via LLM

```python
    # Vérifier que le LLM est disponible
    if ctx.llm is None:
        return {
            "task_id": task["task_id"],
            "status": "failed",
            "error": {
                "code": "LLM_UNAVAILABLE",
                "message": "Aucun backend LLM configuré. Voir le chapitre 6.",
            },
        }

    # Résumer
    response = await ctx.llm.chat(
        system=(
            "Tu es un assistant expert en synthèse de documents. "
            "Produis un résumé clair et concis en français. "
            "Le résumé doit tenir en 5 à 10 phrases. "
            "Commence directement par le résumé, sans introduction."
        ),
        user=f"Résume ce document ({total_lines} lignes) :\n\n{file_content}",
    )

    summary = response.content
```

`ctx.llm.chat()` prend un `system` prompt et un message `user`, et retourne un objet avec `.content` (le texte généré) et `.usage` (tokens consommés, coût).

---

## Étape 4 — Sauvegarder le résumé

```python
    # Construire le chemin du fichier résumé
    if "." in file_path.split("/")[-1]:
        # /data/rapport.txt → /data/rapport_summary.txt
        base, ext = file_path.rsplit(".", 1)
        summary_path = f"{base}_summary.{ext}"
    else:
        summary_path = f"{file_path}_summary"

    # Écrire le fichier résumé
    write_result = await ctx.tools.call("file_write", {
        "path": summary_path,
        "content": f"Résumé de {file_path}\nGénéré le : {_now()}\n\n{summary}\n",
    })
```

---

## Étape 5 — Retourner le résultat

```python
    return {
        "task_id": task["task_id"],
        "status": "completed",
        "output": [
            {
                "type": "text",
                "text": (
                    f"Résumé de {file_path} :\n\n{summary}\n\n"
                    f"Résumé sauvegardé dans : {summary_path}"
                ),
            }
        ],
    }
```

---

## run() complet

Voici `run()` assemblé, avec le helper `_now()` :

```python
import re
from datetime import datetime


def _now() -> str:
    return datetime.now().strftime("%Y-%m-%d %H:%M")


class FileAssistant:

    def manifest(self):
        return {
            "name": "file-assistant",
            "version": "1.0.0",
            "description": "Lit un fichier, le résume via LLM, sauvegarde le résumé",
            "tools_required": ["file_read", "file_write"],
            "max_concurrent_tasks": 1,
            "step_budget": 10,
        }

    def _extract_path(self, text: str) -> str | None:
        match = re.search(r'[~/.]?/[\w./-]+\.\w+', text)
        return match.group(0) if match else None

    async def run(self, task, ctx):
        # --- Extraire l'entrée ---
        parts = task["input"]["parts"]
        if not parts or parts[0].get("type") != "text":
            return {
                "task_id": task["task_id"],
                "status": "failed",
                "error": {"code": "MISSING_INPUT", "message": "Une entrée texte est requise"},
            }

        user_text = parts[0]["text"]

        # --- Extraire le chemin ---
        file_path = self._extract_path(user_text)
        if not file_path:
            return {
                "task_id": task["task_id"],
                "status": "failed",
                "error": {
                    "code": "NO_FILE_PATH",
                    "message": f"Aucun chemin de fichier trouvé dans : {user_text!r}",
                },
            }

        # --- Lire le fichier ---
        read_result = await ctx.tools.call("file_read", {"path": file_path})
        if read_result.get("error"):
            return {
                "task_id": task["task_id"],
                "status": "failed",
                "error": {
                    "code": "FILE_NOT_FOUND",
                    "message": f"Impossible de lire {file_path} : {read_result['error']}",
                },
            }

        file_content = read_result["content"]
        total_lines = read_result.get("total_lines", 0)

        # --- Résumer via LLM ---
        if ctx.llm is None:
            return {
                "task_id": task["task_id"],
                "status": "failed",
                "error": {
                    "code": "LLM_UNAVAILABLE",
                    "message": "Aucun backend LLM configuré. Voir le chapitre 6.",
                },
            }

        response = await ctx.llm.chat(
            system=(
                "Tu es un assistant expert en synthèse de documents. "
                "Produis un résumé clair et concis en français. "
                "Le résumé doit tenir en 5 à 10 phrases. "
                "Commence directement par le résumé, sans introduction."
            ),
            user=f"Résume ce document ({total_lines} lignes) :\n\n{file_content}",
        )

        summary = response.content

        # --- Sauvegarder le résumé ---
        if "." in file_path.split("/")[-1]:
            base, ext = file_path.rsplit(".", 1)
            summary_path = f"{base}_summary.{ext}"
        else:
            summary_path = f"{file_path}_summary"

        await ctx.tools.call("file_write", {
            "path": summary_path,
            "content": f"Résumé de {file_path}\nGénéré le : {_now()}\n\n{summary}\n",
        })

        # --- Retourner le résultat ---
        return {
            "task_id": task["task_id"],
            "status": "completed",
            "output": [
                {
                    "type": "text",
                    "text": (
                        f"Résumé de {file_path} :\n\n{summary}\n\n"
                        f"Résumé sauvegardé dans : {summary_path}"
                    ),
                }
            ],
        }


agent = FileAssistant()
```

---

## Lire run() comme un contrat

Remarquez la structure de `run()` : chaque condition d'erreur est vérifiée **avant** d'aller plus loin. On ne commence pas à écrire le fichier résumé si la lecture a échoué. On ne commence pas à appeler le LLM si le fichier est introuvable.

Cette structure "vérifier avant d'avancer" est un pattern fondamental dans les agents bien écrits. Elle garantit que l'état de sortie (`status`) reflète toujours ce qui s'est réellement passé.

La section suivante explique en détail comment fonctionnent `file_read`, `file_write`, et `ctx.llm`.
