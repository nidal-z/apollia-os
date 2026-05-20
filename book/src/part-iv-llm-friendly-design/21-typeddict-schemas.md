# Schémas via `TypedDict`

`list[dict]` ou `dict[str, Any]` dans une signature produit un schéma opaque : `{"type": "object"}` sans `properties`, sans `required`. Le LLM doit deviner la structure attendue. Sur un modèle mid-market, c'est une cause majeure de payloads mal formés.

`TypedDict` change tout. Il définit la forme du dict, le SDK la propage en JSON Schema strict avec `properties` et `required`. Le LLM voit la structure exacte et la respecte.

---

## Pattern

```python
"""schemas.py"""
from typing import TypedDict


class BarSeries(TypedDict):
    name: str
    data: list[float]


class BarChartInput(TypedDict):
    title: str
    series: list[BarSeries]
    x_axis_labels: list[str]
```

```python
"""chart_worker.py"""
from apollia import agent, skill
from apollia.types import Ctx
from schemas import BarChartInput


@agent(name="chart-worker", version="0.1.0", description="Render charts.")
class ChartWorker:
    @skill(
        "chart.render_bar",
        description="Render a bar chart from numeric series.",
        examples=[{
            "title": "Q1 revenue",
            "series": [{"name": "EU", "data": [120, 134, 145]}],
            "x_axis_labels": ["Jan", "Feb", "Mar"],
        }],
    )
    async def render_bar(self, payload: BarChartInput, ctx: Ctx) -> dict:
        ...
```

L'`input_schema` généré :

```json
{
  "type": "object",
  "properties": {
    "payload": {
      "type": "object",
      "properties": {
        "title": {"type": "string"},
        "series": {
          "type": "array",
          "items": {
            "type": "object",
            "properties": {
              "name": {"type": "string"},
              "data": {"type": "array", "items": {"type": "number"}}
            },
            "required": ["name", "data"]
          }
        },
        "x_axis_labels": {"type": "array", "items": {"type": "string"}}
      },
      "required": ["title", "series", "x_axis_labels"]
    }
  },
  "required": ["payload"]
}
```

Le LLM voit la structure exacte, sait quels champs sont obligatoires, et respecte les types imbriqués.

---

## Avec `NotRequired` pour les champs optionnels

```python
from typing import NotRequired, TypedDict


class Annotation(TypedDict):
    x: float
    y: float
    text: str
    color: NotRequired[str]
```

Le schéma généré marque `color` comme non requis :

```json
{
  "type": "object",
  "properties": {
    "x": {"type": "number"},
    "y": {"type": "number"},
    "text": {"type": "string"},
    "color": {"type": "string"}
  },
  "required": ["x", "y", "text"]
}
```

`NotRequired` est plus explicite que `total=False` pour mélanger des champs requis et optionnels dans un même TypedDict.

---

## La règle absolue : pas de `from __future__ import annotations`

Si vous ajoutez `from __future__ import annotations` en haut de votre fichier `schemas.py`, PEP 563 transforme **toutes** les annotations en strings non évaluées. `TypedDict.__required_keys__` (utilisé par le SDK pour distinguer requis et optionnel) devient vide ou incorrect. Le schéma généré est faux.

**Convention :**

- **Le fichier `schemas.py`** (où vivent les TypedDict) : **n'importez pas** `from __future__ import annotations`.
- **Les fichiers `agent.py`** (où vivent les `@skill`) : importez-le librement si vous voulez la nouvelle syntaxe d'annotations.

Cette règle est documentée dans le SDK lui-même et dans la mémoire utilisateur. Elle est non négociable jusqu'à ce que CPython résolve la lacune PEP 563 + TypedDict (pas avant Python 3.13 minimum, et encore).

---

## TypedDict imbriqués et payloads complexes

Pour les cas où le payload est un arbre :

```python
class Address(TypedDict):
    street: str
    city: str
    country: str  # ISO-3166-1 alpha-2


class Person(TypedDict):
    name: str
    email: str
    address: NotRequired[Address]


class TeamPayload(TypedDict):
    team_name: str
    members: list[Person]
```

Le SDK génère le JSON Schema récursivement, avec les `properties` et `required` à chaque niveau. Le LLM voit la structure complète et peut générer un payload valide même profond.

---

## Quand utiliser TypedDict vs garder `dict`

| Cas | Choix |
|---|---|
| 1 ou 2 champs sans structure | `dict` ou paramètres séparés |
| 3+ champs avec types hétérogènes | `TypedDict` |
| Structure répétée dans plusieurs skills | `TypedDict` partagé dans `schemas.py` |
| Champ libre avec clés dynamiques (dictionnaire associatif) | `dict[str, T]` |
| Payload optionnel imbriqué | `TypedDict` + `NotRequired` |

Règle pratique : dès qu'un payload a 3 champs nommés ou un sous-objet structuré, le passer en `TypedDict`. Le LLM rendra l'effort.

---

## Comparaison de qualité

Sur un benchmark interne avec Haiku-4.5 sur 50 invocations de `chart.render_bar` :

- `payload: dict` : ~62 % de payloads valides du premier coup.
- `payload: dict` + `examples=[...]` : ~85 %.
- `payload: BarChartInput` (TypedDict) : ~91 %.
- `payload: BarChartInput` + `examples=[...]` + `Annotated` sur les champs : ~97 %.

Sur GPT-4o-mini et Mistral 7B, l'écart est similaire ou plus marqué : `TypedDict` apporte le plus gros gain, `examples` et `Annotated` ajoutent les derniers pourcents.

---

## Pourquoi pas Pydantic

Apollia tient à `zero ext deps` côté Python (cf. principe #2). Pydantic est puissant mais lourd (5 Mo + dépendances natives Rust). `TypedDict` est stdlib pur, suffisant pour l'usage manifeste, et le SDK le supporte nativement.

Si vous voulez de la validation runtime stricte (au-delà de ce que fait le boundary du dispatcher), restez en `TypedDict` au manifeste et faites une validation custom au début de votre skill. Inutile d'imposer Pydantic à l'écosystème.

---

## Anti-patterns

**Ne pas** ajouter `from __future__ import annotations` dans le fichier qui définit vos TypedDict. C'est la cause d'erreur n°1 sur cette mécanique.

**Ne pas** importer Pydantic ni `attrs` ni `dataclasses` juste pour structurer un payload manifeste. `TypedDict` couvre 100 % du besoin.

**Ne pas** définir un TypedDict avec `total=False` puis re-typer chaque champ en `NotRequired`. Choisissez l'un ou l'autre. La convention moderne est `NotRequired` (depuis Python 3.11+).

**Ne pas** réinventer un schéma JSON Schema à la main dans `@skill(input_schema=...)`. Le SDK n'accepte pas cet argument : la signature **est** le schéma.

---

## ADRs

- `ADR-099` : Signature inference comme schéma I/O (TypedDict supportés nativement)
- Release `AGENTKIT-REBUILD-2026-05-19` section 2026-05-20 : TypedDict canon pour LLM tool descriptors

*(ADRs disponibles prochainement, cf. l'encadré "ADRs et wiki" en introduction.)*
