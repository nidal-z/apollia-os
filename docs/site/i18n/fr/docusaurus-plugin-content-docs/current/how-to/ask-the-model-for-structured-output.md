---
sidebar_position: 6.7
title: Demander une sortie structurée au modèle
---

# Demander une sortie structurée au modèle

Un agent qui attend une valeur, et non un paragraphe, passe un JSON Schema à
`ctx.llm.complete` ou `ctx.llm.chat`. L'appel contraint alors ce que le modèle
peut émettre, vérifie ce qui revient contre ce même schéma, et vous rend la
valeur validée sous la forme d'un objet Python ordinaire. Il n'y a rien à
analyser et aucune expression régulière à maintenir.

## Passer un schéma, recevoir une valeur

```python
from apollia import agent, skill

RAPPORT = {
    "type": "object",
    "properties": {
        "title": {"type": "string"},
        "count": {"type": "integer"},
    },
    "required": ["title", "count"],
}


@agent(name="reporter", version="1.0.0", description="Résume en une fiche")
class Reporter:
    @skill(id="summarize")
    async def summarize(self, payload, ctx):
        rapport = await ctx.llm.complete(
            [{"role": "user", "content": payload["text"]}],
            schema=RAPPORT,
        )
        # `rapport` est un dict, déjà vérifié contre RAPPORT.
        return {"title": rapport["title"], "count": rapport["count"]}
```

Sans `schema`, l'appel rend un `LlmResponse` et `response.content` est une
chaîne, comme avant. Ajouter `schema` change le type de retour, et c'est le
propos : un appel qui promet une forme ne devrait pas rendre quelque chose que
vous devez encore inspecter.

## Ce que le schéma peut contenir

<!-- claim:llm-schema-constrains-local-generation -->
Objets et tableaux s'imbriquent librement, et leurs membres sont typés. Sur le
`llama-server` embarqué, le schéma devient une grammaire GBNF qui contraint le
décodage jeton par jeton, de sorte que le modèle ne peut pas émettre une forme
que le schéma interdit. Sur tout autre fournisseur compatible OpenAI, il voyage
en `response_format`, la surface de sortie structurée que ce protocole définit.

| Construction | Prise en charge |
| --- | --- |
| `object` avec `properties`, imbriqué | oui |
| `array` avec `items`, imbriqué | oui |
| `string`, `number`, `integer`, `boolean`, `null` | oui |
| `enum`, sur n'importe quel type | oui |
| `required` | oui |
| `minimum`, `maximum`, `minLength`, `maxLength`, `minItems`, `maxItems` | vérifié, non contraint |
| `anyOf`, `oneOf`, `allOf`, `$ref` | refusé nommément |

Les deux dernières lignes sont les intéressantes.

Une borne sur un nombre ou sur une longueur est vérifiée après la génération et
jamais pendant, car aucune grammaire de décodage n'en exprime. Déclarez-les :
elles sont appliquées, simplement une étape plus tard que le reste.

Un combinateur est refusé plutôt qu'ignoré. Une construction non prise en charge
qui relâcherait la contrainte en silence vous laisserait valider une réponse que
personne n'a contrainte, et en tenir le modèle pour responsable.

## Quand cela échoue

Un échec lève `apollia.errors.StructuredOutputError`, qui porte le chemin JSON du
noeud fautif, de sorte que vous branchez sur un champ plutôt que sur une phrase.

```python
from apollia.errors import StructuredOutputError

try:
    rapport = await ctx.llm.complete(messages, schema=RAPPORT)
except StructuredOutputError as e:
    ctx.logger.warning("sortie structurée en échec sur %s : %s", e.path, e.reason)
```

`e.kind` distingue les trois cas :

- `schema_unsupported` : le schéma ne peut pas devenir une contrainte. Levé avant
  l'appel au modèle, donc aucun jeton n'a été dépensé.
- `backend_unsupported` : le backend résolu n'a pas de mode de sortie structurée.
  Anthropic et Vertex sont dans ce cas aujourd'hui.
- `response_invalid` : la réponse est revenue et le schéma la refuse.

Rien n'est réessayé pour vous. Qu'une seconde tentative vaille ses jetons dépend
de l'agent, et le runtime n'a pas à en décider.

## Ce que cela coûte

Un appel contraint est un appel au modèle, donc il compte pour une étape dans le
`StepBudget` de l'agent, exactement comme un appel non contraint. Le journal
d'audit note que la génération était contrainte, ainsi qu'une empreinte du
schéma ; le schéma lui-même n'est jamais écrit dans le journal.

## Étapes suivantes

- [Référence `ctx.llm`](/reference/sdk/llm)
- [Écrire un worker](/how-to/write-a-worker)
