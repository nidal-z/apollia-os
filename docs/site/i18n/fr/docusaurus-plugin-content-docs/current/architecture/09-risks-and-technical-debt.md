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
| **Vérifications** | La passe de critique est câblée sur le chemin orchestré, mais l'exécution sous gouvernance des `check_commands` shell déclarés par un agent ne l'est pas ; ce mécanisme d'invocation est aujourd'hui un no-op. Le critique LLM est réel ; les vérifications shell déterministes restent à câbler. |
| **Reconnaissance vocale** | Uniquement en mode batch. La transcription et la traduction fonctionnent ; il n'existe pas de transcription en streaming. La fonctionnalité s'exécute en local sur CPU et renvoie une réponse de service indisponible quand le modèle est absent. |
| **Connecteurs** | Google est limité aux scopes non restreints du niveau gratuit (envoi Gmail et création de brouillons uniquement, aucun scope restreint ; Agenda, fichier Drive, et les scopes de documents sensibles). Il s'agit d'une posture délibérée de niveau gratuit, pas d'un bug, mais qui borne ce qu'un agent peut faire sur Google. |
| **Chemin orchestré du bureau** | Le chemin d'exécution orchestré est un no-op dans l'application de bureau ; le chemin direct, lui, est borné et câblé. |
| **Budget de coût en tokens** | Le budget d'étapes (steps, appels d'outils, temps réel) est appliqué, mais le seuil de coût en tokens ne l'est pas : il est par défaut pratiquement illimité. Les plafonds de coût ne constituent pas encore un arrêt strict. |
| **Budget d'exécution depuis la configuration** | Le plafond d'étapes est appliqué avec une valeur par défaut sûre, mais la lecture de ce plafond depuis `apollia.toml` au moment de l'exécution reste à câbler. |
| **Serveur MCP entrant** | Apollia en tant que client MCP est solide sur trois transports. Le serveur MCP entrant (Apollia s'exposant lui-même) est partiel : stdio uniquement. |
| **Déclencheurs** | Les sources cron, intervalle, ponctuelle et surveillance de fichiers sont câblées. La source de déclenchement webhook est un stub no-op. Il n'existe ni déclencheur email ni déclencheur Slack. |
| **Copilote / couche méta-LLM** | L'ambition d'être « plus transparent qu'un assistant cloud » est câblée à environ un tiers. Parmi les commandes méta, seule Next Steps constitue un appel LLM réel de bout en bout ; le moteur de coaching est réel et l'assistant d'ajout d'un connecteur l'atteint à son étape de coaching, et le reste relève d'heuristiques ou de modèles. Les contrats sont en place ; le LLM secondaire reste, pour l'essentiel, à connecter. |
| **Usage du streaming** | Le streaming des tokens est réel, mais l'événement `done` du flux ne transporte aucun chiffre d'usage. |

## Absent ou code mort

| Domaine | Réalité |
|---|---|
| **Chargement GGUF fragmenté (sharded)** | Absent. Le moteur `llama-server` embarqué charge un modèle GGUF en fichier unique par processus serveur. Un commentaire du code suggérant un chargement fragmenté ne reflète aucun chemin câblé. |
| **Embeddings** | Absent. Le chemin des embeddings est un stub, pas une capacité livrée. |
| **Supervision de santé et redémarrage automatique de l'inférence** | Absent. Le daemon lance le processus d'inférence `llama-server` embarqué et en verrouille le chargement, mais il n'existe ni supervision de santé ni redémarrage automatique. Un commentaire du code affirmant le contraire est erroné. |
| **Politique de redémarrage des acteurs** | Le superviseur d'acteurs définit une politique de redémarrage, mais elle n'est pas appliquée. Il s'agit aujourd'hui, dans les faits, de code mort. |
| **Exécution directe via le chemin unifié** | Le chemin direct-via-unifié `execute()` est un stub. Le véritable chemin direct passe par un point d'entrée distinct ; le stub est secondaire. |

## Dérive documentaire que cette cartographie corrige

D'anciennes notes de sous-système ont surestimé certains points précis. Pour
mémoire, et afin qu'aucun lecteur ne se fie à la version périmée :

- Le contrat du SDK est `sdk/apollia/types.py`, pas un répertoire
  `sdk/apollia/stubs/` (qui n'existe pas).
- Le contexte d'exécution `ctx` regroupe quinze services, pas la forme plate
  antérieure. Voir la [référence du SDK](/reference/sdk).
- Le chargement GGUF fragmenté, le redémarrage automatique du runner, et la
  politique de redémarrage des acteurs étaient décrits comme fonctionnels ;
  ils ne le sont pas, comme indiqué plus haut.
- Le rejeu (replay) était décrit comme une fonctionnalité de fidélité ; il a
  été abandonné par décision.

## Ce que cela signifie pour un adoptant

L'avantage concurrentiel est réel et démontrable : une autonomie bornée avec
un budget d'étapes appliqué, un journal d'audit signé et vérifiable, un
système de permissions avec supervision humaine et paliers d'autonomie, un
garde-fou qui refuse les commandes shell enchaînées ou redirigées, et un
chemin d'outils gouverné, emprunté aussi bien par les outils natifs que par
les outils MCP. La détection d'injection de prompt ne fait pas partie de cette
liste, et aucun composant de l'arbre ne la fournit : ce que le garde-fou
ci-dessus filtre est l'injection shell. La
dette se situe surtout en périphérie : durcir le sidecar d'inférence, boucler
le volet vérifications shell des contrôles, câbler les plafonds de coût, et
achever la couche copilote. Savoir précisément où se situent ces marges est
tout l'objet de cette page.
