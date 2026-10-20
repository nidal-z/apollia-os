---
sidebar_position: 6.7
title: Demander une sortie structurée au modèle
---

# Demander une sortie structurée au modèle

Un code qui attend une valeur, et non un paragraphe, passe un JSON Schema à
`ctx.llm.complete` ou `ctx.llm.chat`. L'appel contraint alors ce que le modèle
peut émettre, vérifie ce qui revient contre ce même schéma, et vous rend la
valeur validée sous la forme d'un objet Python ordinaire. Il n'y a rien à
analyser et aucune expression régulière à maintenir.

## Où `ctx.llm` est joignable

`ctx.llm` est un service que le runtime peut laisser non attaché, et il vaut
alors `None` : la [référence `ctx.llm`](/reference/sdk/llm) le dit. Il est
attaché au contexte d'un workflow dès que le runtime a un modèle configuré, et
pas à celui d'un outil, qui joint le modèle par l'agent qui l'a appelé.
Cette page décrit le contrat de l'appel là où le service est attaché, et c'est
aussi ce que sert `MockContext` dans un test unitaire.

## Passer un schéma, recevoir une valeur

```python
from apollia.types import Ctx

REPORT = {
    "type": "object",
    "properties": {
        "title": {"type": "string"},
        "count": {"type": "integer"},
    },
    "required": ["title", "count"],
}


async def summarize(ctx: Ctx, text: str) -> dict[str, object]:
    report = await ctx.llm.complete(
        [{"role": "user", "content": text}],
        schema=REPORT,
    )
    # `report` is a dict, already checked against REPORT.
    return {"title": report["title"], "count": report["count"]}
```

Sans `schema`, l'appel rend un `LlmResponse` et `response.content` est une
chaîne, comme avant. Ajouter `schema` change le type de retour, et c'est le
propos : un appel qui promet une forme ne devrait pas rendre quelque chose que
vous devez encore inspecter.

## Ce que le schéma peut contenir

<!-- claim:llm-schema-constrains-local-generation -->
Objets et tableaux s'imbriquent librement, et leurs membres sont typés. Le
schéma voyage en `response_format`, la surface de sortie structurée que le
protocole OpenAI définit. Sur le moteur local llama.cpp, le `llama-server`
embarqué en tire une grammaire qui contraint la réponse jeton par jeton, de
sorte que le modèle ne peut pas émettre une forme que le schéma interdit ; un
modèle qui réfléchit d'abord le fait librement, et la contrainte porte sur ce
qu'il répond ensuite. Là, un schéma que le runtime ne sait pas exprimer en
grammaire est refusé avant l'appel plutôt qu'assoupli.

Sur le moteur MLX et sur un serveur compatible OpenAI que vous avez déclaré,
`response_format` est une demande : le serveur applique la contrainte dans la
mesure où il l'implémente, et le runtime ne vérifie pas le schéma avant
l'appel. Sur tous les backends, la réponse est validée contre le schéma avant
que votre code ne la reçoive.

| Construction | Prise en charge |
| --- | --- |
| `object` avec `properties`, imbriqué | oui |
| `array` avec `items`, imbriqué | oui |
| `string`, `number`, `integer`, `boolean`, `null` | oui |
| `enum`, sur n'importe quel type | oui |
| `required` | oui |
| `minimum`, `maximum`, `minLength`, `maxLength`, `minItems`, `maxItems` | vérifié, non contraint |
| `anyOf`, `oneOf`, `allOf`, `not`, `$ref` | refusé nommément sur le moteur local llama.cpp |

Les deux dernières lignes sont les intéressantes.

Une borne sur un nombre ou sur une longueur est vérifiée après la génération et
jamais pendant, car aucune grammaire de décodage n'en exprime. Déclarez-les :
elles sont appliquées, simplement une étape plus tard que le reste.

Sur le moteur local llama.cpp, un combinateur est refusé plutôt qu'ignoré. Une construction non prise en charge
qui relâcherait la contrainte en silence vous laisserait valider une réponse que
personne n'a contrainte, et en tenir le modèle pour responsable.

## Quand cela échoue

Un échec lève `apollia.errors.StructuredOutputError`, qui porte le chemin JSON du
noeud fautif, de sorte que vous branchez sur un champ plutôt que sur une phrase.

```python
from apollia.errors import StructuredOutputError

try:
    rapport = await ctx.llm.complete(messages, schema=REPORT)
except StructuredOutputError as e:
    ctx.logger.warning("sortie structurée en échec sur %s : %s", e.path, e.reason)
```

`e.kind` distingue les trois cas :

- `schema_unsupported` : le schéma ne peut pas devenir une contrainte sur le
  moteur local llama.cpp. Levé avant l'appel au modèle, donc aucun jeton n'a été
  dépensé.
- `backend_unsupported` : le backend résolu n'a pas de mode de sortie structurée.
  Aucun backend de cette version ne le lève : tous reçoivent le schéma.
- `response_invalid` : la réponse est revenue et le schéma la refuse.

Rien n'est réessayé pour vous. Qu'une seconde tentative vaille ses jetons dépend
de l'appelant, et le runtime n'a pas à en décider.

## Ce que cela coûte

Un appel contraint est un appel au modèle, donc il compte pour une étape dans le
`StepBudget` de l'exécution, exactement comme un appel non contraint. Le journal
d'audit note que la génération était contrainte, ainsi qu'une empreinte du
schéma ; le schéma lui-même n'est jamais écrit dans le journal.

## Étapes suivantes

- [Référence `ctx.llm`](/reference/sdk/llm)
- [Écrire un workflow](/how-to/write-a-director)
