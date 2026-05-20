# Annexe C. Principes architecturaux

Les 8 principes non-négociables qui guident chaque décision dans Apollia OS. Chacun a été forgé par un problème réel rencontré dans la phase SaaS précédente ou par l'analyse rigoureuse des besoins du projet.

---

## Principe #1 : Local-first, toujours

**Formulation :** Aucun octet de données utilisateur ne quitte la machine sans une action explicite du développeur.

Concrètement : runtime Rust entièrement local, mémoire SQLite locale, modèles d'embedding GGUF locaux, audit trail SQLite local, aucun telemetry.

**Pourquoi :** L'architecture SaaS cloud précédente rendait impossible la souveraineté réelle des données. Les retours des prospects PME étaient clairs : "On veut bien essayer, mais nos données client ne peuvent pas sortir de chez nous." La solution n'était pas d'améliorer les garanties contractuelles. Il fallait rendre le cloud techniquement inutile.

**Conséquence architecturale :** Toute dépendance à un service externe est optionnelle et se dégrade gracieusement. SQLite remplace PostgreSQL/Redis/Qdrant.

---

## Principe #2 : Zéro dépendance externe côté runtime

**Formulation :** Le binaire Apollia OS doit fonctionner sur n'importe quel Linux avec zéro installation préalable.

Concrètement : pas de Docker requis, pas de Node.js, pas de base de données externe, pas de Python côté runtime (PyO3 intègre l'interpréteur), un seul fichier binaire.

**Pourquoi :** Le projet précédent nécessitait plusieurs services d'infrastructure à déployer et maintenir. Chaque service était une source de friction à l'installation et un composant à maintenir. Pour un runtime qui cible des développeurs individuels et des DSI réticentes, la complexité opérationnelle est un veto commercial.

**Conséquence architecturale :** `cargo install apollia-os` suffit. SQLite remplace PostgreSQL + Qdrant + Redis. Les namespaces Linux remplacent Docker. PyO3 intègre Python.

---

## Principe #3 : Contrat minimal, friction zéro

**Formulation :** Un agent existant doit pouvoir tourner dans Apollia OS avec moins de 10 lignes de code d'adaptation.

Concrètement : AIP supporte le duck typing Python, `manifest()` et `run()` suffisent pour un agent minimal, wrappers fournis pour LangGraph et CrewAI.

**Pourquoi :** Les runtimes qui imposent un framework ont une courbe d'adoption élevée. Apollia OS résout un problème d'infrastructure. Si adopter la solution nécessite de réécrire l'agent, la solution crée autant de travail qu'elle en économise.

**Conséquence architecturale :** AIP = duck typing. `hasattr(agent, 'manifest') and hasattr(agent, 'run')` suffit à la validation. La classe de base est optionnelle.

---

## Principe #4 : Fail fast, pas de surprises à l'exécution

**Formulation :** Toute erreur détectable au démarrage doit être détectée au démarrage, jamais silencieusement au milieu d'une tâche.

Concrètement : validation stricte du manifest à `INITIALIZING`, résolution des `tools_required` au démarrage, connexion aux serveurs MCP à `INITIALIZING`.

**Pourquoi :** Un agent qui démarre avec succès et plante à la 3ème étape de sa 2ème tâche parce qu'un outil n'est pas disponible est un désastre en production. "Fail fast" transforme des bugs de production en erreurs de configuration détectables avant le déploiement.

**Conséquence architecturale :** Distinction explicite `tools_required` vs `tools_optional`. `DEGRADED` vs `STOPPED` pour les outils manquants. L'agent ne passe à `ACTIVE` que si tout est prêt.

---

## Principe #5 : Un acteur, une responsabilité

**Formulation :** Le Runtime Core n'est pas un monolithe interne. Chaque responsabilité est un acteur Tokio distinct.

Concrètement : `EventBus` diffuse uniquement, `AgentRegistry` inventorie uniquement, `TaskRouter` dispatche uniquement. Chaque acteur communique par messages, jamais par état partagé.

**Pourquoi :** Le projet SaaS précédent avait des services qui faisaient trop de choses. Quand un bug apparaissait, il était difficile de déterminer dans quelle couche il se trouvait. Le modèle acteur Tokio force la séparation des responsabilités par construction.

**Conséquence architecturale :** Pattern `mpsc::channel` + état interne + `JoinHandle` Tokio pour chaque acteur. Pas d'état partagé entre acteurs (pas de `Arc<Mutex<...>>` traversant les frontières).

---

## Principe #6 : La mémoire à l'initiative de l'agent

**Formulation :** Le runtime n'injecte jamais automatiquement de mémoire dans le contexte d'un agent. C'est toujours l'agent qui décide ce qu'il récupère et comment il l'utilise.

Concrètement : `ctx.memory.search` est appelé explicitement, le runtime ne pré-charge pas de contexte mémoriel dans le prompt.

**Pourquoi :** La "mémoire automatique" génère des appels LLM non contrôlés, des coûts imprévisibles, et des comportements difficiles à debugger. Chaque injection automatique ajoute un appel LLM et 1-3 secondes de latence.

**Conséquence architecturale :** `MemoryInterface` est une API appelée explicitement. Pas de hook de lifecycle qui injecte automatiquement. La consolidation sera une feature opt-in v1.0, jamais un comportement par défaut.

---

## Principe #7 : Les garde-fous sont non négociables

**Formulation :** Tout agent, quel que soit son code, est soumis aux limites du StepBudget et du ResilienceLayer.

Concrètement : `max_steps`, `max_tool_calls`, et `wall_clock_timeout` sont appliqués par le runtime Rust. Un agent ne peut pas se soustraire à ces limites depuis son code Python.

**Pourquoi :** Les boucles infinies et les coûts LLM incontrôlés sont des risques fréquemment observés dans les déploiements d'agents IA. En production PME, ce type d'incident est inacceptable. Le runtime doit être la couche de sécurité sur laquelle on peut compter indépendamment de la qualité du code de l'agent.

**Conséquence architecturale :** `StepBudget` implémenté dans `ExecutionCoordinator` (Rust), pas dans l'agent Python. L'agent reçoit `ctx.step_budget` en lecture seule pour adapter son comportement proactivement.

---

## Principe #8 : La CLI est pour les humains, l'API est pour les machines

**Formulation :** La CLI doit être utilisable par un administrateur PME non-développeur. L'API REST doit être exploitable par n'importe quel script bash.

Concrètement : commandes de niveau 1 lisibles sans documentation (`start()`, `stop()`, `status`, `run()`), `--json` disponible sur toutes les commandes, TTY auto-détecté, exit codes standards Unix.

**Pourquoi :** La CLI est la première impression d'Apollia OS. Une CLI cryptique qui suppose une connaissance de l'architecture interne est un obstacle à l'adoption. Les meilleures CLIs techniques (docker, kubectl, git) ont en commun un onboarding progressif et des messages d'erreur actionnables.

**Conséquence architecturale :** Pattern `noun verb` cohérent. `apollia-os` sans argument explique les commandes disponibles. `--json` global, pas par commande.

---

## Résumé

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
