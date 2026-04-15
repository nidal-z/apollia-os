"""csv-data-worker — Worker Agent for CSV data analysis on Apollia OS.

Specialised agent that reads, analyses, filters, and transforms CSV files
using pandas. Handles encoding detection (UTF-8, latin-1, utf-8-sig) and
separator detection (, or ;) automatically. Works on any LLM backend (7B+).

Required packages (installed automatically at agent initialisation):
  pandas>=2.0.0
"""

from __future__ import annotations

from typing import Any

from apollia.agents import AIPResult, WorkerAgent

SYSTEM_PROMPT: str = """Tu es csv-data-worker, un agent expert de l'analyse de fichiers CSV via pandas.

## RÈGLES ABSOLUES

1. Toujours détecter l'encodage : essayer UTF-8 d'abord, puis latin-1 si UnicodeDecodeError, puis utf-8-sig.
   RAISON : Les CSVs français exportés depuis Excel Windows sont souvent en latin-1.

2. Toujours inspecter df.dtypes avant d'effectuer des calculs numériques sur une colonne.
   RAISON : Une colonne "Prix" lue comme `object` ne peut pas être sommée directement.

3. Ne jamais supposer le séparateur : si pd.read_csv() retourne 1 seule colonne, réessayer avec sep=";".
   RAISON : Les CSVs européens utilisent ";" comme séparateur au lieu de ",".

4. Toujours signaler les NaN avant tout calcul d'agrégat (df.isnull().sum()).
   RAISON : df.groupby().sum() sur une colonne avec NaN retourne des résultats trompeurs.

## IMPORTS STANDARDS

```python
import pandas as pd
```

## PATTERNS OBLIGATOIRES

### Lecture avec détection auto d'encodage et séparateur
```python
def read_csv_safe(path):
    for enc in ("utf-8", "latin-1", "utf-8-sig"):
        try:
            df = pd.read_csv(path, encoding=enc)
            if len(df.columns) == 1:    # probablement mauvais séparateur
                df = pd.read_csv(path, sep=";", encoding=enc)
            return df
        except UnicodeDecodeError:
            continue
    raise ValueError(f"Impossible de décoder {path}")
```

### Statistiques descriptives
```python
await ctx.log(f"DataFrame summary:\\n{df.describe()}")
await ctx.log(f"Valeurs manquantes :\\n{df.isnull().sum()}")
await ctx.log(f"Types de colonnes :\\n{df.dtypes}")
```

### Groupby et agrégation
```python
grouped = df.groupby("colonne_cle")["colonne_valeur"].agg(["sum", "mean", "count"])
await ctx.log(f"Groupby result:\\n{grouped.to_markdown()}")
```

### Filtrage
```python
filtered = df[df["colonne"] > valeur].copy()
```

### Export
```python
df.to_csv(output_path, index=False, encoding="utf-8")
await ctx.log(f"Exporté {len(df)} lignes vers {output_path}")
```

## GESTION DES ERREURS DOMAINE

- `FileNotFoundError` → message clair : "Fichier introuvable : {path}"
- `UnicodeDecodeError` sur tous les encodings → informer que l'encodage est inconnu
- `KeyError` sur colonne → lister les colonnes disponibles : list(df.columns)
- `EmptyDataError` → informer que le fichier est vide
- `ParserError` → informer que le fichier ne peut pas être parsé (mauvais format)

## FORMAT DE RÉPONSE

- Toujours indiquer l'encodage détecté, le séparateur, et le nombre de lignes/colonnes
- Pour describe() : présenter en tableau Markdown compact (to_markdown())
- Pour les transformations : confirmer les lignes avant/après + path du fichier exporté
- Pour les erreurs : message clair avec le path du fichier et la raison précise
"""


def manifest() -> dict[str, Any]:
    """Return the AIP agent manifest for csv-data-worker."""
    return {
        "name": "csv-data-worker",
        "version": "0.2.0",
        "description": (
            "Agent spécialisé pour l'analyse et la transformation de fichiers CSV. "
            "Lit, filtre, agrège et exporte des données CSV via pandas. "
            "Gère automatiquement l'encodage (UTF-8, latin-1) et le séparateur (, ou ;)."
        ),
        "execution_mode": "direct",
        "agent_type": "worker",
        "tools_required": ["python_executor", "file_read"],
        "tools_optional": ["file_write"],
        "tools_requiring_approval": [],
        "packages": ["pandas>=2.0.0"],
        "memory_namespace": "csv-data-worker",
        "supports_a2a": True,
        "skills": [
            {
                "id": "read-csv",
                "name": "Lire un fichier CSV",
                "description": "Parse et retourne le contenu d'un CSV avec détection auto de l'encodage",
                "input_modes": ["text"],
                "output_modes": ["text"],
            },
            {
                "id": "analyze-csv",
                "name": "Analyser les données CSV",
                "description": "Statistiques descriptives, comptage valeurs manquantes, groupby",
                "input_modes": ["text"],
                "output_modes": ["text"],
            },
            {
                "id": "transform-csv",
                "name": "Transformer un CSV",
                "description": "Filtrer, trier, renommer colonnes, exporter nouveau CSV",
                "input_modes": ["text"],
                "output_modes": ["text"],
            },
        ],
        "tags": ["csv", "data", "pandas", "analysis", "worker"],
        "max_concurrent_tasks": 1,
        "dangerous_tools_allowed": False,
    }


class CsvDataWorkerAgent(WorkerAgent):
    """Worker Agent specialised for CSV data analysis via pandas.

    The expertise (encoding detection, separator handling, NaN guardrails,
    dtype inspection) is compiled into SYSTEM_PROMPT and the class, not
    injected at runtime. This makes the agent reliable on small LLMs (7B+)
    that cannot reliably discover pandas patterns on their own.
    """

    SYSTEM_PROMPT = SYSTEM_PROMPT
    MAX_STEPS = 8
    TEMPERATURE = 0.1

    def manifest(self) -> dict[str, Any]:
        """Return the AIP agent manifest for csv-data-worker."""
        return manifest()

    async def run(self, task: dict[str, Any], ctx: Any) -> dict[str, Any]:
        """Execute the CSV task using the ReAct loop.

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


agent = CsvDataWorkerAgent()
