# Exemples de payloads

`Annotated` donne au LLM des descriptions. `examples` lui donne des **payloads complets et valides** qu'il peut copier presque tel quel. Sur des modèles mid-market (Haiku, GPT-4o-mini), un exemple est souvent ce qui transforme « 70 % de payloads valides du premier coup » en « 95 % de payloads valides du premier coup ».

---

## Pattern

```python
from apollia import agent, skill
from apollia.types import Ctx

@agent(name="chart-worker", version="0.1.0", description="Render charts.")
class ChartWorker:
    @skill(
        "chart.render_bar",
        description="Render a bar chart from numeric series.",
        examples=[
            {
                "title": "Q1 revenue per region",
                "series": [
                    {"name": "EU", "data": [120, 134, 145]},
                    {"name": "US", "data": [98, 112, 130]},
                ],
                "x_axis_labels": ["Jan", "Feb", "Mar"],
            },
            {
                "title": "Active users",
                "series": [
                    {"name": "Web", "data": [1200, 1340, 1450, 1510]},
                ],
                "x_axis_labels": ["W1", "W2", "W3", "W4"],
            },
        ],
    )
    async def render_bar(
        self,
        title: str,
        series: list[dict],
        x_axis_labels: list[str],
        ctx: Ctx,
    ) -> dict:
        ...
```

Les exemples apparaissent dans le tool descriptor passé au LLM, à côté de l'`input_schema`. Le LLM lit, comprend la forme attendue, et génère son propre payload en s'inspirant.

---

## Combien d'exemples

**Un seul exemple :** suffisant pour les skills simples (1 à 3 paramètres, structure plate). C'est mieux que zéro de loin.

**Deux à trois exemples :** utile quand la skill a des cas variés (avec et sans paramètre optionnel, plusieurs structures de données acceptées). Montrez les différentes formes.

**Plus de trois :** rarement utile, et alourdit le tool descriptor. Préférez ajouter une `description` plus précise ou un `Annotated` plus parlant.

---

## Que mettre dans un exemple

Un exemple est un **payload réaliste**, pas un payload minimal. Si la skill accepte des paramètres optionnels intéressants, montrez-les. Si elle accepte des structures complexes, montrez-en au moins une.

Bon exemple :

```python
examples=[
    {
        "query": "claude code rate limits",
        "max_results": 5,
        "lang": "en",
        "freshness_days": 30,
    },
]
```

Le LLM voit l'usage typique : tous les paramètres remplis, valeurs plausibles.

Mauvais exemple (trop minimal) :

```python
examples=[{"query": "test"}]
```

Le LLM voit que seul `query` est utilisé et tend à oublier les autres paramètres, même quand ils seraient utiles.

---

## Validation

Le SDK **ne valide pas** les exemples contre l'`input_schema` inféré. C'est volontaire : valider à la décoration créerait une dépendance circulaire avec la construction du manifeste. L'auteur est responsable de la cohérence.

Concrètement, si vous renommez un paramètre dans la signature et oubliez de mettre à jour l'exemple, `apollia inspect` montrera quand même les deux. Vérification manuelle :

```bash
python -m apollia inspect chart_worker.py
# Lisez l'examples affiché et vérifiez que les clés correspondent aux properties.
```

Un test unitaire peut aussi vérifier la cohérence si vous tenez à automatiser :

```python
import json
import jsonschema
from chart_worker import ChartWorker

def test_examples_match_schema():
    skill = ChartWorker.__apollia_manifest__["skills"][0]
    for ex in skill["examples"]:
        jsonschema.validate(instance=ex, schema=skill["input_schema"])
```

---

## Examples vs description vs Annotated

Trois leviers, trois rôles complémentaires :

| Levier | Rôle | Cas typique |
|---|---|---|
| `description` (du skill) | Phrase d'invite : « à quoi sert cette skill » | Toujours présente. |
| `Annotated[T, "..."]` (par paramètre) | Préciser format, unité, contrainte | Quand le type seul est ambigu. |
| `examples=[...]` (par skill) | Montrer la forme complète | Quand le schéma est non trivial ou que des paramètres optionnels comptent. |

Les trois s'additionnent. Une skill avec une bonne description, des annotations utiles et un exemple réaliste maximise les chances qu'un LLM mid-market réussisse l'invocation du premier coup.

---

## Anti-patterns

**Ne pas** mettre du faux contenu dans un exemple (`{"query": "<your query>"}`). Le LLM peut littéralement copier le placeholder. Préférez un exemple réaliste qui ressemble à un cas d'usage.

**Ne pas** dupliquer les exemples avec de légères variations. Un exemple « avec tous les paramètres » et un exemple « minimal » couvrent 95 % des cas. Pas besoin de cinq variantes.

**Ne pas** mettre des secrets ou des données sensibles dans un exemple. Les exemples sont publics dans le tool descriptor. Un faux email ou une fausse URL suffit.

**Ne pas** oublier de mettre à jour les exemples quand la signature change. Le SDK ne valide pas, donc rien ne vous arrête.

---

## ADRs

- `ADR-023` : Signature inference comme schéma I/O
- Release `AGENTKIT-REBUILD-2026-05-19` section 2026-05-20 : LLM tool descriptor optimization

*(ADRs disponibles prochainement, cf. l'encadré "ADRs et wiki" en introduction.)*
