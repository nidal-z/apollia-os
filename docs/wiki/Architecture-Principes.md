# Principes Architecturaux — Les Décisions Qui Guident Tout

> *Ces principes ne sont pas des règles arbitraires. Chacun a été forgé par un problème réel rencontré dans la phase SaaS ou par l'analyse rigoureuse des besoins du projet.*

---

## Principe #1 : Local-first, toujours

**Formulation :** Aucun octet de données utilisateur ne quitte la machine sans une action explicite du développeur.

**Ce que ça signifie concrètement :**
- Le runtime Rust tourne entièrement en local
- La mémoire est un fichier SQLite local
- Les modèles d'embedding (optionnels) sont des fichiers GGUF locaux
- L'audit trail est un fichier SQLite local
- Aucun modèle d'embedding n'est téléchargé automatiquement
- Aucun telemetry, aucun "phone home"

**Pourquoi ce principe existe :**
Le projet SaaS précédent avait une architecture cloud nécessaire par design. La promesse de souveraineté des données se heurtait à la réalité : les données transitaient quand même par l'infrastructure cloud pour être indexées et traitées. Les retours des prospects PME étaient clairs : "On veut bien essayer, mais nos données client ne peuvent pas sortir de chez nous."

La solution n'était pas d'améliorer les garanties contractuelles. C'était de rendre le cloud techniquement inutile.

**Conséquence architecturale :**
Toute dépendance à un service externe est optionnelle et doit se dégrader gracieusement. FTS5 avant les embeddings. Pas d'embedding avant `local_gguf` ou `ollama`. Jamais de "ça ne marchera pas si vous n'avez pas de connexion internet."

---

## Principe #2 : Zéro dépendance externe côté runtime

**Formulation :** Le binaire Apollia OS doit fonctionner sur n'importe quel Linux avec zéro installation préalable.

