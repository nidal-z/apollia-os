# Le pattern Worker Agent

Un Worker Agent est un agent spécialisé dont l'expertise de domaine est **compilée dans le code** — pas injectée dans le contexte LLM à l'exécution.

La distinction est fondamentale. Un agent générique reçoit ses instructions dans chaque prompt : le LLM doit les lire, les mémoriser, et s'en souvenir pendant toute l'exécution. Sur un modèle frontier (Claude, GPT-4o), ça fonctionne bien. Sur un modèle léger 7–14B utilisé en local, les guardrails s'oublient après quelques étapes, les imports sont hallucinés, les séquences sont inversées.

Un Worker Agent élimine ce problème en rendant l'expertise incontournable.

---

## Agent générique vs Worker Agent

| Propriété | Agent générique | Worker Agent |
|---|---|---|
| Expertise de domaine | Injectée dans le contexte à runtime | Compilée dans `SYSTEM_PROMPT` — toujours présente |
| Guardrails | Dans le prompt de la tâche | Dans `SYSTEM_PROMPT` et dans le code — impossible à oublier |
| Compatibilité modèles légers | Dégradée (hallucinations, guardrails oubliés) | Robuste — testé avec modèles 7B+ |
| Dépendances pip | Non supportées nativement | Déclarées dans `manifest["packages"]`, installées automatiquement |
| Réutilisabilité A2A | Optionnelle | Première classe — `supports_a2a: True` natif |
| Cas d'usage | Logique métier générale, orchestration | Domaine spécialisé (formats de fichier, API spécifique, langages) |

---

## La règle des 2 conditions sur 3

Créer un Worker Agent si **au moins 2 des 3 conditions** suivantes sont réunies :

### Condition 1 — Séquence non-triviale

La tâche impose un ordre d'opérations que le LLM oublie ou inverse régulièrement.

> CSV : détecter l'encodage → détecter le séparateur → lire avec pandas → inspecter `dtypes` → calculer. Chaque étape dépend de la précédente. Sur un modèle 7B, le `dtypes` est souvent sauté, causant des résultats incorrects silencieux.

### Condition 2 — Guardrail critique

Il existe une façon dangereuse ou silencieusement incorrecte de faire la tâche que le LLM emprunte naturellement.

> CSV : un modèle peut tenter `float(valeur)` sur une colonne au lieu de `pd.to_numeric(..., errors='coerce')`. Les erreurs de conversion sont silencieuses, les NaN non détectés faussent les calculs.

### Condition 3 — Pattern d'erreur domaine récurrent

Le domaine produit des exceptions spécifiques avec des causes non-évidentes.

> `UnicodeDecodeError` sur CSV en latin-1, `EmptyDataError` sur CSV vide, `ParserError` sur CSV avec lignes incohérentes — autant de cas que le LLM doit connaître précisément pour répondre utilement.

```
2 conditions sur 3 → Worker Agent
moins de 2         → MCP Tool ou ORIA Mode Direct
```

---

## Anatomie d'un fichier Worker Agent

Un fichier Worker Agent contient exactement 4 éléments dans cet ordre :

```python
# 1. Docstring module
"""csv-data-worker — analyse et transformation de fichiers CSV (pandas)."""

# 2. Constante SYSTEM_PROMPT
SYSTEM_PROMPT: str = """..."""

# 3. Fonction manifest() au niveau module
def manifest() -> dict[str, Any]:
    return {
        "name": "csv-data-worker",
        "version": "0.1.0",
        ...
    }

# 4. Classe WorkerAgent + instance module-level
class CsvDataWorkerAgent(WorkerAgent):
    SYSTEM_PROMPT = SYSTEM_PROMPT
    MAX_STEPS = 8
    TEMPERATURE = 0.1

    def manifest(self) -> dict[str, Any]:
        return manifest()

    async def run(self, task, ctx):
        user_message = (
            task.get("input", {}).get("text", "")
            if isinstance(task.get("input"), dict)
            else str(task.get("input", ""))
        )
        result = await self.react(task, ctx, user_message)
        if isinstance(result, dict):
            return result
        return AIPResult.completed(result)

agent = CsvDataWorkerAgent()   # le runtime lit cet attribut — obligatoire
```

L'instance `agent` au niveau module est la seule interface que le runtime utilise. Le nom de la classe est libre.

---

## Le manifest — tous les champs

```python
def manifest() -> dict[str, Any]:
    return {
        # Identité
        "name": "csv-data-worker",
        "version": "0.1.0",
        "description": "Analyse, transformation et export de fichiers CSV. "
                        "Gère multi-encodage (UTF-8, latin-1), multi-séparateur. "
                        "Compatible modèles 7B+.",

        # Exécution
        "execution_mode": "direct",         # toujours "direct" pour les Workers

        # Outils
        "tools_required": ["python_executor", "file_read"],
        "tools_optional": ["file_write"],    # nécessaire uniquement pour l'export
        "tools_requiring_approval": [],      # vide si aucune validation humaine

        # Dépendances Python
        "packages": ["pandas>=2.0.0"],       # installés dans le venv de l'agent

        # Mémoire
        "memory_namespace": "csv-data-worker",

        # A2A — rend l'agent composable
        "supports_a2a": True,
        "skills": [
            {
                "id": "read-csv",
                "name": "Lire un CSV",
                "description": "Lit un fichier CSV et retourne son contenu "
                               "avec détection auto de l'encodage et du séparateur.",
                "input_modes": ["text"],
                "output_modes": ["text"],
                "input_schema": {
                    "file_path": {
                        "type": "string",
                        "description": "Chemin absolu ou relatif vers le fichier CSV",
                        "required": True,
                    }
                },
            },
            {
                "id": "analyze-csv",
                "name": "Analyser un CSV",
                "description": "Calcule statistiques descriptives, groupby, "
                               "et inspecte les types de colonnes d'un fichier CSV.",
                "input_modes": ["text"],
                "output_modes": ["text"],
            },
            {
                "id": "transform-csv",
                "name": "Transformer un CSV",
                "description": "Filtre, trie, pivote et exporte un fichier CSV "
                               "en CSV ou JSON.",
                "input_modes": ["text"],
                "output_modes": ["text", "json"],
            },
        ],

        # Métadonnées
        "tags": ["csv", "data", "pandas", "worker"],
        "max_concurrent_tasks": 1,
        "dangerous_tools_allowed": False,
    }
```

### MAX_STEPS et TEMPERATURE

`MAX_STEPS` détermine le nombre maximal d'itérations de la boucle ReAct avant abandon forcé. `TEMPERATURE` contrôle la variabilité des réponses du modèle :

| Complexité de la tâche | MAX_STEPS recommandé |
|---|---|
| Simple (1–2 appels d'outils) | 4–6 |
| Moyenne (3–5 appels) | 6–8 |
| Complexe (séquence de vérification) | 8–10 |

Pour les Worker Agents, garder `TEMPERATURE` entre 0.0 et 0.2 maximum. Au-delà, la variabilité peut faire ignorer les guardrails sur les modèles légers.

---

## Générer le squelette

```bash
apollia new csv-data-worker --type worker
```

Crée automatiquement :
- `agents/csv-data-worker.py` — squelette avec placeholders `# TODO`
- `agents/tests/test_csv_data_worker.py` — tests `test_manifest_is_valid` + `test_system_prompt_has_guardrails`

Vérifier que le squelette est importable avant de modifier quoi que ce soit :

```bash
python agents/csv-data-worker.py
pytest agents/tests/test_csv_data_worker.py -v
```
