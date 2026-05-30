# Garde-fous domaine dans le code

Le `SYSTEM_PROMPT` est efficace pour guider le modèle. Mais il reste du texte — le modèle peut l'ignorer sur un modèle très léger, ou dans un contexte saturé après plusieurs étapes. Pour les règles critiques, la deuxième ligne de défense est le code Python.

Les garde-fous dans le code ne demandent pas — ils bloquent.

---

## Deux niveaux de défense

| Niveau | Mécanisme | Peut être contourné ? |
|---|---|---|
| SYSTEM_PROMPT | Instructions textuelles au modèle | Oui (modèle léger, contexte saturé) |
| Code Python | Vérifications dans `run()` avant/après le ReAct loop | Non |

Un Worker Agent robuste utilise les deux. Le SYSTEM_PROMPT guide le modèle dans les cas normaux. Le code Python attrape les cas où le modèle dévie.

---

## Valider l'entrée avant le ReAct loop

La première vérification utile : valider que les prérequis de la tâche sont satisfaits avant même de démarrer le raisonnement. Si le fichier n'existe pas, inutile de consommer des steps.

```python
async def run(self, task, ctx):
    user_message = (
        task.get("input", {}).get("text", "")
        if isinstance(task.get("input"), dict)
        else str(task.get("input", ""))
    )

    # Garde-fou 1 : extraire et valider le chemin du fichier
    file_path = self._extract_file_path(user_message)
    if file_path:
        path = Path(file_path)
        if not path.exists():
            return AIPResult.failed(
                "file_not_found",
                f"Fichier introuvable : {file_path}"
            )
        if path.suffix.lower() not in (".csv", ".tsv", ".txt"):
            return AIPResult.failed(
                "unsupported_format",
                f"Format non supporté : {path.suffix}. Utilisez .csv, .tsv ou .txt"
            )
        if path.stat().st_size > 500 * 1024 * 1024:  # 500 MB
            return AIPResult.failed(
                "file_too_large",
                "Fichier trop volumineux (> 500 MB) — traitement en streaming non supporté"
            )

    # Déléguer au ReAct loop
    result = await self.react(task, ctx, user_message)
    if isinstance(result, dict):
        return result
    return AIPResult.completed(result)

def _extract_file_path(self, message: str) -> str | None:
    """Extrait le premier chemin de fichier d'un message utilisateur."""
    import re
    match = re.search(r'(/[\w/._-]+\.(?:csv|tsv|txt))', message)
    return match.group(1) if match else None
```

---

## Intercepter les erreurs domaine après le ReAct loop

Le modèle peut produire du code Python qui lève des exceptions. L'ORIA Engine les capture et les transmet — mais vous pouvez les intercepter et les traduire en erreurs structurées compréhensibles :

```python
async def run(self, task, ctx):
    user_message = ...  # extraction comme ci-dessus

    try:
        result = await self.react(task, ctx, user_message)
    except Exception as exc:
        # Traduire les exceptions connues en codes d'erreur stables
        error_map = {
            "FileNotFoundError": ("file_not_found", "Fichier introuvable"),
            "EmptyDataError": ("empty_file", "Le fichier CSV est vide"),
            "ParserError": ("parse_error", "Format CSV invalide"),
            "UnicodeDecodeError": ("encoding_error", "Encodage non supporté"),
            "MemoryError": ("file_too_large", "Fichier trop volumineux"),
        }
        exc_type = type(exc).__name__
        code, message = error_map.get(exc_type, ("internal_error", str(exc)))
        return AIPResult.failed(code, message)

    if isinstance(result, dict):
        return result
    return AIPResult.completed(result)
```

---

## Valider le résultat avant de le retourner

Pour les tâches qui produisent des fichiers de sortie, vérifier que le fichier existe bien et n'est pas vide :

```python
async def run(self, task, ctx):
    result = await self.react(task, ctx, user_message)

    # Garde-fou sur le fichier de sortie déclaré dans le résultat
    if isinstance(result, dict) and result.get("status") == "completed":
        output = result.get("output", {})
        if isinstance(output, dict) and "file_path" in output:
            out_path = Path(output["file_path"])
            if not out_path.exists() or out_path.stat().st_size == 0:
                return AIPResult.failed(
                    "output_missing",
                    f"Le fichier de sortie n'a pas été créé : {out_path}"
                )

    return result
```

