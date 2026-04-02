# Skills et découverte

Un skill est l'unité d'exposition A2A d'un Worker Agent. C'est par son `skill_id` que les Directors l'invoquent, et c'est sa `description` que le runtime utilise pour le matching sémantique.

---

## Déclarer les skills dans le manifest

```python
def manifest(self):
    return {
        "name": "csv-data-worker",
        "version": "0.1.0",

        # Activer la visibilité A2A
        "supports_a2a": True,

        # Déclarer les skills exposés
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
                "description": "Filtre, trie, pivote et exporte un fichier CSV.",
                "input_modes": ["text"],
                "output_modes": ["text", "json"],
            },
        ],
    }
```

`supports_a2a: False` (défaut) rend l'agent **invisible** au router A2A — il ne peut pas être invoqué par d'autres agents.

---

## Les champs d'un AgentSkill

| Champ | Type | Obligatoire | Rôle |
|---|---|---|---|
| `id` | `str` | Oui | Identifiant machine pour le routing — `ctx.delegate("analyze-csv", ...)` |
| `name` | `str` | Oui | Libellé lisible — affiché dans la CLI, les logs et l'UI |
| `description` | `str` | Oui | Phrase complète — utilisée par le router A2A pour le matching sémantique |
| `input_modes` | `list[str]` | Oui | Modes d'entrée : `"text"`, `"file"`, `"json"` |
| `output_modes` | `list[str]` | Oui | Modes de sortie : `"text"`, `"json"` |
| `input_schema` | `dict` | Non | Schéma des paramètres nommés — aide le Director à construire le payload |

### Règle d'unicité des skill_id

Les `skill_id` doivent être **uniques dans l'ensemble des agents déployés**, pas seulement dans un agent. Si deux agents actifs déclarent le même `skill_id`, le runtime détecte le conflit au démarrage du second agent (`SkillConflict`) — pas à l'invocation.

```
apollia-os agent start agents/mon-worker.py
# ERREUR: SkillConflict — skill_id 'analyze-csv' déjà déclaré par csv-data-worker
#         Renommez le skill ou arrêtez l'agent en conflit.
```

Ce fail-fast (Principe #4) évite une ambiguïté silencieuse où un Director appelle le mauvais Worker.

### Description efficace

La `description` est l'interface sémantique du skill. Le router A2A l'utilise pour le matching — une description vague produit des invocations incorrectes.

| Description faible | Description efficace |
|---|---|
| "Analyse un CSV" | "Calcule statistiques descriptives (moyenne, médiane, écart-type), groupby par colonne catégorielle, et inspecte les types de colonnes d'un fichier CSV (UTF-8 ou latin-1)." |
| "Envoie un email" | "Envoie un email via SMTP avec pièce jointe optionnelle. Requiert destinataire, sujet et corps en texte ou HTML." |

---

## Découverte automatique — SkillIndex

Au démarrage de chaque agent avec `supports_a2a: True`, le runtime lit `manifest()["skills"]` et alimente le `SkillIndex` — un index inversé `skill_id → agent_name` intégré à l'`AgentRegistry` :

```
apollia-os agent start agents/csv-data-worker.py
  → AgentRegistry.Register(manifest)
  → SkillIndex.insert("read-csv"      → "csv-data-worker")
  → SkillIndex.insert("analyze-csv"   → "csv-data-worker")
  → SkillIndex.insert("transform-csv" → "csv-data-worker")
  → ProcessState → ACTIVE
```

Quand l'agent est arrêté, ses skills sont retirés de l'index :

```
apollia-os agent stop csv-data-worker
  → AgentRegistry.Unregister("csv-data-worker")
  → SkillIndex.remove("read-csv", "analyze-csv", "transform-csv")
```

La découverte est **locale et instantanée** — pas de découverte réseau, pas de DNS, pas de registre externe.

---

## Lister les agents A2A disponibles

```bash
apollia-os agent list --supports-a2a

# A2A-capable agents (4):
#   csv-data-worker  [Active]
#     - read-csv    : Lit et retourne le contenu d'un CSV
#     - analyze-csv : Statistiques descriptives, groupby
#     - transform-csv: Filtrer, trier, exporter
#   excel-worker  [Active]
#     - read-excel  : Lit et retourne le contenu d'un classeur Excel
#     - edit-excel  : Modifie des cellules et ajoute des lignes
#     - analyze-excel: Calcule totaux, moyennes, recherche
#   pdf-worker  [Active]
#     - read-pdf    : Extrait texte et métadonnées d'un PDF
#     - extract-tables: Extrait les tableaux en Markdown
#   code-worker  [Active]
#     - generate-code: Génère un fichier source Python ou Rust
#     - review-code : Retourne LGTM / SUGGESTION / ISSUE par ligne
```

---

## AgentCard A2A — exposition externe

Si au moins un agent actif déclare `supports_a2a: True`, Apollia OS expose une AgentCard conforme A2A à `/.well-known/agent.json` :

```bash
curl http://localhost:7771/.well-known/agent.json
```

```json
{
  "name": "csv-data-worker",
  "description": "Analyse et transformation de fichiers CSV (pandas).",
  "url": "http://localhost:7771",
  "skills": [
    {
      "id": "analyze-csv",
      "name": "Analyser un CSV",
      "inputModes": ["text"],
      "outputModes": ["text"]
    }
  ]
}
```

> Les manifests Python utilisent `snake_case` (`input_modes`). L'AgentCard JSON utilise `camelCase` (`inputModes`) conformément à la spec A2A. La conversion est automatique — vous n'avez rien à faire.

L'AgentCard permet à n'importe quel client A2A externe (autre runtime, outil tiers) de découvrir et invoquer vos Workers.

---

## Lister les skills via l'API REST

```bash
# Tous les skills disponibles
curl http://localhost:7771/api/v1/a2a/skills

# AgentCards de tous les agents A2A
curl http://localhost:7771/api/v1/a2a/agents

# AgentCard d'un agent spécifique
curl http://localhost:7771/api/v1/a2a/agents/csv-data-worker
```
