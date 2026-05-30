# Annexe B. Glossaire

Termes techniques et conventions utilisés dans le book et dans le code Apollia OS. Ordre alphabétique.

---

**A2A (Agent-to-Agent).** Mécanisme d'invocation d'une skill d'un agent depuis un autre agent. Exposé via `ctx.a2a.invoke(skill_id, input=...)`. Cf. [chapitre 14](../part-iii-the-ctx-protocol/14-ctx-a2a.md).

**Agent.** Une classe Python décorée par `@agent`. Possède un manifeste (`__apollia_manifest__`), au moins un handler (`@skill`, `@on_message` ou `@orchestrated`), et est instanciée une fois au load du module.

**AIPResult.** Format interne du SDK pour les réponses d'agents. L'agent ne le manipule jamais directement : il retourne un `dict` ou lève une exception typée (`DomainError`, `NeedHumanInput`), et le boundary du dispatcher emballe en `AIPResult.completed`, `AIPResult.failed`, ou `AIPResult.input_required`.

**Annotated.** `typing.Annotated[T, "description"]`. Permet d'ajouter une description à un paramètre de skill qui apparaît dans l'`input_schema` exposé au LLM. Cf. [chapitre 19](../part-iv-llm-friendly-design/19-annotated-descriptions.md).

**APOLLIA.md.** Fichier markdown à la racine d'un workspace qui contient les règles et sections du projet. Lu par le runtime et exposé via `ctx.workspace`. Cf. [chapitre 18](../part-iii-the-ctx-protocol/18-ctx-other-services.md).

**Audit trail.** Journal append-only des invocations d'outils, tracé en SQLite local (`~/.apollia/audit.db`). Pas d'export externe par défaut.

