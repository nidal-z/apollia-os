---
sidebar_position: 9
title: 9. Risques et dette technique
---

# 9. Risques et dette technique

Cette page énonce, sans détour, ce qui est partiel et ce qui est absent. Elle
découle d'une revue de capacités certifiée par le code, pas d'une intention de
conception. Une cartographie qui masque ses lacunes n'est pas crédible : les
lacunes sont donc exposées ici dans leur intégralité. Les statuts utilisent
trois mots : **partiel** (câblé mais incomplet), **absent** (non câblé, un
stub, ou du code mort) et, le cas échéant, une note signalant qu'un
commentaire du code exagère la réalité.

## Partiel : câblé, mais avec une limite énoncée

| Domaine | La limite honnête |
|---|---|
| **Vérifications** | La passe de critique est câblée sur le chemin orchestré. Le type du manifeste porte un champ `check_commands` pour des vérifications shell, et rien ne l'exécute. Le critique LLM est réel ; aucune vérification shell déterministe ne l'est. |
| **Reconnaissance vocale** | Uniquement en mode batch. La transcription et la traduction fonctionnent ; il n'existe pas de transcription en streaming. La fonctionnalité s'exécute en local sur CPU et renvoie une réponse de service indisponible quand le modèle est absent. |
| **Connecteurs** | Google est limité aux scopes non restreints du niveau gratuit (envoi Gmail et création de brouillons uniquement, aucun scope restreint ; Agenda, fichier Drive, et les scopes de documents sensibles). Il s'agit d'une posture délibérée de niveau gratuit, pas d'un bug, mais qui borne ce qu'un agent peut faire sur Google. |
| **Budget de coût en tokens** | Le budget d'étapes (steps, appels d'outils, temps réel) est appliqué, mais le seuil de coût en tokens ne l'est pas : il est par défaut pratiquement illimité. Les plafonds de coût ne constituent pas un arrêt strict. |
| **Budget d'exécution depuis la configuration** | Le plafond d'une exécution d'agent est fixé dans le code, à 100 pas de raisonnement, 200 appels d'outils et 1200 secondes, et aucune clé d'`apollia.toml` ne le change. Une exécution de workflow lit ses propres bornes, `[workflows] max_steps` et `wall_clock_secs`. |
| **Serveur MCP** | Apollia en tant que client MCP est solide sur trois transports. Le côté serveur est un pont stdio uniquement, désactivé par défaut, et il sert les agents et les workflows que l'utilisateur a exposés, pas les outils natifs. |
| **Recommandation de modèles** | L'onboarding classe les modèles pour la machine locale, mais trois de ses entrées relèvent du jugement plutôt que de la mesure : les scores de capacité de la table de générations (`families.toml`), la part de capacité que conserve chaque quantization, et les débits mémoire derrière le terme de vitesse. Les coûts mémoire, la compatibilité et le placement des couches sont calculés à partir des fichiers eux-mêmes. Le catalogue lit un index en ligne signé et se rabat sur l'index livré dans le binaire, si bien qu'une machine sans réseau se voit proposer les fiches que son build connaissait, aussi anciennes que ce build. Les fichiers GGUF fragmentés ne sont pas proposés, car le téléchargeur ne récupère qu'une URL. |
| **Déclencheurs** | Les sources cron, intervalle, ponctuelle, surveillance de fichiers et webhook sont câblées. Il n'existe ni déclencheur email ni déclencheur Slack. |
| **Copilote / couche méta-LLM** | L'ambition d'être « plus transparent qu'un assistant cloud » est câblée à environ un tiers. Parmi les commandes méta, seule Next Steps constitue un appel LLM réel de bout en bout ; le moteur de coaching est réel et l'assistant d'ajout d'un connecteur l'atteint à son étape de coaching, et le reste relève d'heuristiques ou de modèles. Les contrats sont en place ; le LLM secondaire reste, pour l'essentiel, à connecter. |
| **Usage du streaming** | Le streaming des tokens est réel, mais l'événement `done` du flux ne transporte aucun chiffre d'usage. |

## Absent ou code mort

| Domaine | Réalité |
|---|---|
| **Chargement GGUF fragmenté (sharded)** | Absent. Le moteur `llama-server` embarqué charge un modèle GGUF en fichier unique par processus serveur. Un commentaire du code suggérant un chargement fragmenté ne reflète aucun chemin câblé. |
| **Redémarrage des acteurs** | Aucun, par décision. Un acteur qui meurt n'est pas relancé : le runtime signale la perte et dégrade la capacité que cet acteur servait (voir [supervision](/architecture/decisions#supervision)). Les moteurs d'inférence et le runner de reconnaissance vocale sont des processus séparés, et le daemon relance ceux-là. |
| **Exécution directe via le chemin unifié** | Le chemin direct-via-unifié `execute()` est un stub. Le véritable chemin direct passe par un point d'entrée distinct ; le stub est secondaire. |

## Dérive documentaire que cette cartographie corrige

D'anciennes notes de sous-système ont surestimé certains points précis. Pour
mémoire, et afin qu'aucun lecteur ne se fie à la version périmée :

- Le contrat du SDK est `sdk/apollia/types.py`, pas un répertoire
  `sdk/apollia/stubs/` (qui n'existe pas).
- Le contexte d'exécution `ctx` regroupe dix-sept services, pas la forme plate
  antérieure. Voir la [référence du SDK](/reference/sdk).
- Le chargement GGUF fragmenté et une politique de redémarrage des acteurs
  étaient décrits comme fonctionnels ; aucun des deux n'existe, comme indiqué
  plus haut.
- Le rejeu était décrit comme une nouvelle exécution ; ce qui existe parcourt
  la trace capturée et n'exécute rien de nouveau.

## Ce que cela signifie pour un adoptant

L'avantage concurrentiel est réel et démontrable : une autonomie bornée avec
un budget d'étapes appliqué, un journal d'audit signé et vérifiable, un
système de permissions avec supervision humaine et paliers d'autonomie, un
garde-fou qui refuse les commandes shell enchaînées ou redirigées, et un
chemin d'outils gouverné, emprunté aussi bien par les outils natifs que par
les outils MCP. La détection d'injection de prompt ne fait pas partie de cette
liste, et aucun composant de l'arbre ne la fournit : ce que le garde-fou
ci-dessus filtre est l'injection shell. La
dette se situe surtout en périphérie : le volet vérifications shell des
contrôles, les plafonds de coût, et la couche copilote. Savoir précisément où
se situent ces marges est
tout l'objet de cette page.
