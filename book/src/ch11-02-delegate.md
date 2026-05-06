# Déléguer à un Worker

`ctx.delegate` est l'interface principale de délégation A2A en Mode Direct. Le Director appelle un Worker par son `skill_id`, attend son résultat, et continue.

---

## Syntaxe de base

```python
result = await ctx.delegate(
    "analyze-csv",                                    # skill_id du Worker
    {"input": {"text": "Analyse /data/ventes.csv"}},  # payload AIPTask
    timeout_secs=120,                                  # défaut : 120s
)
```

- `skill_id` : l'identifiant du skill à invoquer — doit correspondre exactement à l'`id` déclaré dans le manifest d'un Worker actif
- `payload` : un dict avec la même structure qu'une `AIPTask` normale
- `timeout_secs` : timeout de cette invocation individuelle

Le résultat est un dict avec la même structure qu'un `AIPResult` :

```python
# Résultat d'une invocation réussie
{
    "status": "completed",
    "output": [{"type": "text", "text": "2 lignes, colonnes : region, montant..."}]
}

# Résultat d'une invocation échouée
{
    "status": "failed",
    "error": {"code": "file_not_found", "message": "Fichier introuvable : /data/ventes.csv"}
}
```

---

## Exemple complet — Director Agent

```python
class RapportDirectorAgent:
    """Director Agent qui coordonne trois Workers pour produire un rapport."""

    def manifest(self):
        return {
            "name": "rapport-director",
            "version": "1.0.0",
            "tools_required": [],   # le Director lui-même n'a pas besoin d'outils natifs
            "supports_a2a": False,  # ce Director n'est pas invocable par d'autres agents
        }

    async def run(self, task, ctx):
        rapport_path = task["input"]["text"]

        # Étape 1 — extraire le texte du PDF
        pdf_result = await ctx.delegate(
            "read-pdf",
            {"input": {"text": f"Extrais le texte de {rapport_path}"}},
            timeout_secs=60,
        )
        if pdf_result["status"] == "failed":
            return AIPResult.failed("PDF_ERROR", pdf_result["error"]["message"])

        texte = pdf_result["output"][0]["text"]

        # Étape 2 — analyser les données CSV annexées
        csv_result = await ctx.delegate(
            "analyze-csv",
            {"input": {"text": f"Analyse les données dans {rapport_path}.csv"}},
            timeout_secs=90,
        )

        # Étape 3 — générer le code de visualisation
        code_result = await ctx.delegate(
            "generate-code",
            {"input": {"text": f"Génère un script Python pour visualiser : {csv_result}"}},
        )

        return AIPResult.completed(
            f"Rapport analysé.\n\nRésumé PDF :\n{texte}\n\n"
            f"Données :\n{csv_result['output'][0]['text']}\n\n"
            f"Visualisation :\n{code_result['output'][0]['text']}"
        )


agent = RapportDirectorAgent()
```

---

## Trust model mémoire

En composition A2A, les règles d'accès à la mémoire sont asymétriques :

| Opération | Portée |
|---|---|
| **Lecture** mémoire par le Worker | Globale — le Worker peut lire les entrées de n'importe quel namespace |
| **Écriture** mémoire par le Worker | Namespace propre uniquement — confiné à `manifest["memory_namespace"]` |

Ce modèle permet au Director de partager du contexte en mémoire avec le Worker (le Worker lit le namespace du Director), sans que le Worker puisse polluer l'espace mémoire d'autres agents.

Si un Worker tente d'écrire dans un namespace qui n'est pas le sien, l'écriture est redirigée vers son propre namespace avec un `WARN` dans les logs.

---

## Gérer les erreurs A2A

| Situation | Erreur retournée |
|---|---|
| Skill non trouvé dans l'index | `skill 'X' not found — available: [...]` |
| Même skill déclaré par 2+ agents actifs | `ambiguous skill 'X' — declared by: [A, B]` |
| Timeout de l'invocation dépassé | `delegation timed out after N seconds` |
| Worker a `supports_a2a: False` | `A2A delegation requires supports_a2a: true` |
| Worker échoue (erreur domaine) | `result["status"] == "failed"` — gérer dans le code |

```python
result = await ctx.delegate("analyze-csv", payload)

if result["status"] == "failed":
    code = result["error"]["code"]
    if code == "file_not_found":
        return AIPResult.failed("MISSING_DATA", "Le fichier CSV source est introuvable.")
    if code == "empty_file":
        return AIPResult.completed("Aucune donnée à analyser — fichier vide.")
    # Erreur inattendue — propager
    return AIPResult.failed(code, result["error"]["message"])
```

---

## Les trois garde-fous automatiques

Les garde-fous A2A sont appliqués par le runtime à **chaque** invocation, sans exception. Ils protègent contre les scénarios d'abus les plus courants dans les architectures multi-agents.

### Nombre maximal de hops (`max_hops`)

```
Director (delegation_chain=[])
  └── csv-data-worker (delegation_chain=["director"])
        └── autre-worker (delegation_chain=["director", "csv-data-worker"])
              └── encore-un-worker (delegation_chain=["director", "csv-data-worker", "autre-worker"])
                    └── ...
                          └── BLOQUÉ — MaxHopsExceeded (len >= max_hops=5)
```

L'algorithme détecte également les cycles : si l'agent cible figure déjà dans `delegation_chain`, `CycleDetected` est retourné immédiatement, indépendamment du nombre de hops.

Configurable dans `apollia.toml` : `[a2a] max_hops = 5` (défaut).

### Self-invocation bloquée

Un agent ne peut pas s'invoquer lui-même via A2A, directement ou indirectement. Cette protection empêche les boucles infinies immédiates.

```python
# Dans csv-data-worker — BLOQUÉ
result = await ctx.delegate("analyze-csv", payload)
# RuntimeError: SelfInvocation — agent 'csv-data-worker' cannot invoke itself
```

Configurable dans `apollia.toml` :

```toml
[a2a]
max_hops                = 5    # nombre maximal de hops dans la chaîne
invocation_timeout_secs = 120  # timeout par invocation individuelle
```

Chaque déclenchement de garde-fou émet un `RuntimeEvent::A2AGuardTriggered` sur l'EventBus avec `guard_type`, `caller`, `skill_id` et `detail` — observable via les notifications et l'audit log.

---

## ctx.a2a_invoke — variante bas niveau

`ctx.delegate` est un alias de haut niveau. La variante bas niveau `ctx.a2a_invoke` offre le même comportement avec un contrôle plus explicite :

```python
result = await ctx.a2a_invoke(
    "read-excel",
    {"text": "Lis ventes.xlsx"},
    timeout_secs=60,
)
```

Dans la pratique, `ctx.delegate` est recommandé — plus lisible, même sémantique.
