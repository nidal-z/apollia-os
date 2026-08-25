---
sidebar_position: 8
title: Valeurs par défaut d'échantillonnage LLM
---

# Valeurs par défaut d'échantillonnage LLM

<!-- claim:sampling-only-temperature-reaches-the-backend -->

**Un seul paramètre d'échantillonnage atteint un modèle : `temperature`.** Une requête
transporte `temperature`, `max_tokens`, `seed`, `model`, `messages`, `tools` et une
grammaire optionnelle, rien de plus. `top_p`, `top_k` et `repetition_penalty` ne sont
pas des champs d'une requête ; un backend ne les reçoit jamais. Quand l'appelant ne
passe pas de `temperature`, le runtime n'en fixe aucune et le fournisseur ou
`llama-server` applique sa propre valeur par défaut.

C'est là tout ce qui gouverne l'échantillonnage aujourd'hui. Le reste de cette page
décrit une mécanique qui existe mais n'est pas encore exploitée, répertoriée ici parce
qu'elle est visible sur le disque et pourrait sinon passer pour un réglage fonctionnel.

## Définir la température

Par appel, via le SDK :

```python
await ctx.llm.complete(messages=..., temperature=0.3)
```

Par session de chat, `[chat] tool_turn_temperature` s'applique à un tour qui annonce
des outils, où une valeur plus basse stabilise la sélection d'outil. Voir
[Configuration](/reference/configuration).

## Ce qui existe mais n'est pas câblé

Télécharger un GGUF depuis HuggingFace lit le `generation_config.json` publié du
modèle et écrit les hyperparamètres trouvés dans
`~/.apollia/models/sampling-defaults.json`, une table plate associant une clé de
modèle à des champs :

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

Le runtime embarque aussi une table organisée de douze entrées couvrant les mêmes
familles. **Aucune des deux n'est lue au moment de l'inférence.** Le résolveur qui
les consulterait n'a aucun appelant en dehors du code de test, si bien que modifier
le fichier ne change rien aujourd'hui. Il est écrit, pas appliqué.

## Reproductibilité

**Une exécution n'est pas reproductible.** `ctx.llm.complete`, `chat` et `stream`
acceptent un `seed`, et la valeur est transportée jusqu'à la structure de requête,
mais aucun backend ne la lit : il n'existe aucune occurrence de `seed` dans une
implémentation de backend. En passer un est accepté et n'a aucun effet.

Deux exécutions du même prompt sur le même modèle peuvent donc diverger. Ce qui
est enregistré n'est pas la génération elle-même. Sur une exécution d'agent, la
piste d'invocations d'outils porte chaque appel d'outil avec une empreinte de ses
entrées et son issue, et le journal chaîné par hachage porte les entrées d'appel
d'outil et d'appel LLM de l'exécution. Aucun des deux registres ne reçoit un
appel d'outil effectué dans une session de chat. Voir
[Auditer et vérifier une exécution](/how-to/audit-and-verify).
