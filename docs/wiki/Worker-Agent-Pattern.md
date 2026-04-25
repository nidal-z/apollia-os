# Worker Agent Pattern — Guide pour builders

> Ce guide est la référence pour créer un Worker Agent Apollia OS de A à Z.
> Il couvre le concept, les critères de décision, l'anatomie, le guide pas-à-pas,
> le template scaffolding, les bonnes pratiques, le routing A2A, et les exemples.

---

## 1. Qu'est-ce qu'un Worker Agent ?

Un **Worker Agent** est un agent spécialisé dont l'expertise de domaine est **compilée dans le code Python** — pas injectée en contexte LLM à l'exécution.

L'expertise prend la forme d'un `SYSTEM_PROMPT` constant contenant :
- Les guardrails non-contournables (ce que l'agent ne doit **jamais** faire)
- Les imports corrects de la librairie cible
- Les patterns d'usage obligatoires (blocs de code Python de référence)
- La gestion des erreurs domaine (exceptions connues, messages clairs)
- Le format de réponse attendu

Un Worker Agent hérite de `WorkerAgent` (qui étend `BaseReActAgent`) et expose les mêmes deux méthodes contractuelles que tout agent Apollia : `manifest()` + `run()`.

### Agent générique vs Worker Agent

| Propriété | Agent générique (ORIA Direct) | Worker Agent |
|---|---|---|
| Expertise de domaine | Injectée dans le contexte à runtime | Compilée dans `SYSTEM_PROMPT` — toujours présente |
| Guardrails | Dans le prompt de la tâche ou `run()` | Dans `SYSTEM_PROMPT` — impossible à oublier |
| Compatibilité modèles légers | Dégradée (hallucinations API, guardrails oubliés) | Robuste — testé avec modèles 7B+ |
| Dépendances pip | Non supportées nativement | Déclarées dans `manifest["packages"]`, installées automatiquement |
| Réutilisabilité A2A | Optionnelle | Première classe — `supports_a2a: True` natif |
| Cas d'usage | Logique métier générale, orchestration | Domaine spécialisé (formats de fichier, API spécifique, langages) |

### Pourquoi "compiler" l'expertise ?

Sur les modèles frontier (Claude, GPT-4o), le LLM improvise correctement même sans instructions spécifiques. Sur les modèles légers 7–14B utilisés en local :

| Problème | Agent générique | Worker Agent |
|---|---|---|
| Hallucination API | `openpyxl.open_workbook` (inexistant) | Import correct dans le `SYSTEM_PROMPT` |
| Guardrails oubliés | "ne jamais bash" ignoré après 4 étapes | Règle compilée, toujours présente |
| Séquençage incorrect | `wb.save` oublié | Pattern obligatoire dans le prompt |
| Contexte saturé | 4K tokens → instructions tronquées | `SYSTEM_PROMPT` statique, jamais perdu |

---

## 2. Quand créer un Worker Agent ?

→ Voir aussi la [Matrice de décision — Capabilities](Decision-Matrix-Capabilities.md) pour l'arbre de décision complet.

Créer un Worker Agent si **au moins 2 des 3 conditions** suivantes sont réunies :

### Condition 1 — Séquence non-triviale

La tâche impose un ordre d'opérations critique que le LLM oublie régulièrement.

> Exemple positif : lecture d'un `.xlsx` → inspection des feuilles → calcul → `wb.save`. Chaque étape dépend de la précédente avec des contraintes `openpyxl` précises. Sur un modèle 7B, l'ordre peut être inversé et `save` oublié.

### Condition 2 — Guardrail critique

Il existe une façon dangereuse ou silencieusement incorrecte de faire la tâche que le LLM emprunte naturellement.

> Exemple positif : "Ne jamais modifier un `.xlsx` avec `bash_executor` — le format est une archive ZIP et toute écriture directe corrompt le fichier silencieusement." Cette règle dans un `SYSTEM_PROMPT` constant ne peut pas être ignorée.

### Condition 3 — Pattern d'erreur domaine récurrent

Le domaine produit des exceptions spécifiques avec des causes non-évidentes pour le LLM.

> Exemple positif : `zipfile.BadZipFile` pour Excel corrompu, `UnicodeDecodeError` pour CSV en `latin-1`, `PDFSyntaxError` pour PDF malformé.

### Règle de décision

```
2 conditions sur 3 → Worker Agent
moins de 2 → MCP Tool (ou ORIA Mode Direct si séquence légère)
```

### Tableau comparatif détaillé

| Situation | Mécanisme recommandé |
|---|---|
| Opération atomique, API simple, 1 appel | MCP Tool |
| Tâche multi-étapes avec ordre imposé | Worker Agent |
| Guardrail critique à respecter impérativement | Worker Agent |
| Librairie Python tierce nécessaire | Worker Agent (via `packages`) |
| Intégration d'un service externe (Slack, GitHub…) | MCP Tool |
| Format de fichier complexe (Excel, PDF, CSV encodé) | Worker Agent |
| Simple lecture/écriture fichier texte | MCP Tool (`file_read` / `file_write`) |
| Calcul pur sans I/O | MCP Tool ou `python_executor` direct |
| Expertise domaine réutilisable entre agents | Worker Agent |

---

## 3. Anatomie d'un Worker Agent

### Structure de fichier

Chaque Worker Agent est un fichier Python autonome dans `agents/` :

```
agents/
└── mon-worker.py        ← fichier unique de l'agent
agents/tests/
└── test_mon_worker.py   ← tests unitaires (conftest.py partagé)
```

### Composants du fichier agent

Un fichier Worker Agent complet contient 4 éléments dans cet ordre :

```python
# 1. Docstring module
"""mon-worker — description courte de l'agent."""

# 2. Constante SYSTEM_PROMPT
SYSTEM_PROMPT: str = """..."""

# 3. Fonction manifest() au niveau module
def manifest() -> dict[str, Any]:
    return {...}

# 4. Classe WorkerAgent + instance module-level
class MonWorkerAgent(WorkerAgent):
    SYSTEM_PROMPT = SYSTEM_PROMPT
    MAX_STEPS = 8
    TEMPERATURE = 0.1
    ...

agent = MonWorkerAgent()   # obligatoire — le runtime lit cet attribut
```

### Manifest — tous les champs

```python
def manifest() -> dict[str, Any]:
    return {
        # Identité
        "name": "mon-worker",          # kebab-case, unique dans le déploiement
        "version": "0.1.0",            # semver
        "description": "...",          # 1-3 phrases, formats + modèles supportés

        # Exécution
        "execution_mode": "direct",    # toujours "direct" pour les Workers

        # Outils
        "tools_required": ["python_executor", "file_read"],  # échec si absent
        "tools_optional": ["file_write", "file_list"],       # dégradé si absent
        "tools_requiring_approval": [],                      # HITL — liste vide si aucun

        # Dépendances Python
        "packages": ["ma-lib>=1.0.0"], # installés dans le venv de l'agent

        # Mémoire
        "memory_namespace": "mon-worker",  # clé d'isolation en mémoire

        # A2A
        "supports_a2a": True,          # False → invisible au router A2A
        "skills": [...],               # requis si supports_a2a: True

        # Métadonnées
        "tags": ["domaine", "worker"],
        "max_concurrent_tasks": 1,     # 1 = séquentiel (recommandé)
        "dangerous_tools_allowed": False,
    }
```

### Champs d'un skill A2A

```python
{
    "id": "analyze-csv",        # snake-case, unique dans l'ensemble des agents déployés
    "name": "Analyser un CSV",  # lisible, affiché dans l'UI et le CLI
    "description": "...",       # phrase complète — utilisée par le router pour le matching
    "input_modes": ["text"],    # modes supportés en entrée : "text", "file", "json"
    "output_modes": ["text"],   # modes supportés en sortie : "text", "json"
    # input_schema est optionnel — documente les paramètres nommés
    "input_schema": {
        "file_path": {
            "type": "string",
            "description": "Chemin absolu ou relatif vers le fichier",
            "required": True,
        },
    },
}
```

### Structure du SYSTEM_PROMPT

Un SYSTEM_PROMPT de Worker Agent suit une structure fixe en 4 sections :

```
## RÈGLES ABSOLUES (non-négociables)
Guardrails introduits par JAMAIS / TOUJOURS + RAISON immédiate

## IMPORTS STANDARDS (si librairie tierce)
Blocs d'imports exacts — évite les hallucinations de noms de modules

## PATTERNS OBLIGATOIRES
2–4 snippets Python pour les opérations les plus courantes

## GESTION DES ERREURS DOMAINE
Mapping exception → message utilisateur clair

## FORMAT DE RÉPONSE
Ce que la réponse finale doit toujours contenir
```

### Codes d'erreur domaine stables

Pour les erreurs domaine non-génériques, utiliser `domain_error` plutôt qu'une exception :

```python
# Dans le SYSTEM_PROMPT :
# - FileNotFoundError → domain_error("file_not_found", "Fichier introuvable : {path}")
# - Chiffrement PDF → domain_error("password_protected", "PDF protégé")
# - OCR nécessaire  → domain_error("scanned_pdf", "PDF scanné — OCR non supporté")
```

Les codes (`"file_not_found"`, `"password_protected"`, etc.) sont stables — ils peuvent être interceptés par un Director Agent via la réponse structurée.

---

## 4. Guide pas-à-pas

### Étape 1 — Générer le squelette

```bash
apollia new mon-agent --type worker
```

Crée automatiquement :
- `agents/mon-agent.py` — squelette Worker Agent avec placeholders
- `agents/tests/test_mon_agent.py` — tests de démarrage (manifest + guardrails)

### Étape 2 — Personnaliser le manifest

Ouvrir `agents/mon-agent.py` et remplir :

```python
def manifest() -> dict[str, Any]:
    return {
        "name": "mon-agent",          # ← remplacer si différent du nom du fichier
        "description": "...",         # ← décrire précisément le domaine et les formats
        "tools_required": ["python_executor", "file_read"],  # ← outils nécessaires
        "packages": ["ma-lib>=1.0.0"],  # ← dépendances pip du domaine
        "supports_a2a": True,
        "skills": [
            {
                "id": "main-skill",
                "name": "...",
                "description": "...",  # ← phrase complète utilisée pour le routing
                "input_modes": ["text"],
                "output_modes": ["text"],
            },
        ],
        ...
    }
```

**Pour choisir les outils :**
- `python_executor` → obligatoire si des `packages` sont déclarés
- `file_read` / `file_write` → accès aux fichiers locaux
- `file_edit` → modification partielle (ne pas écraser le fichier entier)
- `bash_executor` → uniquement si aucune librairie Python n'est disponible (code-worker)

### Étape 3 — Rédiger le SYSTEM_PROMPT

C'est l'étape la plus importante. Pour chaque guardrail :

1. Identifier l'erreur dangereuse ou silencieuse la plus probable dans ce domaine
2. Formuler la règle avec `JAMAIS` ou `TOUJOURS`
3. Ajouter immédiatement la `RAISON` (conséquence si la règle est violée)

```python
SYSTEM_PROMPT: str = """Tu es mon-agent, un agent expert de [domaine].

## RÈGLES ABSOLUES

1. N'utilise JAMAIS [outil ou méthode dangereuse] pour [opération].
   RAISON : [Ce qui se passe silencieusement si cette règle est violée].

2. Utilise TOUJOURS [pattern correct].
   RAISON : [Pourquoi l'alternative échoue].

## PATTERNS OBLIGATOIRES

### [Opération courante 1]
```python
# code exact ici
```

### [Opération courante 2]
```python
# code exact ici
```

## GESTION DES ERREURS DOMAINE

- `SomeSpecificError` → domain_error("error_code", "Message clair pour l'utilisateur")

## FORMAT DE RÉPONSE

- Toujours indiquer [information contextuelle obligatoire]
"""
```

### Étape 4 — Implémenter run

Dans la grande majorité des cas, le pattern hérité suffit sans modification :

```python
class MonAgentAgent(WorkerAgent):
    SYSTEM_PROMPT = SYSTEM_PROMPT
    MAX_STEPS = 8      # ajuster selon la complexité — voir section 6
    TEMPERATURE = 0.1  # garder bas — voir section 6

    def manifest(self) -> dict[str, Any]:
        return manifest()

    async def run(self, task: dict[str, Any], ctx: Any) -> dict[str, Any]:
        user_message = (
            task.get("input", {}).get("text", "")
            if isinstance(task.get("input"), dict)
            else str(task.get("input", ""))
        )
        result = await self.react(task, ctx, user_message)
        if isinstance(result, dict):
            return result
        return AIPResult.completed(result)


agent = MonAgentAgent()
```

Surcharger `run()` uniquement si une logique de pré/post-traitement est nécessaire avant le ReAct loop (ex. validation du format du fichier d'entrée, enrichissement de la tâche).

### Étape 5 — Écrire les tests

Trois tests minimaux à implémenter dans `agents/tests/test_mon_agent.py` :

```python
import importlib.util
from pathlib import Path
import pytest
from conftest import MockCtx, MockTools, MockLlm
import json

_AGENT_PATH = Path(__file__).parent.parent / "mon-agent.py"
spec = importlib.util.spec_from_file_location("mon_agent", _AGENT_PATH)
assert spec and spec.loader
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)


def test_manifest_is_valid():
    m = mod.manifest()
    assert m["name"] == "mon-agent"
    assert "python_executor" in m["tools_required"]
    assert m["supports_a2a"] is True
    assert len(m["skills"]) >= 1


def test_system_prompt_has_guardrails():
    prompt = mod.SYSTEM_PROMPT
    assert "JAMAIS" in prompt or "TOUJOURS" in prompt
    assert "RAISON" in prompt


@pytest.mark.asyncio
async def test_happy_path():
    tools = MockTools()
    tools.set_result("python_executor", {"stdout": "résultat", "stderr": "", "exit_code": 0})
    llm = MockLlm([
        json.dumps({
            "thought": "Je vais utiliser python_executor",
            "action": "tool_call",
            "tool": "python_executor",
            "args": {"code": "print('ok')", "timeout_secs": 30},
        }),
        json.dumps({
            "thought": "Terminé",
            "action": "final_answer",
            "text": "Résultat : résultat",
        }),
    ])
    ctx = MockCtx(tools=tools, llm=llm)

    result = await mod.agent.run({"input": {"text": "Tâche test"}}, ctx)

    assert result["status"] == "completed"
    assert tools.called_with("python_executor")
```

### Étape 6 — Démarrer et tester en live

```bash
# Démarrer l'agent
apollia-os agent start agents/mon-agent.py

# Vérifier qu'il est actif
apollia-os agent list

# Envoyer une tâche de test
apollia-os agent run mon-agent "Teste avec un fichier simple"

# Arrêter
apollia-os agent stop mon-agent
```

---

## 5. Template `apollia new --type worker`

### Usage

```bash
# Créer un nouveau Worker Agent
apollia new mon-agent --type worker

# Exemples de noms courants
apollia new sql-worker --type worker
apollia new image-worker --type worker
apollia new json-validator --type worker
```

### Fichiers générés

| Fichier | Contenu |
|---|---|
| `agents/mon-agent.py` | Squelette complet avec `SYSTEM_PROMPT`, `manifest()`, classe `WorkerAgent` et `agent =...` |
| `agents/tests/test_mon-agent.py` | Tests `test_manifest_is_valid` + `test_system_prompt_has_guardrails` |

### Points à personnaliser dans le squelette généré

Le template génère des placeholders à remplacer, marqués `# TODO` :

| Placeholder | Emplacement | Quoi mettre |
|---|---|---|
| `mon-agent` | `manifest["name"]` | Nom kebab-case du domaine |
| `"..."` | `manifest["description"]` | Description précise (formats gérés, LLMs supportés) |
| `["ma-lib>=1.0.0"]` | `manifest["packages"]` | Dépendances pip réelles du domaine |
| `"main-skill"` | `manifest["skills"][0]["id"]` | Skill principal du domaine |
| Section RÈGLES ABSOLUES | `SYSTEM_PROMPT` | Guardrails réels du domaine |
| Section PATTERNS OBLIGATOIRES | `SYSTEM_PROMPT` | Snippets d'usage exacts de la librairie |

### Vérifier que le squelette généré fonctionne

```bash
# L'agent doit être importable sans erreur
python agents/mon-agent.py

# Les tests de base doivent passer
pytest agents/tests/test_mon-agent.py -v
```

---

## 6. Bonnes pratiques

### Guardrails — ce qui les rend efficaces

Un guardrail efficace combine trois éléments :

```
1. Verbe fort    : JAMAIS / TOUJOURS / NE PAS
2. Règle précise : l'outil interdit + l'opération spécifique
3. RAISON        : la conséquence réelle si la règle est violée
```

| Guardrail faible | Guardrail efficace |
|---|---|
| "Évite bash si possible" | "N'utilise JAMAIS bash_executor sur un `.xlsx`. RAISON : `.xlsx` est une archive ZIP — bash corrompt l'archive silencieusement." |
| "Sauvegarde après modification" | "Appelle TOUJOURS `wb.save(path)` après modification. RAISON : sans `save`, les changements ne sont jamais écrits sur disque." |
| "Attention aux CSVs encodés" | "Essaie TOUJOURS UTF-8 puis latin-1 si UnicodeDecodeError. RAISON : les CSVs Excel Windows français sont en latin-1, pas UTF-8." |

Les guardrails sont **testables statiquement** sans exécuter l'agent :

```python
def test_guardrail_bash_forbidden():
    assert "JAMAIS bash" in mod.SYSTEM_PROMPT or "JAMAIS bash_executor" in mod.SYSTEM_PROMPT
```

### MAX\_STEPS — calibrer le budget de l'agent

`MAX_STEPS` est le nombre maximal d'itérations du ReAct loop avant abandon forcé. Une valeur trop basse tronque les tâches complexes ; trop haute laisse l'agent boucler.

| Complexité de la tâche | MAX_STEPS recommandé | Exemple |
|---|---|---|
| Simple (1–2 appels d'outils) | 4–6 | lecture d'un fichier, extraction de texte |
| Moyenne (3–5 appels) | 6–8 | analyse + transformation + export |
| Complexe (séquence de vérification) | 8–10 | génération code + vérification syntaxe + correction |

Tous les agents built-in utilisent `MAX_STEPS = 8` sauf `code-worker` (`MAX_STEPS = 10`) qui inclut une étape de vérification syntaxe/compilation.

### TEMPERATURE — favoriser le déterminisme

Les Worker Agents opèrent sur des formats structurés où la variabilité est un défaut. Utiliser des valeurs basses :

| Valeur | Usage |
|---|---|
| `0.0` | Opérations purement déterministes (calcul, extraction, validation) |
| `0.1` | Valeur par défaut pour les Workers — permet une légère adaptation sans variabilité excessive |
| `0.2` | Maximum recommandé pour un Worker Agent |
| `> 0.2` | Déconseillé — la variabilité peut faire ignorer les guardrails sur modèles légers |

### Packages pip — bonnes pratiques

```python
"packages": ["openpyxl>=3.1.0"],          # version minimale — pas de lock
"packages": ["pandas>=2.0.0"],             # spécifier la version majeure minimum
"packages": ["requests>=2.31.0", "lxml"],  # plusieurs packages possibles
"packages": [],                            # si aucune dépendance tierce (code-worker)
```

Règles :
- Utiliser `>=` — ne pas bloquer les mises à jour de sécurité
- Ne déclarer que les packages **non-inclus** dans la stdlib Python
- Si l'installation échoue, l'agent démarre en état `DEGRADED` (non bloquant)
- Licence : vérifier la compatibilité avant d'ajouter un package (ex. PyMuPDF est AGPL — préférer `pdfplumber` MIT)

### Tests — couverture minimale

| Type de test | Ce qu'il vérifie | Outil |
|---|---|---|
| `test_manifest_is_valid` | Champs obligatoires, skills, supports_a2a | `manifest()` uniquement |
| `test_system_prompt_has_guardrails` | Présence de guardrails avec RAISON | Inspection texte SYSTEM_PROMPT |
| `test_happy_path` | Le ReAct loop s'exécute et retourne `status: completed` | `MockCtx` + `MockLlm` |
| `test_domain_error_*` | Les erreurs domaine retournent un résultat valide (pas d'exception) | `MockTools` avec stderr |

Les tests domaine avec de **vrais fichiers** utilisent les fixtures de `conftest.py` :
- `excel_file` → `.xlsx` valide avec 10 lignes
- `csv_utf8_file` → CSV UTF-8 avec séparateur virgule
- `csv_latin1_file` → CSV latin-1 (simule export Excel Windows)
- `pdf_file` → PDF minimaliste 1 page

---

## 7. A2A — rendre son agent composable

### Déclarer la compatibilité A2A dans le manifest

```python
"supports_a2a": True,    # obligatoire — False → invisible au router A2A
"skills": [
    {
        "id": "analyze-csv",            # ← utilisé pour le routing : ctx.delegate("analyze-csv", ...)
        "name": "Analyser un CSV",      # ← affiché dans CLI et UI
        "description": "Analyse un fichier CSV et retourne statistiques et types de colonnes.",
        "input_modes": ["text"],        # "text" | "file" | "json"
        "output_modes": ["text"],       # "text" | "json"
    },
],
```

**Chaque champ de `AgentSkill` :**

| Champ | Type | Rôle |
|---|---|---|
| `id` | `str` | Identifiant machine utilisé par `ctx.delegate(skill_id,...)` — doit être unique dans l'ensemble des agents déployés |
| `name` | `str` | Libellé lisible affiché dans l'UI, la CLI, et les logs |
| `description` | `str` | Phrase complète utilisée par le router A2A pour le matching sémantique — être précis sur les inputs/outputs |
| `input_modes` | `list[str]` | Modes d'entrée supportés (`"text"` = description textuelle, `"file"` = chemin de fichier, `"json"` = payload structuré) |
| `output_modes` | `list[str]` | Modes de sortie produits |
| `input_schema` | `dict` (optionnel) | Schéma des paramètres nommés — aide le Director à construire le payload |

### Appel depuis un Director Agent

```python
async def run(self, task, ctx):
    # Délégation par skill_id
    result = await ctx.delegate(
        "analyze-csv",
        {"input": {"text": "Analyse /data/ventes.csv"}},
        timeout_secs=120,
    )
    # result : {"status": "completed", "output": [...], ...}
    return AIPResult.completed(f"Analyse terminée : {result}")
```

Ou via le helper `WorkerAgent.delegate_skill` depuis un autre Worker Agent :

```python
result = await self.delegate_skill(ctx, "analyze-csv", payload)
```

### Comment l'agent est découvert et invoqué

1. Au démarrage, le runtime lit `manifest["skills"]` de chaque agent actif avec `supports_a2a: True`
2. Il construit un index `skill_id → agent_name`
3. Quand `ctx.delegate("analyze-csv", payload)` est appelé, le runtime résout l'agent et lui envoie la tâche via A2A
4. Le résultat est retourné synchrone au Director (avec timeout configurable)

### Trust model mémoire

En composition A2A, le trust model appliqué à la mémoire est :

| Opération | Portée |
|---|---|
| Lecture mémoire | **Globale** — un Worker Agent invoqué via A2A peut lire les entrées de n'importe quel namespace |
| Écriture mémoire | **Namespace propre uniquement** — un Worker Agent ne peut écrire que dans `manifest["memory_namespace"]` |

Ce modèle permet au Director de partager du contexte en mémoire avec le Worker (lecture), sans que le Worker puisse polluer l'espace mémoire d'autres agents (écriture isolée).

### Erreurs possibles à l'invocation

| Situation | Erreur |
|---|---|
| Skill non trouvé | `RuntimeError: skill 'X' not found — available: [...]` |
| Skill déclaré par 2+ agents actifs | `RuntimeError: ambiguous skill 'X' — declared by: [A, B]` |
| Timeout dépassé | `RuntimeError: delegation timed out after N seconds` |
| `supports_a2a: False` dans manifest | `RuntimeError: A2A delegation requires supports_a2a: true` |

### CLI — lister les agents A2A disponibles

```bash
apollia-os agent list --supports-a2a
# A2A-capable agents (6):
#   csv-data-worker  [Active]
#     - read-csv: Lit et retourne le contenu d'un CSV
#     - analyze-csv: Statistiques descriptives, groupby
#     - transform-csv: Filtrer, trier, exporter
#   excel-worker  [Active]
#     - read-excel: Lit et retourne le contenu d'un classeur Excel
#     - edit-excel: Modifie des cellules, ajoute des lignes
#     - analyze-excel: Calcule totaux, moyennes, recherche
#   pdf-worker  [Active]
#     - read-pdf: Extrait texte et métadonnées d'un PDF
#     - extract-text: Extrait le texte d'une plage de pages
#     - extract-tables: Extrait les tableaux en Markdown
#   code-worker  [Active]
#     - generate-code: Génère un fichier source (Python ou Rust)
#     - refactor-code: Améliore la structure sans changer le comportement
#     - review-code: Retourne LGTM / SUGGESTION / ISSUE par ligne
#   sql-worker  [Active]
#     - query-sql: Exécute une requête SELECT sur une base SQLite
#     - schema-inspect: Inspecte le schéma d'une base SQLite
#     - data-export: Exporte les résultats en CSV ou JSON
#   git-worker  [Active]
#     - git-status: Affiche l'état du dépôt Git
#     - git-diff: Affiche les modifications en cours
#     - git-commit: Crée un commit conventionnel
```

---

## 8. Exemples — les 6 agents disponibles

### 8.1 Agents bundled (distribués avec le runtime)

| Agent | Domaine | Package requis | Skills | Guardrail central |
|---|---|---|---|---|
| [`excel-worker`](../../agents/bundled/excel-worker.py) | Fichiers Excel `.xlsx` / `.xlsm` | `openpyxl>=3.1.0` | `read-excel`, `edit-excel`, `analyze-excel` | Jamais `bash_executor` sur `.xlsx` (archive ZIP) |
| [`csv-data-worker`](../../agents/bundled/csv-data-worker.py) | Fichiers CSV (multi-encodage, multi-séparateur) | `pandas>=2.0.0` | `read-csv`, `analyze-csv`, `transform-csv` | Toujours détecter l'encodage et inspecter `dtypes` avant calcul |
| [`pdf-worker`](../../agents/bundled/pdf-worker.py) | Documents PDF | `pdfplumber>=0.10.0` | `read-pdf`, `extract-text`, `extract-tables` | Jamais `bash` sur PDF ; chunking auto > 50 pages ; pas de crack de mot de passe |
| [`code-worker`](../../agents/bundled/code-worker.py) | Génération, refactoring, revue de code (Python + Rust) | aucun | `generate-code`, `refactor-code`, `review-code` | Toujours `file_read` avant `file_write` ; vérification syntaxe/compilation obligatoire |

Les 4 agents bundled sont auto-installés au premier démarrage du runtime via `agents/bundled/manifest.json`. Si un agent est déjà installé, il n'est pas réinstallé (idempotence).

### 8.2 Agents communautaires (installables séparément)

| Agent | Domaine | Package requis | Skills | Guardrail central |
|---|---|---|---|---|
| [`sql-worker`](../../agents/community/sql-worker.py) | Bases de données SQLite | aucun (sqlite3 stdlib) | `query-sql`, `schema-inspect`, `data-export` | SELECT uniquement par défaut ; paramétrage `?` obligatoire (anti-injection SQL) |
| [`git-worker`](../../agents/community/git-worker.py) | Versioning Git | aucun (bash_executor) | `git-status`, `git-diff`, `git-commit` | Jamais `push --force`, `reset --hard`, `clean -fd`, `branch -D` ; commits conventionnels obligatoires |

Ces agents servent de template pour la communauté. Ils sont installés via :

```bash
$ apollia-os agent install agents/community/sql-worker.py
  → Validation du manifest...
  ✔ Manifest valide (name: sql-worker, version: 0.1.0)
  → Scan dangerous_tools_allowed...
  ✔ Aucun outil dangereux déclaré
  ✔ Agent "sql-worker" installé

$ apollia-os agent install agents/community/git-worker.py --skip-tests
  ⚠ Tests ignorés (--skip-tests)
  ✔ Agent "git-worker" installé
```

Pour créer un agent communautaire : voir [Community Agent Registry](./Community-Agent-Registry).

### `excel-worker`

Spécialité : manipulation de classeurs Excel via openpyxl. Guardrail central : n'utilise jamais `bash_executor` pour lire ou modifier un `.xlsx` (un `.xlsx` est une archive ZIP — bash corromprait silencieusement l'archive). Inspecte toujours `wb.sheetnames` avant d'accéder à une feuille.

```bash
apollia-os agent start agents/bundled/excel-worker.py
apollia-os agent run excel-worker "Analyse la feuille Ventes de /data/rapport.xlsx"
```

### `csv-data-worker`

Spécialité : analyse et transformation de CSVs via pandas. Gère automatiquement la détection d'encodage (UTF-8, latin-1, utf-8-sig) et de séparateur (`,` ou `;`). Guardrail central : inspecter `df.dtypes` avant tout calcul numérique — une colonne lue comme `object` ne peut pas être sommée directement.

```bash
apollia-os agent start agents/bundled/csv-data-worker.py
apollia-os agent run csv-data-worker "Calcule le total de la colonne CA dans /data/ventes.csv"
```

### `pdf-worker`

Spécialité : extraction de texte, métadonnées et tableaux depuis des PDFs via pdfplumber (licence MIT). Gère les PDFs multi-pages (chunking auto au-delà de 50 pages), détecte les PDFs protégés par mot de passe (erreur structurée `password_protected`) et les PDFs scannés (erreur `scanned_pdf` — OCR non supporté en V1).

```bash
apollia-os agent start agents/bundled/pdf-worker.py
apollia-os agent run pdf-worker "Extrais le texte des pages 1 à 10 de /data/contrat.pdf"
```

### `code-worker`

Spécialité : génération, refactoring et revue de code source Python et Rust. N'utilise pas de packages pip — s'appuie sur `bash_executor`, `file_read`, `file_write`, `file_edit`. Guardrail central : toujours lire un fichier avant de l'écrire (`file_read` → `file_write`). Vérifie la syntaxe Python (`ast.parse`) et la compilation Rust (`cargo check`) après toute génération. Revue structurée en LGTM / SUGGESTION / ISSUE.

```bash
apollia-os agent start agents/bundled/code-worker.py
apollia-os agent run code-worker "Génère une classe Python pour valider des adresses email, avec tests"
```

### `sql-worker` (communautaire)

Spécialité : interrogation de bases SQLite locales. SELECT uniquement par défaut — INSERT/UPDATE/DELETE nécessitent `dangerous_tools_allowed: True` dans le manifest. Guardrail central : paramétrage `?` obligatoire pour toutes les requêtes (protection contre l'injection SQL), jamais de f-string dans les requêtes. Timeout 30s par requête, validation existence + intégrité du fichier SQLite à la connexion.

```bash
apollia-os agent install agents/community/sql-worker.py
apollia-os agent run sql-worker "Liste les clients dont le CA dépasse 10000"
```

### `git-worker` (communautaire)

Spécialité : opérations Git en lecture et commit. Bloque les opérations destructives (`push --force`, `reset --hard`, `clean -fd`, `branch -D`, `checkout --.`). Guardrail central : commits conventionnels obligatoires (`type(scope): description`), `git status` systématique avant tout commit, pas de `git add.` sans inspection. Opérations distantes (push, pull, fetch) interdites sans approbation.

```bash
apollia-os agent install agents/community/git-worker.py
apollia-os agent run git-worker "Montre-moi les modifications en cours et committe-les"
```

### `browser-worker` (communautaire)

Spécialité : navigation web et capture d'écran via Playwright. Skills A2A : `browse-url`, `screenshot-url`. Packages pip : `playwright`, `pillow`. Guardrail central : validation de l'URL avant navigation (schéma `http`/`https` uniquement), timeout par page (défaut 30s), screenshots dans un répertoire temporaire isolé. Installation depuis Git :

```bash
apollia-os agent install https://github.com/apollia-os/browser-worker.git
apollia-os agent run browser-worker "Prends une capture d'écran de https://example.com"
```

### `email-worker` (communautaire)

Spécialité : envoi et lecture d'emails via SMTP/IMAP. Skills A2A : `send-email` (HITL — approbation opérateur requise), `read-inbox`. Packages pip : stdlib Python (`smtplib`, `imaplib`). Guardrail central : `send-email` est une action HITL non-contournable — aucun email n'est envoyé sans confirmation explicite. Validation des adresses email avant soumission.

```bash
apollia-os agent install https://github.com/apollia-os/email-worker.git
apollia-os agent run email-worker "Envoie un rapport hebdomadaire à admin@acme.com"
```

### `slack-worker` (communautaire)

Spécialité : intégration Slack — envoi de messages et lecture de canaux. Skills A2A : `send-message` (HITL), `read-channel`. Packages pip : `slack-sdk`. Guardrail central : `send-message` est une action HITL — confirmation opérateur avant envoi. `read-channel` en lecture seule, aucune modification de canal. Token Slack lu depuis `SLACK_BOT_TOKEN` (jamais hardcodé).

```bash
apollia-os agent install https://github.com/apollia-os/slack-worker.git
apollia-os agent run slack-worker "Résume les messages #sales de cette semaine"
```

---

## Checklist avant de soumettre un Worker Agent

- [ ] `SYSTEM_PROMPT` contient au moins 1 guardrail avec `RAISON`
- [ ] `manifest["packages"]` liste toutes les dépendances pip (vide si aucune)
- [ ] `manifest["tools_required"]` contient `python_executor` si des packages sont utilisés
- [ ] `MAX_STEPS` est défini (valeur conseillée : 6–10 selon la complexité)
- [ ] `TEMPERATURE` est bas (0.0–0.2)
- [ ] `agent = MonWorkerAgent` est déclaré au niveau du module (requis par le runtime)
- [ ] Tests : manifest valide, guardrail vérifié, happy path, au moins 1 cas d'erreur domaine
- [ ] Si `supports_a2a: True` : au moins 1 skill déclaré dans `manifest["skills"]`
- [ ] Si `supports_a2a: True` : `skill_id` unique dans l'ensemble des agents déployés
- [ ] `pytest agents/tests/test_mon-agent.py` passe sans erreur

---

## Références

- [Matrice de décision — Capabilities](Decision-Matrix-Capabilities.md)
- [ADR-048 — Worker Agents : expertise de domaine compilée](../adr/ADR-048-worker-agents-expertise-domaine.md)
- [ADR-049 — Routing A2A inter-agents](../adr/ADR-049-a2a-routing-inter-agents.md)
- [ADR-050 — Distribution Worker Agents](../adr/ADR-050-distribution-worker-agents.md)
- [Community Agent Registry](Community-Agent-Registry.md)
- [Benchmark : Worker Agent vs generic-agent](../benchmarks/worker-agent-benchmark.md)
- [Guide SDK Agent](Agents-SDK-Guide.md)
- [RuntimeContext guide](Agents-RuntimeContext-Guide.md)