**Boundary.** Couche du SDK (`apollia._internal.dispatch`) qui sépare le code utilisateur (l'agent) du runtime. Elle valide les inputs contre le schéma, trappe les exceptions typées, et formate la réponse en `AIPResult`.

**BudgetView.** Surface en lecture seule du `StepBudget` du runtime, exposée via `ctx.budget`. Permet à l'agent de connaître son budget restant sans pouvoir le contourner.

**Capstone.** Le projet end-to-end de la [Partie IX](../part-ix-capstone/37-capstone-overview.md) qui consolide tous les patterns du book : préparation de RDV commercial PME.

**Conversational (agent).** Agent qui expose `@on_message`. Répond à un humain en flux dans le chat Apollia. Pattern 1 des 4 quickstarts.

**Ctx.** Le `typing.Protocol` injecté dans chaque invocation. Expose 14 services nestés. Cf. [chapitre 10](../part-iii-the-ctx-protocol/10-ctx-overview.md).

**Datasource.** Fichier YAML déclaré dans `@agent(datasources=(...))`, lu via `ctx.datasources.get(name)`. Read-only, gating strict.

**Decorator-first.** Style canonique du SDK v0.5 : un décorateur de classe `@agent` + des décorateurs de méthode `@skill`, `@on_message`, `@orchestrated`. Remplace l'héritage `BaseReActAgent` / `WorkerAgent` / etc. de la v0.4.

**Director (agent).** Agent qui orchestre d'autres agents via A2A et `apollia.react`. Pattern 3 des 4 quickstarts.

**DomainError.** Exception typée levée par un agent pour signaler une erreur métier connue. Trappe par le boundary, devient `AIPResult.failed(code, message, details)`. Cf. [chapitre 22](../part-v-error-handling/22-domain-errors.md).

**EventBus.** Acteur Tokio interne au runtime qui diffuse les `RuntimeEvent` (TaskCompleted, AgentReady, etc.) via `broadcast::channel`. Pas d'API publique côté agent : utilisez `ctx.events` pour émettre des événements destinés à l'UI.

**Examples.** Liste de payloads d'exemple sur une skill : `@skill("foo.bar", examples=[{...}])`. Propagés au tool descriptor LLM. Cf. [chapitre 20](../part-iv-llm-friendly-design/20-examples-payloads.md).

**Fail-fast.** Principe architectural #4. Toute incohérence détectable au démarrage (signature invalide, datasource manquante, secret non configuré) est signalée à l'import ou au boot, jamais silencieusement au milieu d'une tâche.

**Gating.** Le manifeste d'un agent déclare exhaustivement ce qu'il peut consommer : `datasources`, `templates`, `secrets`, `tools_required`. Tout accès à une ressource non déclarée est rejeté (pour datasources / templates / secrets) ou refusé par le permission engine (pour tools).

**Handle.** Pattern Rust utilisé par chaque acteur Tokio du runtime : une struct clonable qui contient un `mpsc::Sender` vers l'acteur. C'est l'unique interface publique. Cf. [chapitre 30](../part-viii-runtime-rust/30-actors-supervisor.md).

**HITL (Human-In-The-Loop).** Mécanisme qui suspend une tâche en attente d'une décision humaine. Déclenché soit par `raise NeedHumanInput(...)` côté agent, soit par `requires_approval=True` sur une skill ou un outil. Cf. [chapitre 23](../part-v-error-handling/23-need-human-input.md).

**Isomorphique (test).** Test qui utilise la même surface (Ctx Protocol) que le runtime de production, mais avec un mock injecté. Fourni par `apollia.testing.mock`. Pas de duplication d'API entre prod et tests.

**LLM Router.** Composant du runtime qui dispatche les appels `ctx.llm` vers le backend choisi (local llama.cpp, Ollama, Anthropic, OpenAI, Vertex). Transparent côté agent.

**Manifeste.** Dict canonique d'un agent, généré par `@agent` au load et caché sur `cls.__apollia_manifest__`. Contient name, version, skills, datasources, templates, secrets, tools_required, etc.

**MCP (Model Context Protocol).** Protocole standard pour exposer des outils à un agent IA. Apollia inclut un client MCP qui route les appels préfixés `mcp:<server>/<name>` vers le serveur MCP correspondant.

**Memory namespace.** Préfixe d'isolation pour la mémoire (`ctx.memory`). Par défaut = nom de l'agent. Override possible via `@agent(memory_namespace="...")`.

**NeedHumanInput.** Exception typée levée par un agent pour suspendre la tâche en attente d'une réponse humaine. Devient `AIPResult.input_required(prompt, context)`. Cf. [chapitre 23](../part-v-error-handling/23-need-human-input.md).

**ORIA (Observer, Reasoner, Actor).** Moteur de plan dynamique côté Rust qui pilote les agents `@orchestrated`. Construit un plan via le LLM, l'exécute, replanifie si besoin.

**Orchestrated (agent).** Agent décoré par `@orchestrated(system_prompt=...)`. Le moteur ORIA pilote la boucle d'exécution. Pattern 4 des 4 quickstarts.

**PermissionEngine.** Composant Rust qui applique 3 couches de règles (session / project / global) sur chaque appel d'outil. Cf. [chapitre 35](../part-viii-runtime-rust/35-tools-sandbox-permissions.md).

**Profile.** Profil utilisateur global, exposé via `ctx.profile`. Clés conventionnellement préfixées `user.*`. Lecture toujours autorisée ; écriture gated par `@agent(user_memory_write=True)`.

**Protocol.** `typing.Protocol` (PEP 544). Permet le structural typing : un objet implémente un Protocol s'il a les attributs et méthodes attendus, sans héritage explicite. Utilisé pour tout le Ctx.

**PyO3.** Crate Rust qui crée le pont entre Rust et Python. Apollia l'utilise dans `apollia-aip` pour appeler les agents Python depuis le runtime Tokio sans passer par HTTP.

**ReAct.** Pattern Reason+Act : le LLM raisonne, choisit un outil, observe le résultat, recommence jusqu'à converger. Exposé via `apollia.react(ctx, system, user, tools=..., max_steps=...)` comme fonction libre.

**Sandbox.** Isolation des outils natifs via Linux user namespaces (`unshare --user --mount --pid --net --fork`). Cf. [chapitre 35](../part-viii-runtime-rust/35-tools-sandbox-permissions.md).

**Schema (input/output).** JSON Schema inféré depuis la signature d'une `@skill` Python. Le SDK utilise `typing.get_type_hints(fn, include_extras=True)` pour préserver `Annotated`. TypedDict pour les structures imbriquées.

**Secret.** Credential (clé API, token OAuth) stocké chiffré en local. Déclaré dans `@agent(secrets=(...))`, lu via `ctx.secrets.get(key)`. Read-only côté agent.

**SkillCard.** Métadonnées d'une skill A2A : `skill_id`, `description`, `agent_name`, `input_schema`, `output_schema`. Retournée par `ctx.a2a.discover(skill_id)`.

**Skill ID.** Identifiant en `dot.snake_case` minuscule, regex `^[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*)*$`. Convention : préfixer par le domaine (`pdf.read_text`, `web.search`, `crm.lookup.account`).

**StepBudget.** Plafond de steps, d'appels d'outils, et de wall-clock pour une tâche. Appliqué par le runtime Rust, non négociable. Vue côté agent via `ctx.budget`.

**Supervisor.** Acteur Tokio qui orchestre le démarrage ordonné des autres acteurs et gère leur RestartPolicy. Cf. [chapitre 30](../part-viii-runtime-rust/30-actors-supervisor.md).

**Template.** Fichier Jinja2 dans `templates/` d'un agent. Rendu côté runtime via `ctx.templates.render(name, **vars)`. Moteur `minijinja` côté Rust, sandboxé.

**Tool descriptor.** Vue exposée au LLM d'une skill ou d'un outil natif : nom, description, `input_schema`, `examples`. C'est ce qui permet au LLM de générer un payload valide.

**Tool natif.** Outil fourni par le runtime Rust : `file_read`, `file_write`, `bash_executor`, `web_read`, `web_search`, `python_executor`, etc. Sandboxés par profil.

**TypedDict.** `typing.TypedDict`, structure de dict typée. Utilisée pour les payloads structurés afin de produire un JSON Schema strict (`properties` + `required`). Cf. [chapitre 21](../part-iv-llm-friendly-design/21-typeddict-schemas.md).

**venv (isolé).** Chaque agent installé a son propre `~/.apollia/sandboxes/<agent>/venv/` avec ses packages PyPI. Les dépendances déclarées dans `@agent(packages=(...))` sont installées au boot.

**Worker (agent).** Agent qui expose une ou plusieurs `@skill` A2A, sans `@on_message`. Pattern 2 des 4 quickstarts.

**Workspace.** Projet de l'utilisateur. Contient optionnellement un `APOLLIA.md` (règles partagées) et des datasources qui peuvent override les datasources locales d'un agent.