---

## Principe : défense en profondeur

L'architecture de défense d'un Worker Agent fonctionne par couches :

```
Couche 1 : SYSTEM_PROMPT
  → Guide le modèle vers les patterns corrects
  → Formule explicitement les interdictions avec RAISON

Couche 2 : Validation dans run() avant le ReAct loop
  → Vérifie les prérequis (fichier existe, format supporté, taille)
  → Retourne une erreur claire sans consommer de steps

Couche 3 : Interception des erreurs après le ReAct loop
  → Traduit les exceptions Python en codes d'erreur stables
  → Garantit que la réponse est toujours structurée

Couche 4 : Validation du résultat
  → Vérifie que les fichiers de sortie ont été créés
  → Permet au Director Agent d'intercepter les cas de défaillance
```

Chaque couche attrape ce que la précédente laisse passer. Le modèle ne peut pas contourner les couches 2, 3 et 4 — elles s'exécutent en dehors de son contrôle.

---

## Exemple complet — csv-data-worker avec défense en profondeur

```python
import re
from pathlib import Path
from typing import Any
from apollia_aip import WorkerAgent, AIPResult


SYSTEM_PROMPT: str = """..."""  # voir section précédente


def manifest() -> dict[str, Any]:
    return {
        "name": "csv-data-worker",
        "version": "0.1.0",
        "description": "Analyse et transformation de fichiers CSV (pandas). Compatible 7B+.",
        "execution_mode": "direct",
        "tools_required": ["python_executor", "file_read"],
        "tools_optional": ["file_write"],
        "packages": ["pandas>=2.0.0"],
        "memory_namespace": "csv-data-worker",
        "supports_a2a": True,
        "skills": [
            {
                "id": "analyze-csv",
                "name": "Analyser un CSV",
                "description": "Statistiques descriptives et inspection des types de colonnes.",
                "input_modes": ["text"],
                "output_modes": ["text"],
            },
        ],
        "tags": ["csv", "data", "pandas", "worker"],
        "max_concurrent_tasks": 1,
        "dangerous_tools_allowed": False,
    }


class CsvDataWorkerAgent(WorkerAgent):
    SYSTEM_PROMPT = SYSTEM_PROMPT
    MAX_STEPS = 8
    TEMPERATURE = 0.1

    def manifest(self) -> dict[str, Any]:
        return manifest()

    async def run(self, task: dict[str, Any], ctx: Any) -> dict[str, Any]:
        user_message = (
            task.get("input", {}).get("text", "")
            if isinstance(task.get("input"), dict)
            else str(task.get("input", ""))
        )

        # Couche 2 : validation des prérequis
        file_path = self._extract_csv_path(user_message)
        if file_path:
            path = Path(file_path)
            if not path.exists():
                return AIPResult.failed("file_not_found", f"Fichier introuvable : {file_path}")
            if path.suffix.lower() not in (".csv", ".tsv", ".txt"):
                return AIPResult.failed("unsupported_format",
                                        f"Format non supporté : {path.suffix}")
            if path.stat().st_size > 500 * 1024 * 1024:
                return AIPResult.failed("file_too_large", "Fichier trop volumineux (> 500 MB)")

        # Couche 3 : interception des erreurs domaine
        try:
            result = await self.react(task, ctx, user_message)
        except Exception as exc:
            error_map = {
                "FileNotFoundError": ("file_not_found", "Fichier introuvable"),
                "EmptyDataError":    ("empty_file",     "Le fichier CSV est vide"),
                "ParserError":       ("parse_error",    "Format CSV invalide"),
                "UnicodeDecodeError":("encoding_error", "Encodage non supporté"),
                "MemoryError":       ("file_too_large", "Fichier trop volumineux"),
            }
            code, msg = error_map.get(type(exc).__name__, ("internal_error", str(exc)))
            return AIPResult.failed(code, msg)

        if isinstance(result, dict):
            return result
        return AIPResult.completed(result)

    def _extract_csv_path(self, message: str) -> str | None:
        match = re.search(r'(/[\w/._-]+\.(?:csv|tsv|txt))', message)
        return match.group(1) if match else None


agent = CsvDataWorkerAgent()
```