**Ce que ça signifie concrètement :**
- Pas de Docker requis
- Pas de Node.js requis
- Pas de base de données externe (PostgreSQL, Redis, Qdrant)
- Pas de Python côté runtime (PyO3 intègre l'interpréteur)
- Un seul fichier binaire à distribuer

**Pourquoi ce principe existe :**
Le SaaS précédent nécessitait 6 services d'infrastructure pour tourner (PostgreSQL, DragonflyDB, Qdrant, MinIO, Keycloak, Traefik). Chaque service était une source de friction à l'installation, une surface d'attaque supplémentaire, et un composant à maintenir.

Pour un runtime d'agents IA qui cible des développeurs individuels et des entreprises avec des DSI réticents, la complexité opérationnelle est un veto commercial.

**Conséquence architecturale :**
SQLite remplace PostgreSQL + Qdrant + Redis. Les namespaces Linux remplacent Docker. PyO3 intègre Python. Un seul `cargo install apollia-os` suffit.

---

## Principe #3 : Contrat minimal, friction zéro

**Formulation :** Un agent existant doit pouvoir tourner dans Apollia OS avec moins de 10 lignes de code d'adaptation.

**Ce que ça signifie concrètement :**
- L'AIP supporte le duck typing Python (pas de classe de base obligatoire)
- `manifest()` et `run()` suffisent pour un agent minimal
- Des wrappers d'adaptation sont fournis pour LangGraph et CrewAI
- La validation du manifest se fait au démarrage (fail fast), pas à l'exécution

**Pourquoi ce principe existe :**
Les runtimes qui imposent un framework (Actix actors, Erlang OTP, Actor Model strict) ont une courbe d'adoption élevée. Le développeur doit d'abord apprendre le paradigme avant de pouvoir l'utiliser.

Apollia OS résout un problème d'infrastructure. Si adopter la solution nécessite de réécrire l'agent, la solution crée autant de travail qu'elle en économise.

**Conséquence architecturale :**
AIP = duck typing. `hasattr(agent, 'manifest') and hasattr(agent, 'run')` suffit à la validation. La classe de base `AIPAgent` est optionnelle. Le `AIPWrapper` permet d'encapsuler n'importe quel callable async.

---

## Principe #4 : Fail fast, pas de surprises à l'exécution

**Formulation :** Toute erreur détectable au démarrage doit être détectée au démarrage, jamais silencieusement au milieu d'une tâche.

**Ce que ça signifie concrètement :**
- Validation stricte du manifest à `INITIALIZING`
- Résolution des `tools_required` à `INITIALIZING` — outil absent = agent ne démarre pas
- Installation des packages Python à `INITIALIZING` — package manquant = erreur au démarrage
- Connexion aux serveurs MCP à `INITIALIZING` — serveur inaccessible = `ProcessState.DEGRADED` ou `STOPPED`

**Pourquoi ce principe existe :**
Un agent qui démarre avec succès et plante à la 3ème étape de sa 2ème tâche parce qu'un outil n'est pas disponible est un désastre en production. Le bug est difficile à reproduire, le log d'erreur est cryptique, l'utilisateur est frustré.

"Fail fast" est le principe de conception qui transforme des bugs de production en erreurs de configuration détectables avant le déploiement.

**Conséquence architecturale :**
`tools_required` vs `tools_optional` distinction explicite. `DEGRADED` vs `STOPPED` pour les outils manquants. Toute la phase `INITIALIZING` est de la validation — l'agent ne passe à `ACTIVE` que si tout est prêt.

---

## Principe #5 : Un acteur, une responsabilité

**Formulation :** Le Runtime Core n'est pas un monolithe interne. Chaque responsabilité est un acteur Tokio distinct.

**Ce que ça signifie concrètement :**
- `EventBus` : diffusion d'événements uniquement
- `AgentRegistry` : inventaire des agents uniquement
- `TaskRouter` : dispatch des tâches uniquement
- `ExecutionCoordinator` : interface avec ORIA par agent uniquement
- `APIServer` : exposition externe uniquement
- Chaque acteur communique par messages, jamais par état partagé

**Pourquoi ce principe existe :**
Le projet SaaS précédent avait des services qui faisaient trop de choses. Quand un bug apparaissait dans le pipeline de traitement, il était difficile de déterminer dans quelle couche il se trouvait.

Le modèle acteur Tokio (inspiré d'Alice Ryhl's blog post canonique sur les acteurs Tokio) force la séparation des responsabilités par construction : chaque acteur possède exclusivement son état, et toute interaction passe par des messages explicites.

**Conséquence architecturale :**
Pattern `mpsc::channel` + `HashMap` état interne + `JoinHandle` Tokio pour chaque acteur. `Handle` séparé pour l'API publique. Pas d'état partagé entre acteurs (pas de `Arc<Mutex<...>>` traversant les frontières d'acteurs).

---

## Principe #6 : La mémoire à l'initiative de l'agent

**Formulation :** Le runtime n'injecte jamais automatiquement de mémoire dans le contexte d'un agent. C'est toujours l'agent qui décide ce qu'il récupère et comment il l'utilise.

**Ce que ça signifie concrètement :**
- `ctx.memory.search()` est appelé explicitement par l'agent
- Le runtime ne pré-charge pas de contexte mémoriel dans le prompt de l'agent
- Pas de consolidation automatique des épisodes en background en MVP
- L'agent contrôle ce qu'il mémorise via `ctx.memory.record()` et `ctx.memory.remember()`

**Pourquoi ce principe existe :**
La "mémoire automatique" est séduisante en théorie. En pratique, elle génère des appels LLM non contrôlés (résumés automatiques, extraction de faits...), des coûts imprévisibles, des comportements difficiles à debugger, et des risques de perte d'information par consolidation trop agressive.

La littérature sur les agents IA 2025 identifie la latence due au traitement mémoriel constant comme l'un des principaux goulots d'étranglement des agents en production.

**Conséquence architecturale :**
`MemoryInterface` est une API appelée explicitement par l'agent. Pas de hook de lifecycle qui injecte automatiquement. La consolidation sera une feature opt-in v1.0, jamais un comportement par défaut.

---

## Principe #7 : Les garde-fous sont non négociables

**Formulation :** Tout agent, quel que soit son code, est soumis aux limites du StepBudget et du ResilienceLayer.

**Ce que ça signifie concrètement :**
- `max_steps`, `max_tool_calls`, et `wall_clock_timeout` sont appliqués par le runtime, pas par l'agent
- Un agent ne peut pas se soustraire à ces limites depuis son code Python
- Les valeurs par défaut sont conservatives (10 steps, 20 tool_calls, 5 minutes)
- Un agent peut déclarer des limites supérieures dans son manifest (opt-in explicite)

**Pourquoi ce principe existe :**
Les boucles infinies et les coûts LLM incontrôlés sont les deux causes de mort les plus communes des agents en production. Un agent qui loupe sa condition d'arrêt peut générer des centaines d'appels LLM et des coûts en heures.

En production PME, ce type d'incident est inacceptable. Le runtime doit être la couche de sécurité sur laquelle on peut compter indépendamment de la qualité du code de l'agent.

**Conséquence architecturale :**
`StepBudget` implémenté dans `ExecutionCoordinator` (Rust), pas dans l'agent Python. L'agent reçoit `ctx.step_budget` en lecture seule pour adapter son comportement proactivement — mais il ne peut pas le désactiver.

---

## Principe #8 : La CLI est pour les humains, l'API est pour les machines

**Formulation :** La CLI doit être utilisable par un administrateur PME non-développeur. L'API REST doit être exploitable par n'importe quel script bash.

**Ce que ça signifie concrètement :**
- Commandes de niveau 1 lisibles et mémorisables sans documentation (`start`, `stop`, `status`, `run`)
- Sorties humaines par défaut (tableaux colorés, indicateurs visuels)
- `--json` disponible sur toutes les commandes pour les scripts
- TTY auto-détecté (couleurs désactivées hors terminal)
- Exit codes standards Unix

**Pourquoi ce principe existe :**
La CLI est la première impression d'Apollia OS pour un développeur qui découvre le projet. Une CLI cryptique ou qui suppose une connaissance de l'architecture interne est un obstacle à l'adoption.

Les meilleures CLIs techniques (docker, kubectl, git) ont en commun : un onboarding progressif, des messages d'erreur actionnables, et un comportement prévisible.

**Conséquence architecturale :**
Pattern `noun verb` cohérent (`apollia-os agent start`, `apollia-os task list`). Onboarding `apollia-os` sans argument qui explique les commandes disponibles. `--json` global, pas par commande.

---

## Résumé des 8 principes

| # | Principe | Résumé |
|---|---|---|
| 1 | Local-first, toujours | Zéro cloud dans le chemin d'exécution |
| 2 | Zéro dépendance externe | Un binaire, aucun service requis |
| 3 | Contrat minimal | Duck typing, 10 lignes d'adaptation maximum |
| 4 | Fail fast | Les erreurs détectables au démarrage le sont au démarrage |
| 5 | Un acteur, une responsabilité | Modèle acteur Tokio sans état partagé |
| 6 | Mémoire à l'initiative de l'agent | Pas d'injection automatique, coûts prévisibles |
| 7 | Garde-fous non négociables | StepBudget et résilience appliqués par le runtime |
| 8 | CLI humaine, API machine | Deux audiences, deux interfaces, un seul outil |

---

*Prochaine lecture recommandée : [Vue d'ensemble technique & AIP](./Architecture-Vue-Ensemble)*
