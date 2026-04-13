"""sql-worker — Worker Agent for SQLite database interrogation on Apollia OS.

Specialised agent that connects to local SQLite databases, inspects schemas,
executes parameterised SELECT queries, and exports results to CSV.  All SQL
execution is delegated to ``python_executor`` using Python's stdlib ``sqlite3``
module — no additional packages required.

Guardrails built into this agent:
- SELECT only by default; mutations require ``dangerous_tools_allowed: True``
- Parameterised queries only; f-string interpolation is strictly forbidden
- 30-second timeout per query
- File existence and integrity check (``PRAGMA integrity_check``) on first open
- Connection always closed via context manager ``with``

Works on any LLM backend (7B+).
"""

from __future__ import annotations

from typing import Any

from apollia.agents import AIPResult, WorkerAgent

SYSTEM_PROMPT: str = """Tu es sql-worker, un agent expert de l'interrogation de bases de données SQLite via Python.

## RÈGLES ABSOLUES (non-négociables)

1. SELECT UNIQUEMENT par défaut.
   N'exécute JAMAIS de INSERT, UPDATE, DELETE, DROP, ALTER, CREATE sans vérifier
   que dangerous_tools_allowed est True dans le manifest.
   RAISON : une mutation SQL est irréversible sur des données utilisateur.

2. PARAMÉTRAGE OBLIGATOIRE — jamais de f-string dans les requêtes SQL.
   ✅ cursor.execute("SELECT * FROM t WHERE name = ?", (user_input,))
   ❌ cursor.execute(f"SELECT * FROM t WHERE name = '{user_input}'")
   RAISON : injection SQL. Même en local, les données peuvent contenir du SQL malveillant.

3. TIMEOUT de 30 secondes sur chaque requête.
   Utiliser un contexte timeout pour éviter les requêtes infinies sur de grosses tables.

4. Toujours vérifier l'existence du fichier .db/.sqlite AVANT de l'ouvrir.
   Toujours vérifier l'intégrité via PRAGMA integrity_check sur la première connexion.

5. Toujours FERMER la connexion après usage (context manager `with`).

## IMPORTS STANDARDS

```python
import sqlite3
import os
```

## PATTERNS OBLIGATOIRES

### Connexion sécurisée
```python
if not os.path.exists(db_path):
    raise FileNotFoundError(f"Base de données introuvable : {db_path}")

with sqlite3.connect(db_path, timeout=30) as conn:
    conn.row_factory = sqlite3.Row
    cursor = conn.cursor()

    # Vérification intégrité (première connexion uniquement)
    integrity = cursor.execute("PRAGMA integrity_check").fetchone()[0]
    if integrity != "ok":
        raise ValueError(f"Base corrompue : {integrity}")
```

### Requête paramétrée (TOUJOURS — jamais de f-string)
```python
cursor.execute(
    "SELECT * FROM clients WHERE ville = ? AND ca > ?",
    (ville, seuil_ca),
)
rows = cursor.fetchall()
columns = [desc[0] for desc in cursor.description]
```

### Inspection du schéma
```python
tables = cursor.execute(
    "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name"
).fetchall()

for table in tables:
    name = table[0]
    cols = cursor.execute(f"PRAGMA table_info({name})").fetchall()
    # cols = [(cid, name, type, notnull, dflt_value, pk), ...]
```

### Export CSV
```python
import csv
with open(output_path, "w", newline="", encoding="utf-8") as f:
    writer = csv.writer(f)
    writer.writerow(columns)
    writer.writerows(rows)
```

## GESTION DES ERREURS DOMAINE

- `FileNotFoundError` → domain_error("file_not_found", "Base de données introuvable : {path}")
- `sqlite3.DatabaseError` (corrupted) → domain_error("corrupted_database", ...)
- `sqlite3.OperationalError` (locked) → domain_error("database_locked", "Base verrouillée — un autre processus l'utilise")
- `sqlite3.OperationalError` (syntax) → domain_error("sql_syntax_error", "Erreur SQL : {message}")
- Tentative mutation sans permission → domain_error("mutation_forbidden", "Les mutations SQL requièrent dangerous_tools_allowed: true")
- Timeout requête → domain_error("query_timeout", "Requête interrompue après 30 secondes")

## FORMAT DE RÉPONSE

- Pour les requêtes : tableau Markdown avec colonnes + nombre de lignes
- Pour le schéma : liste des tables avec colonnes et types
- Pour les erreurs : message clair + suggestion SQL corrigée si possible
"""


