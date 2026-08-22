---
sidebar_position: 9
title: Tirer le meilleur parti de l'inférence locale
---

# Tirer le meilleur parti de l'inférence locale

L'inférence LLM locale est servie par un `llama-server` embarqué (le projet
llama.cpp en amont) que le daemon lance et supervise via son API HTTP compatible
OpenAI. C'est le moteur local intégré, et le seul : il n'y a aucun processus
séparé à installer, à lancer, ni vers lequel pointer Apollia. Il est livré
préconstruit dans l'application desktop, et sur une build depuis les sources, le
daemon retrouve `llama-server` sur votre `PATH` (voir
[Installer et lancer le runtime](/how-to/install-and-run#local-gguf-inference)).

Deux capacités qui exigeaient auparavant un serveur additionnel lancé à la main
sont désormais actives par défaut, parce que le moteur embarqué est ce serveur
:

- **Batching continu.** Le moteur décode plusieurs séquences dans la même passe
  GPU : les requêtes concurrentes et par lot se partagent le matériel au lieu de
  se sérialiser l'une derrière l'autre. Rien à activer.
- **Appel d'outils natif.** Le moteur est piloté avec `--jinja`, si bien que les
  appels d'outils passent par le template de conversation propre au modèle plutôt
  que par un chemin de grammaire sur mesure. Les modèles locaux appellent vos
  outils de façon fiable, sans réglage de votre côté.

Suivre llama.cpp en amont élargit aussi la couverture de modèles : les nouvelles
architectures arrivent dans le moteur au fur et à mesure qu'elles arrivent en
amont.

## Configurer un backend local

Enregistrez un modèle `.gguf` comme backend local et laissez le daemon le
servir. Le nom du fournisseur est `llama-cpp` :

```sh
apollia-os llm setup --local --model /path/to/model.gguf
apollia-os llm reload
apollia-os llm status
```

Le daemon démarre le `llama-server` embarqué pour ce modèle à la demande et lui
route l'inférence. Le streaming SSE et les appels d'outils empruntent le même
chemin, déjà câblé et testé.

## Obtenir un bon débit

Le moteur gère la mécanique (déport GPU, batching, cache KV). Les choix qui font
varier le débit se situent en amont de lui :

- **Choisir un modèle dimensionné pour votre matériel.** Un modèle
  mixture-of-experts (MoE) n'active qu'une fraction de ses paramètres par token,
  ce qui lui permet de dépasser un modèle dense de qualité comparable à la fois
  en vitesse et en débit par lot. Préférez une quantization qui laisse de la
  marge pour le cache KV.
- **Servir un seul modèle par processus serveur.** Le moteur charge un modèle
  GGUF mono-fichier. Changer le backend par défaut change le modèle que le
  daemon sert.
- **Provisionner les slots avant d'attendre de la concurrence.** Le batching
  continu est actif par défaut, mais le moteur démarre avec un seul slot de
  décodage, si bien que les requêtes s'empilent au lieu de décoder ensemble.
  Augmentez `APOLLIA_LLAMA_N_PARALLEL` (voir plus bas).

## Régler le moteur embarqué

<!-- claim:llama-server-env-overrides -->

Le moteur embarqué lit douze variables d'environnement à chaque démarrage, si
bien qu'un réglage peut être modifié sans build depuis les sources. Définissez-
les dans l'environnement de ce qui lance le daemon.

| Variable | Défaut | Ce qu'elle fait |
|---|---|---|
| `APOLLIA_LLAMA_N_CTX` | `32768` | Fenêtre de contexte en tokens. La valeur par défaut est fixe, elle n'est pas lue dans le modèle. |
| `APOLLIA_LLAMA_N_GPU_LAYERS` | `999` | Couches déportées vers le GPU ; `0` force le CPU. |
| `APOLLIA_LLAMA_N_BATCH` | défaut du moteur | Taille de batch logique. |
| `APOLLIA_LLAMA_N_UBATCH` | défaut du moteur | Taille du micro-batch physique. |
| `APOLLIA_LLAMA_N_PARALLEL` | `1` | Slots de décodage servis simultanément. |
| `APOLLIA_LLAMA_CONT_BATCHING` | `true` | Batching continu. |
| `APOLLIA_LLAMA_CACHE_TYPE_K` | défaut du moteur | Quantization du cache KV, clés. |
| `APOLLIA_LLAMA_CACHE_TYPE_V` | défaut du moteur | Quantization du cache KV, valeurs. |
| `APOLLIA_LLAMA_FLASH_ATTN` | `on` | Mode flash attention. |
| `APOLLIA_LLAMA_CACHE_REUSE` | défaut du moteur | Seuil de réutilisation de préfixe. |
| `APOLLIA_LLAMA_METRICS` | `false` | Expose le point de terminaison de métriques du moteur. |
| `APOLLIA_LLAMA_EXTRA_ARGS` | vide | Options supplémentaires transmises telles quelles. |

Augmenter `N_PARALLEL` est ce qui transforme le batching continu en véritable
concurrence : avec le défaut d'un seul slot, les requêtes s'empilent.

La liste complète, y compris les variables de stockage de secrets et de
diagnostic, se trouve dans
[Variables d'environnement](/reference/environment-variables).

## Développeur : lancer un `llama-server` séparé et réglé sur mesure

Sur une build depuis les sources, le daemon utilise le `llama-server` qu'il
trouve sur votre `PATH` plutôt qu'un binaire embarqué. Le dépôt fournit une
recette pour en lancer un pour des tests locaux :

```sh
just llama-server /path/to/model.gguf
```

Cette recette lance le binaire en amont, si bien que les options habituelles de
llama.cpp s'appliquent lorsque vous expérimentez en local : contexte total
(`-c`) réparti entre slots parallèles (`-np`), déport GPU (`-ngl`), flash
attention (`--flash-attn on`), et cache KV quantifié (`-ctk q8_0 -ctv q8_0`, qui
nécessite flash attention). Ce sont des options llama.cpp en amont, utiles pour
sonder ce que votre matériel encaisse avant d'arrêter votre choix sur un modèle
et une quantization.

## Voir aussi

- [Installer et lancer le runtime](/how-to/install-and-run) pour la build et
  l'exigence de `PATH` sur une build depuis les sources.
- [Déployer en production](/how-to/deploy-in-production) pour servir depuis un
  serveur.
- La [référence CLI](/reference/cli) pour les commandes `llm`.
