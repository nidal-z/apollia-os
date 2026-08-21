---
sidebar_position: 10
title: 10. Glossaire
---

# 10. Glossaire

Termes clés utilisés dans cette section et dans le reste de la documentation.

| Terme | Signification |
|---|---|
| **Agent** | Un processus Python qui raisonne et agit de façon autonome dans une boucle ReAct, sous la gouvernance du runtime. Il expose, par typage structurel, une `manifest()` et un `run()` asynchrone. |
| **Worker** | Un agent qui expose un ou plusieurs skills typés que d'autres agents peuvent appeler. L'expert de domaine au sein d'une équipe. |
| **Director** | Un agent qui orchestre des workers en appelant leurs skills. |
| **Skill** | Une capacité typée et invocable qu'un agent expose, déclarée avec le décorateur `@skill` et adressée par un `skill_id`. |
| **A2A** | Invocation agent-à-agent : un agent qui appelle le skill d'un autre agent via son `skill_id`, avec des garde-fous de profondeur, d'auto-appel, de délai d'expiration et d'échéance de chaîne. |
| **ORIA** | Le moteur d'exécution autonome (`apollia-oria`) : la boucle ReAct, le planificateur, le budget, la résilience, la vérification et la gestion du contexte. |
| **Direct vs orchestré** | Les deux modes d'exécution d'ORIA. Le mode direct exécute une seule boucle d'agent ; le mode orchestré planifie et pilote des étapes d'outils gouvernées, avec vérification et re-planification. |
| **ReAct** | La boucle « raisonner puis agir » qu'exécute un agent : réfléchir, appeler un outil, observer, recommencer. |
| **StepBudget** | Le plafond non contournable d'étapes de raisonnement, d'appels d'outils et de temps réel que le runtime impose à chaque exécution. |
| **ctx** | Le contexte runtime transmis à chaque gestionnaire d'agent, exposant quinze services typés. Le contrat est `sdk/apollia/types.py`. Voir la [référence du SDK](/reference/sdk). |
| **AgentKit** | Le SDK Python (`apollia`) : les décorateurs, schémas, harnais et fonctions utilitaires contre lesquels un auteur écrit son code. |
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
