---
sidebar_position: 2
title: 2. Contraintes
---

# 2. Contraintes

L'architecture est bornée par un petit ensemble de règles non négociables. Ce ne sont pas des préférences : elles expliquent pourquoi le système est construit ainsi. La plupart découlent des huit principes qui gouvernent le projet.

## Les huit principes en tant que contraintes

1. **Local-first.** Aucune donnée utilisateur ne quitte la machine sans une action explicite. Cela interdit toute télémétrie silencieuse et oriente chaque comportement par défaut vers le chemin local.
2. **Zéro dépendance externe au runtime.** Le binaire s'exécute sur une machine Linux propre, sans rien de préinstallé. L'inférence, le stockage et l'API sont tous intégrés au processus ou exécutés en sidecar, jamais un service externe requis.
3. **Contrat minimal.** Un agent est une fiche : une mission, un modèle, des outils, des sources et des droits, déclarés dans le manifeste d'un paquet. Le code est optionnel : un paquet apporte des outils et des workflows, et rien d'autre.
4. **Fail fast.** Toute erreur détectable au démarrage est détectée au démarrage, pas en cours d'exécution.
5. **Un acteur, une responsabilité.** Le cœur du runtime est un ensemble d'acteurs Tokio sans état mutable partagé entre eux. Ils communiquent uniquement par message.
6. **Mémoire à l'initiative de l'agent.** Le runtime n'injecte jamais automatiquement de contexte mémoire dans le prompt d'un agent, et n'écrit rien en mémoire à sa place. Un agent se souvient avec l'outil `memory_search` et garde une note avec `memory_write`, quand il choisit de le faire ; cocher Mémoire sur sa fiche accorde les deux. L'assistant conversationnel intégré est le seul endroit où cette injection a lieu, de deux façons, et [les huit principes](/explanation/the-8-principles) précisent lesquelles.
7. **Garde-fous non négociables.** Un budget de pas est imposé par le runtime et ne peut être contourné par un agent.
8. **CLI humaine, API machine.** La CLI est faite pour les personnes (consciente du TTY, avec un `--json` global) ; l'API est faite pour les programmes.

L'énoncé de référence des principes se trouve dans le manuel du projet (`AGENTS.md`). Cette section les traite comme des données d'entrée fixes.

## Contraintes techniques

<!-- claim:daemon-binds-tcp-by-default -->
L'entrée Transport ci-dessous est celle qu'un intégrateur lit le plus souvent à
l'envers : le daemon lie le TCP à chaque démarrage, et seul le runtime embarqué
se limite par défaut au socket Unix.

- **Langage et runtime.** Le cœur est écrit en Rust (1.89+) sur Tokio. Les erreurs utilisent des enums `thiserror`, pas `anyhow`, afin que les échecs restent typés et se traduisent en codes de sortie et en traces structurées. Pas de `unwrap`, `panic`, ni `println` dans les chemins de production.
- **Pont Python.** Le Python qu'un paquet apporte (ses outils et ses workflows) est exécuté dans le processus via un pont PyO3 avec `pyo3-async-runtimes`. Le côté Rust possède le processus ; Python est l'invité. Le plancher des sources du SDK est Python 3.10, et le runtime embarque son propre CPython 3.13.
- **Inférence.** L'inférence locale repose sur `llama-server` embarqué (issu de llama.cpp en amont), sur des modèles GGUF, via son API HTTP compatible OpenAI. Le backend n'est pas figé, il est choisi par artefact publié : Metal sur macOS, et CPU, Vulkan ou CUDA sur Linux et Windows. Sur macOS Apple Silicon, un second moteur embarqué, vllm-mlx, sert les modèles au format MLX. La reconnaissance vocale locale s'appuie sur `whisper`.
- **Persistance.** SQLite avec FTS5, en mode WAL. Pas de base de données externe.
- **Transport.** L'API HTTP est servie sur un socket Unix et sur TCP avec un jeton bearer. `apollia-os start` lie les deux, en prenant le port 7771 quand `--port` est omis. Le socket Unix seul est le comportement par défaut du runtime embarqué, pas du daemon. Windows n'a pas de socket Unix : le transport local y est le tube nommé `\\.\pipe\apollia-runtime-<user>`, qui porte le jeton bearer parce qu'un tube est créé avec un descripteur de sécurité par défaut et non avec le `0600` de la socket.
- **Aucune dépendance injustifiée.** Chaque dépendance tierce, Rust ou Python, constitue une surface de souveraineté et n'est ajoutée qu'avec une décision d'architecture qui la justifie. Le SDK Python se limite à la bibliothèque standard.

## Contraintes organisationnelles

- **La documentation dérive du code.** Les références API, CLI et SDK sont générées depuis la source de vérité et ne sont jamais écrites à la main. Cette section d'architecture pointe vers elles plutôt que de les reformuler.
- **Les décisions sont consignées.** Les choix structurants sont énoncés au présent, une section par sujet sous une ancre stable. Voir [Décisions d'architecture](/architecture/decisions).
