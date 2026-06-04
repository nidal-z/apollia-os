# Descriptions de paramètres via `Annotated`

Quand un LLM doit appeler une skill, il regarde l'`input_schema` exposé par le tool descriptor. Une signature Python sans annotation produit un schéma minimal : type + nullable. Pour des LLM mid-market (Haiku, GPT-4o-mini, Mistral), ce n'est souvent pas suffisant : le modèle hallucine la valeur d'un paramètre dont le nom est ambigu, ou oublie un format attendu.

La solution : annoter chaque paramètre non-trivial avec `typing.Annotated[T, "description"]`. Le SDK propage la description dans le JSON Schema sous la forme `properties[param].description`. Le LLM lit cette ligne au moment de générer le payload.

---

## Pattern minimal

Sans annotation :

```python
@skill("pdf.read_text", description="Read text from a PDF.")
async def read_text(
    self,
    path: str,
    ctx: Ctx,
    page_range: str | None = None,
) -> dict:
    ...
```

L'`input_schema` généré :

```json
{
  "type": "object",
  "properties": {
    "path": {"type": "string"},
    "page_range": {"type": "string", "nullable": true}
  },
  "required": ["path"]
}
```

Le LLM ne sait pas si `path` doit être absolu ou relatif, ni le format de `page_range`. Probabilité d'erreur élevée.

Avec annotation :

```python
from typing import Annotated

@skill("pdf.read_text", description="Read text from a PDF, page by page.")
async def read_text(
    self,
    path: Annotated[str, "Absolute filesystem path to the .pdf file."],
    ctx: Ctx,
    page_range: Annotated[
        str | None,
        "1-based page selection like '1-5,7,10-12'. Omit to read all pages.",
    ] = None,
) -> dict:
    ...
```

L'`input_schema` généré :

```json
{
  "type": "object",
  "properties": {
    "path": {
      "type": "string",
      "description": "Absolute filesystem path to the .pdf file."
    },
    "page_range": {
      "type": "string",
      "nullable": true,
      "description": "1-based page selection like '1-5,7,10-12'. Omit to read all pages."
    }
  },
  "required": ["path"]
}
```

Maintenant le LLM sait que `path` est absolu et que `page_range` suit un format précis. Les erreurs d'invocation chutent significativement.

---

## Quand annoter, quand ne pas annoter

Annotez quand :

- **Le format n'est pas évident du type seul.** `page_range: str` est ambigu, `Annotated[str, "1-based pages, e.g. '1-5,7'"]` ne l'est plus.
- **Le paramètre a un domaine de valeurs restreint.** `severity: Annotated[str, "One of: info, warning, error."]` cadre le choix.
- **L'unité ou la convention compte.** `timeout: Annotated[int, "Timeout in seconds (default 30)."]` désambigüise.
- **Plusieurs paramètres pourraient être confondus.** Pour une skill avec `source_path` et `target_path`, annoter clarifie.

N'annotez pas quand :

- **Le type seul suffit.** `bool`, `int`, `float` simples sans contexte particulier.
- **Le nom du paramètre est totalement explicite.** `user_id`, `email`, `count` se passent souvent d'annotation.
- **Vous risquez d'allonger le tool descriptor au-delà de ce qu'un LLM digère.** Un descripteur très verbeux noie le signal.

Règle pratique : si vous testez le tool descriptor sur Haiku-4.5 ou Mistral 7B et que le LLM se trompe sur un paramètre, annotez-le.

---

## Multi-segment

`Annotated` accepte plusieurs strings. Le SDK les concatène avec un espace :

```python
freshness_days: Annotated[
    int,
    "Maximum age of returned items in days.",
    "Use 1 for 'today only', 7 for 'last week', 0 for 'no cutoff'.",
] = 7,
```

L'`input_schema` voit `description: "Maximum age of returned items in days. Use 1 for 'today only', 7 for 'last week', 0 for 'no cutoff'."`.

Pratique pour découper visuellement une description longue sans la transformer en chaîne multiligne.

---

## `Annotated` sur les champs de `TypedDict`

Quand le paramètre est un `TypedDict`, vous pouvez aussi annoter ses champs internes (cf. [chapitre 21](21-typeddict-schemas.md)) :

```python
from typing import Annotated, TypedDict

class ChartSeries(TypedDict):
    name: Annotated[str, "Legend label for this series."]
    data: Annotated[list[float], "Numeric values, same length as x-axis labels."]

@skill("chart.render_bar", description="Render a bar chart.")
async def render_bar(
    self,
    series: Annotated[list[ChartSeries], "One entry per legend group."],
    ctx: Ctx,
) -> dict:
    ...
```

Le schéma inféré contient les descriptions à chaque niveau, donnant au LLM une vision structurée du payload attendu.

---

## Erreurs courantes

**`from __future__ import annotations` casse `Annotated`.** PEP 563 transforme toutes les annotations en strings et les helpers d'introspection perdent les `Annotated`. Le SDK utilise `typing.get_type_hints(fn, include_extras=True)` pour résoudre, donc dans la plupart des cas ça fonctionne. Mais pour les **TypedDict avec `NotRequired`**, l'import `from __future__` casse `__required_keys__` (cf. [chapitre 21](21-typeddict-schemas.md)) et donc le manifeste est faux. Ne pas l'importer dans les fichiers `schemas.py`.

**Annoter `ctx`.** Inutile, le SDK exclut `ctx` du schéma par convention. Si vous annotez `ctx: Annotated[Ctx, "..."]`, la description est silencieusement ignorée.

**Description en français pour un agent multilingue.** Le LLM lit l'`input_schema` au moment de générer un payload. Si l'agent appelant peut être en anglais ou en français, gardez les descriptions en anglais (langue dominante des LLM).

---

## Anti-patterns

**Ne pas** mettre du markdown ou du HTML dans une description. Le LLM les voit comme du texte brut, le rendu est désordonné. Restez en prose simple.

**Ne pas** mettre des exemples dans la description (`"e.g. /tmp/foo.pdf"`). Les exemples vivent dans `@skill(examples=[...])` qui leur donne une place dédiée et structurée (cf. [chapitre 20](20-examples-payloads.md)).

**Ne pas** annoter chaque paramètre par réflexe. Une description par paramètre **utile** vaut mieux que dix descriptions par paramètre **trivial**.

---

## ADRs

- `ADR-023` : Signature inference comme schéma I/O
- Release `AGENTKIT-REBUILD-2026-05-19` section 2026-05-20 : Annotated + examples + TypedDict

*(ADRs disponibles prochainement, cf. l'encadré "ADRs et wiki" en introduction.)*
