---
sidebar_position: 8
title: Valeurs par défaut d'échantillonnage LLM
---

# Valeurs par défaut d'échantillonnage LLM

<!-- claim:sampling-only-temperature-reaches-the-backend -->

Une requête à un modèle porte ses réglages d'échantillonnage sous forme de
champs facultatifs. Un champ renseigné est envoyé au backend ; un champ laissé
vide n'est pas envoyé, et le fournisseur ou `llama-server` applique sa propre
valeur par défaut. Cette page liste les champs, les noms sous lesquels ils
voyagent, et les niveaux de valeurs par défaut qui remplissent les champs qu'un
appel laisse vides.

## Champs de la requête

Chaque backend est joint par le protocole de complétion de chat compatible
OpenAI, le `llama-server` embarqué compris.

| Champ | Envoyé sous | Protocole |
|---|---|---|
| `temperature` | `temperature` | OpenAI |
| `max_tokens` | `max_tokens` | OpenAI |
| `seed` | `seed` | OpenAI |
| `top_p` | `top_p` | OpenAI |
| `presence_penalty` | `presence_penalty` | OpenAI |
| `frequency_penalty` | `frequency_penalty` | OpenAI |
| `stop` | `stop` | OpenAI. Une liste vide n'envoie rien. |
| `top_k` | `top_k` | extension de `llama-server` |
| `min_p` | `min_p` | extension de `llama-server` |
| `repeat_penalty` | `repeat_penalty` | extension de `llama-server` |
| `thinking`, `reasoning_effort` | `chat_template_kwargs` | extension de `llama-server`, lue par le gabarit de chat du modèle |

Les trois extensions d'échantillonnage sont écrites dans le corps de la requête
sous les noms que lit `llama-server`, et elles sont envoyées à tout backend dès
qu'elles sont renseignées, pas seulement au moteur embarqué. Un serveur distant
qui ne connaît pas `top_k`, `min_p` ou `repeat_penalty` fait ce qu'il fait d'un
champ inconnu, refus compris. Laissez-les vides pour un tel serveur.

## Par appel, depuis une étape de workflow

`ctx.llm.complete` prend en mot-clé chaque champ du tableau sauf
`reasoning_effort` :

```python
await ctx.llm.complete(messages, temperature=0.3, top_p=0.9, seed=7, stop=["END"])
```

Un appel d'une étape de workflow n'appartient à aucun agent : aucun des niveaux
ci-dessous ne s'y applique, et ce que l'appel fixe est ce qui est envoyé. Une
borne s'ajoute : `complete` envoie `max_tokens = 4096` quand l'appel n'en fixe
pas. `ctx.llm.chat` et `ctx.llm.stream` ne prennent que `temperature`,
`max_tokens` et `seed`.

## Niveaux de valeurs par défaut d'un agent

Chaque appel au modèle d'un tour d'agent part de réglages de requête résolus
champ par champ. Un niveau l'emporte sur ceux du dessous ; un champ qu'il laisse
vide garde la valeur du niveau inférieur.

| Niveau | Source | Champs |
|---|---|---|
| 4. Conversation | Les réglages saisis pour une conversation, qui ne valent que pour elle | `temperature`, `max_tokens` |
| 3. Préréglage | Le préréglage choisi pour la conversation, sinon celui choisi pour l'agent, sinon le préréglage par défaut du modèle de l'agent. Un préréglage appartient à un modèle et ne compte que si l'appel s'exécute sur ce modèle | `temperature`, `max_tokens`, `top_p`, `top_k`, `min_p`, `presence_penalty`, `repeat_penalty`, `stop` |
| 2. Paquet | `[agents.defaults.model.request]` dans le [manifeste de paquet](/reference/package-manifest) | `temperature`, `top_p`, `top_k`, `repetition_penalty`, `max_tokens`, `seed` |
| 1. Modèle | Les valeurs que recommande son éditeur, voir la section suivante | `temperature`, `top_p`, `top_k`, `min_p`, `presence_penalty`, `repetition_penalty` |

`temperature` suit une règle de plus. Quand ni la conversation ni le
préréglage ne la fixent, un appel qui propose des outils au modèle utilise
`[chat] tool_turn_temperature` (0.3 quand la clé est absente), avant les niveaux
du paquet et du modèle. Une valeur plus basse stabilise le choix de l'outil.
Voir [Configuration](/reference/configuration).

## Valeurs par défaut du modèle

Le niveau du modèle est lui-même résolu depuis trois sources, champ par champ,
la première qui répond l'emportant :

1. La fiche du modèle copiée à côté du fichier du modèle lors de son
   téléchargement.
2. `~/.apollia/models/sampling-defaults.json`. Un téléchargement depuis Hugging
   Face lit le `generation_config.json` que publie le modèle et y écrit les
   valeurs trouvées, sous le nom de fichier du modèle. Le fichier se modifie
   aussi à la main.
3. Une table de douze familles de modèles embarquée dans le binaire, appariée
   sur l'architecture que déclare l'en-tête du GGUF et sur le nom du modèle.

Le fichier est une table plate associant un nom de fichier de modèle à des
champs :

```json
{
  "Qwen3-30B-A3B-Q4_K_M.gguf": {
    "temperature": 0.6,
    "top_p": 0.95,
    "top_k": 20,
    "repetition_penalty": null
  }
}
```

Un modèle qu'aucune des trois ne connaît ne reçoit aucune valeur de ce niveau.

## Reproductibilité

`seed` atteint le backend, depuis un appel ou depuis le niveau du paquet. Que
deux exécutions avec la même graine produisent le même texte dépend du moteur
et du serveur derrière le backend : le runtime envoie la valeur et ne garantit
rien au-delà. Sans graine, deux exécutions du même prompt sur le même modèle
peuvent diverger.

Ce qui est enregistré n'est pas la génération elle-même. Le journal chaîné par
hachage porte les entrées d'appel d'outil et d'appel LLM d'une exécution,
conversation comprise. Voir
[Auditer et vérifier une exécution](/how-to/audit-and-verify).