def manifest() -> dict[str, Any]:
    """Return the AIP agent manifest for sql-worker."""
    return {
        "name": "sql-worker",
        "version": "0.1.0",
        "description": (
            "Agent spécialisé pour l'interrogation de bases de données SQLite. "
            "Exécute des requêtes SQL sécurisées avec paramétrage obligatoire. "
            "SELECT uniquement par défaut — mutations requièrent dangerous_tools_allowed. "
            "Fonctionne sur tous les modèles LLM (7B+)."
        ),
        "execution_mode": "direct",
        "tools_required": ["python_executor", "file_read"],
        "tools_optional": [],
        "tools_requiring_approval": [],
        "packages": [],
        "memory_namespace": "sql-worker",
        "supports_a2a": True,
        "skills": [
            {
                "id": "query-sql",
                "name": "Exécuter une requête SQL",
                "description": "Exécute une requête SELECT et retourne les résultats formatés",
                "input_modes": ["text"],
                "output_modes": ["text"],
                "input_schema": {
                    "db_path": {
                        "type": "string",
                        "description": "Chemin absolu ou relatif vers le fichier SQLite (.db ou .sqlite)",
                        "required": True,
                    },
                    "query": {
                        "type": "string",
                        "description": "Requête SQL SELECT à exécuter (paramétrage ? obligatoire)",
                        "required": False,
                    },
                    "text": {
                        "type": "string",
                        "description": "Instruction en langage naturel décrivant la requête souhaitée",
                        "required": False,
                    },
                },
            },
            {
                "id": "schema-inspect",
                "name": "Inspecter le schéma",
                "description": "Liste les tables, colonnes, types et index d'une base SQLite",
                "input_modes": ["text"],
                "output_modes": ["text"],
                "input_schema": {
                    "db_path": {
                        "type": "string",
                        "description": "Chemin vers le fichier SQLite à inspecter",
                        "required": True,
                    },
                },
            },
            {
                "id": "data-export",
                "name": "Exporter des données",
                "description": "Exporte le résultat d'une requête SELECT en CSV",
                "input_modes": ["text"],
                "output_modes": ["text"],
                "input_schema": {
                    "db_path": {
                        "type": "string",
                        "description": "Chemin vers le fichier SQLite source",
                        "required": True,
                    },
                    "output_path": {
                        "type": "string",
                        "description": "Chemin du fichier CSV de sortie",
                        "required": True,
                    },
                },
            },
        ],
        "tags": ["sql", "sqlite", "database", "query", "worker"],
        "max_concurrent_tasks": 1,
        "dangerous_tools_allowed": False,
    }


class SqlWorkerAgent(WorkerAgent):
    """Worker Agent specialised for SQLite database interrogation.

    The expertise (correct sqlite3 patterns, parameterised queries, SELECT-only
    guardrails, integrity checks, error codes) is compiled into SYSTEM_PROMPT
    and this class.  This makes the agent reliable on small LLMs (7B+) that
    cannot reliably discover safe SQL patterns on their own.
    """

    SYSTEM_PROMPT = SYSTEM_PROMPT
    MAX_STEPS = 8
    TEMPERATURE = 0.1

    def manifest(self) -> dict[str, Any]:
        """Return the AIP agent manifest for sql-worker."""
        return manifest()

    async def run(self, task: dict[str, Any], ctx: Any) -> dict[str, Any]:
        """Execute the SQL task using the ReAct loop.

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


agent = SqlWorkerAgent()
