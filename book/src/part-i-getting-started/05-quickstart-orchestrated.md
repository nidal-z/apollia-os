# Quickstart : agent orchestré

Un agent orchestré laisse le moteur ORIA (Observer, Reasoner, Actor) côté Rust piloter la boucle d'exécution. Vous décrivez l'intention en `system_prompt`, vous listez les outils dont l'agent dispose, le runtime fait le reste.

**Objectif :** écrire un assistant qui synthétise un dossier en suivant une procédure libre que ORIA planifie. Code complet : 30 lignes. Temps : 15 minutes.

---

## Le fichier `briefing_agent.py`

```python
"""Briefing assistant, ORIA plans and executes the steps."""

from apollia import agent, orchestrated


SYSTEM_PROMPT = """\
You are a briefing assistant. Given a topic, build a 1-page briefing that
covers:

1. Context (2 to 3 sentences on why the topic matters now).
2. Key facts (5 to 7 bullet points with sources).
3. Open questions (3 questions a decision-maker would ask).

Use the available tools to gather facts:
- `web.search` to find recent information.
- `web.fetch` to retrieve a full article when needed.
- `memory.recall` to read user preferences before formatting.

Stay grounded. If you cannot verify a fact, say so explicitly in the
briefing.
"""


@agent(
    name="briefing",
    version="0.1.0",
    description="Synthesize a one-page briefing on any topic.",
    tools_required=("web_search", "web_read"),
)
@orchestrated(system_prompt=SYSTEM_PROMPT)
class Briefing:
    def on_plan_complete(self, step_results: dict) -> str:
        texts = [step.get("text", "") for step in step_results.values()]
        return "\n\n".join(t for t in texts if t)
```

C'est tout. Une classe, deux décorateurs empilés, un hook optionnel.

---

## Anatomie du code

**Deux décorateurs.** `@orchestrated(system_prompt=...)` est en bas (le plus proche de la classe), `@agent(...)` en haut. L'ordre d'écriture est important : Python applique les décorateurs de bas en haut, donc `@orchestrated` stamp la config puis `@agent` lit les markers et construit le manifeste.

**`system_prompt`.** En anglais propre, code de plus de 100 caractères. ORIA l'utilise pour planifier et exécuter. Vous décrivez **ce que l'agent doit faire**, pas **comment** : ORIA décompose en étapes.

**`tools_required=("web_search", "web_read")`.** Liste les outils que ORIA pourra appeler. Le runtime fail-fast au boot si l'un d'eux manque (cf. [chapitre 6](../part-ii-the-decorators/06-agent-decorator.md)).

**`on_plan_complete`.** Hook optionnel. Quand ORIA a terminé toutes les étapes du plan, il appelle cette méthode avec `step_results` (un dict `{step_id: {"text": ...}}`). Le retour est la réponse finale.

Si vous ne définissez pas `on_plan_complete`, ORIA concatène les textes des étapes dans l'ordre. Ce qui est souvent suffisant pour un quickstart.

**Pas de `@skill`, pas de `@on_message`.** `@orchestrated` est mutuellement exclusif (cf. [chapitre 9](../part-ii-the-decorators/09-orchestrated-decorator.md)). L'agent est entièrement piloté par ORIA, donc une seule entrée logique.

---

## Lancer l'agent

```bash
python -m apollia inspect briefing_agent.py
apollia agent install ./briefing_agent.py
apollia invoke briefing default "Donne-moi un briefing sur le Permanent Beta de Microsoft."
```

Le runtime ORIA :

1. **Observer :** parse le `system_prompt`, identifie les outils, lit la tâche.
2. **Reasoner :** appelle le LLM pour produire un plan (3 à 6 étapes, structurées).
3. **Actor :** exécute chaque étape via les outils, observe les résultats, replanifie si besoin (max 2 replans par défaut).
4. **`on_plan_complete` :** votre hook agrège les sorties en réponse finale.

Le détail du moteur ORIA est dans la [Partie VIII](../part-viii-runtime-rust/29-runtime-overview.md). Pour le quickstart, retenez : vous décrivez l'intention, ORIA fait le travail.

---

## `@orchestrated` vs. `apollia.react`

Deux voies pour faire raisonner un agent. Quand choisir laquelle ?

| Critère | `@orchestrated` | `apollia.react` |
|---|---|---|
| Qui pilote la boucle | Le runtime ORIA (Rust) | Vous, dans `@on_message` |
| Cas d'usage | Agent autonome multi-étapes, intention en langue naturelle | Workflow connu, vous voulez du contrôle |
| Volume de code | Ultra-court (30 lignes) | Court (50 lignes) |
| Pré et post-traitement | Limité au hook `on_plan_complete` | Libre, code Python avant et après `react` |
| Branches conditionnelles | Difficiles à exprimer | Naturelles |
| Mode conversationnel | Pas de mode chat libre | Oui, via `@on_message` |

Règle simple : si vous pouvez décrire la mission en 5 phrases d'intention, `@orchestrated` est le bon outil. Si vous voulez du contrôle (par exemple un workflow avec deux phases distinctes), `apollia.react` est meilleur.

---

## Variations

**Custom `on_plan_complete` :** mettez en forme la réponse selon votre métier :

```python
def on_plan_complete(self, step_results: dict) -> str:
    sections = []
    for step_id, step in step_results.items():
        if step.get("kind") == "fact":
            sections.append(f"- {step['text']}")
    return "## Key facts\n\n" + "\n".join(sections)
```

**Configurer un step budget custom :**

```python
@agent(
    name="briefing",
    version="0.1.0",
    description="…",
    tools_required=(...),
    step_budget={"max_steps": 25, "max_tool_calls": 40, "wall_clock_secs": 600},
)
@orchestrated(system_prompt=...)
class Briefing:
    ...
```

---

## Prochaines étapes

Vous avez vu les 4 patterns d'agent. La suite du book entre dans le détail :

- **[Partie II. Les décorateurs](../part-ii-the-decorators/06-agent-decorator.md)** : `@agent`, `@skill`, `@on_message`, `@orchestrated` en profondeur.
- **[Partie III. Le protocole Ctx](../part-iii-the-ctx-protocol/10-ctx-overview.md)** : les 14 services injectés dans chaque agent.
- **[Partie IX. Capstone](../part-ix-capstone/37-capstone-overview.md)** : un projet multi-agent complet qui consolide tout.
