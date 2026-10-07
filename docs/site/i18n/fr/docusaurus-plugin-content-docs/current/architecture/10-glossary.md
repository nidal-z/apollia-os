---
sidebar_position: 10
title: 10. Glossaire
---

# 10. Glossaire

Termes clés utilisés dans cette section et dans le reste de la documentation.

| Terme | Signification |
|---|---|
| **Agent** | Une mission, des exemples et les outils qu'il peut atteindre, écrits comme une donnée : une entrée `[[agents]]` du `apollia.toml` d'un paquet, ou un agent créé dans l'application. Le moteur intégré au runtime l'exécute dans une boucle ReAct, sous la gouvernance du runtime. Aucun Python ne représente un agent. |
| **Paquet** | Un dossier qui porte un `apollia.toml` à sa racine, distribué sous la forme d'une archive `.apo` : toute combinaison d'agents, d'outils et de workflows. Un paquet sans agent est valide. |
| **Outil** | Ce qu'un agent peut se voir accorder et appeler : un outil natif du runtime, un outil MCP (`mcp:<server>/<tool>`), ou un outil Python d'un paquet (`pkg:<package>/<tool>`), une fonction `async` sous le décorateur `@tool`. |
| **Workflow** | Une fonction Python `async` d'un paquet sous le décorateur `@workflow`, dont les étapes sont déclarées d'avance et qui peut exécuter des agents, exécuter d'autres workflows et poser une question à l'utilisateur. Adressé par `<package>/<workflow>`. |
| **Skill** | Une capacité typée et invocable adressée par un `skill_id` sur le chemin A2A. Aucun décorateur du SDK n'en déclare : le Python d'un paquet apporte des outils et des workflows. |
| **A2A** | Invocation agent-à-agent : un agent qui appelle le skill d'un autre agent via son `skill_id`, avec des garde-fous de profondeur, d'auto-appel, de délai d'expiration et d'échéance de chaîne. |
| **ORIA** | Le moteur d'exécution autonome (`apollia-oria`) : la boucle ReAct, le planificateur, le budget, la résilience, la vérification et la gestion du contexte. |
| **Direct vs orchestré** | Les deux modes d'exécution d'ORIA. Le mode direct exécute une seule boucle d'agent ; le mode orchestré planifie et pilote des étapes d'outils gouvernées, avec vérification et re-planification. |
| **ReAct** | La boucle « raisonner puis agir » qu'exécute un agent : réfléchir, appeler un outil, observer, recommencer. |
| **StepBudget** | Le plafond non contournable d'étapes de raisonnement, d'appels d'outils et de temps réel que le runtime impose à chaque exécution. |
| **ctx** | Le contexte runtime transmis à chaque outil et à chaque workflow d'un paquet, exposant dix-sept services typés. Le contrat est `sdk/apollia/types.py`. Voir la [référence du SDK](/reference/sdk). |
| **AgentKit** | Le SDK Python (`apollia`) : les deux décorateurs, les schémas, les doublures de test et les fonctions utilitaires contre lesquels s'écrit le Python d'un paquet. |
| **MCP** | Model Context Protocol. Apollia est un client MCP qui découvre et appelle des outils externes, et peut exposer un serveur MCP entrant limité. |
| **Palier d'autonomie** | Le réglage propre à une exécution qui gouverne la porte du plan et la passe de vérification après exécution. Il ne gouverne aucune permission ni aucune approbation : il change la distance qu'une exécution parcourt sans surveillance, pas ce qu'un agent peut toucher. |
| **HITL** | Humain dans la boucle : une approbation qu'une personne doit trancher avant l'exécution d'une action à conséquences. La décision est enregistrée. |
| **Journal d'audit** | Le registre en ajout seul, chaîné par hachage et signé, des actions gouvernées, utilisé pour la vérification. |
| **Vérification** | Contrôler la chaîne de hachage d'audit et les signatures d'une exécution pour confirmer que l'enregistrement n'a pas été altéré. |
| **Rollback** | Annuler les modifications du système de fichiers effectuées au cours d'une session de chat. Le format du journal et la logique de replay existent dans le code, mais rien n'écrit dans ce journal : ce n'est donc pas une capacité de cette version. |
| **Replay** | Rejouer et comparer une exécution. Abandonné par décision ; ce n'est pas une capacité. |
| **Runner** | Le sidecar hors processus (`apollia-runner`) qui exécute la reconnaissance vocale locale (whisper). L'inférence LLM locale est assurée par le `llama-server` embarqué (upstream llama.cpp), que le démon supervise. |
| **GGUF** | Le format de modèle local à fichier unique que charge le runner. |
| **Contrat pilote** | L'API HTTP stable et versionnée, ainsi que les SDK hôtes générés, qu'un produit hôte utilise pour piloter le runtime. Voir la [référence de l'API HTTP](/reference/api/apollia-os-runtime-api). |
| **EventBus** | Le flux d'événements structurés du runtime, partagé par le journal d'audit et l'observabilité. |
| **Souveraineté** | La propriété qu'aucune donnée utilisateur ne quitte la machine sans une action explicite, avec inférence et stockage locaux par défaut. |
