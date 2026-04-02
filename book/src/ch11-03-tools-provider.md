# A2AToolsProvider — Workers comme outils ORIA

`ctx.delegate()` est explicite : le Director sait quel Worker appeler, à quel moment, avec quel payload. C'est le bon choix quand le workflow est connu à l'avance.

Mais un Director qui utilise `ctx.llm.run_tools()` délègue son raisonnement au LLM — et le LLM, lui, ne sait pas quels Workers sont disponibles. Il voit uniquement les outils natifs que vous lui décrivez.

L'`A2AToolsProvider` résout ce problème en injectant automatiquement les skills A2A comme des outils virtuels dans la boucle ReAct d'ORIA. Le LLM voit `a2a:analyze-csv` exactement comme il voit `file_read` ou `python_executor` — et peut décider seul de l'appeler.

---

## Comment ça fonctionne

À chaque démarrage de tâche, l'`ExecutionCoordinator` construit la liste d'outils disponibles pour l'agent en combinant deux sources :

```
ExecutionCoordinator.start_task()
  ├── ToolRegistry        → outils natifs (file_read, python_executor, ...)
  └── A2AToolsProvider    → skills A2A actifs (a2a:read-csv, a2a:analyze-csv, ...)
                              → ToolDescriptor pour chaque skill
```

Chaque skill actif devient un `ToolDescriptor` avec :

- **name** : `"a2a:{skill_id}"` — ex: `"a2a:analyze-csv"`
- **description** : `"{skill_description} (via {agent_name})"` — ex: `"Calcule statistiques descriptives... (via csv-data-worker)"`

Le LLM reçoit cette liste augmentée et peut invoquer n'importe quel outil — natif ou A2A — en utilisant son nom.

---

## Routing transparent

L'`ToolExecutor` distingue les outils A2A des outils natifs par leur préfixe :

```
ctx.tools.call("file_read", {...})
  → ToolExecutor natif — exécution locale

ctx.tools.call("a2a:analyze-csv", {"text": "Analyse /data/ventes.csv"})
  → Préfixe "a2a:" détecté
  → A2AInvoker.invoke("analyze-csv", payload)
  → csv-data-worker [Active]
  → résultat retourné comme output d'outil
```

Du point de vue du LLM, il n'y a aucune différence entre les deux appels — même interface, même format de résultat.

---

## Pattern Director autonome

Voici un Director Agent qui utilise l'`A2AToolsProvider` : il ne connaît pas à l'avance quels Workers utiliser — le LLM décide en fonction des outils disponibles et de la tâche.

```python
class DirectorAutonome:
    """Director Agent qui délègue automatiquement aux Workers disponibles."""

    def manifest(self):
        return {
            "name": "director-autonome",
            "version": "1.0.0",
            "description": "Analyse des documents en déléguant aux Workers spécialisés.",
            "tools_required": [],   # pas d'outils natifs requis — utilise les Workers A2A
            "step_budget": {"max_steps": 20, "max_tool_calls": 30},
        }

    async def run(self, task, ctx):
        if ctx.llm is None:
            return AIPResult.failed("LLM_UNAVAILABLE", "Aucun backend LLM configuré.")

        # Les outils A2A sont déjà injectés automatiquement dans ctx.tools
        # Le LLM voit : a2a:read-csv, a2a:analyze-csv, a2a:read-pdf, a2a:review-code, ...
        available_tools = await ctx.tools.list_descriptors()

        result = await ctx.llm.run_tools(
            messages=[
                {
                    "role": "system",
                    "content": (
                        "Tu es un agent d'analyse documentaire. "
                        "Pour chaque document fourni, utilise l'outil A2A le plus adapté "
                        "à son format. Les outils a2a:* délèguent à des agents spécialisés."
                    ),
                },
                {"role": "user", "content": task["input"]["text"]},
            ],
            tools=available_tools,
            max_iterations=10,
        )

        return AIPResult.completed(result.content)


agent = DirectorAutonome()
```

Quand l'utilisateur demande "Analyse /data/rapport.pdf et /data/ventes.csv", le LLM choisit automatiquement `a2a:read-pdf` pour le PDF et `a2a:analyze-csv` pour le CSV — sans que vous ayez codé cette logique.

---

## Backward-compatibility

Si aucun agent A2A n'est actif, aucun outil `a2a:` n'apparaît dans la liste — le comportement du Director est identique à un agent sans A2A. L'`A2AToolsProvider` ne fait rien quand `SkillIndex` est vide.

Les agents existants qui n'utilisent pas A2A ne sont pas affectés.

---

## Profondeur propagée

Quand le LLM invoque `a2a:analyze-csv` via l'`A2AToolsProvider`, le compteur `a2a_depth` est incrémenté exactement comme lors d'un `ctx.delegate()` explicite. Les trois garde-fous (profondeur max, self-invocation, chain timeout) s'appliquent de la même façon.

Un Director qui utilise l'`A2AToolsProvider` est soumis aux mêmes protections qu'un Director qui appelle `ctx.delegate()` — la transparence du routing ne contourne pas les garde-fous.

---

## Comparaison des deux approches

| Aspect | `ctx.delegate()` | `A2AToolsProvider` |
|---|---|---|
| Qui décide quand invoquer | Le code Python du Director | Le LLM dans la boucle ReAct |
| Prévisibilité | Élevée — flux déterministe | Variable — dépend du raisonnement LLM |
| Adaptabilité aux tâches imprévues | Faible — flux codé | Élevée — le LLM s'adapte |
| Débogage | Facile — appels explicites dans le code | Plus complexe — décisions LLM à inspecter |
| Cas d'usage | Workflow connu, orchestration métier | Analyse ad-hoc, tâches ouvertes |

La règle pratique : commencez avec `ctx.delegate()` pour les workflows que vous connaissez. Passez à l'`A2AToolsProvider` quand les tâches des utilisateurs sont trop variées pour être toutes orchestrées explicitement.
