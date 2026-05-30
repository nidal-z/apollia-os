# Annexe G. FAQ

Questions récurrentes côté auteurs d'agents et opérateurs. Si la vôtre n'y est pas, ouvrez une GitHub discussion sur le repo `apollia-os`.

---

## Côté SDK / écriture d'agents

### Mon agent n'est pas trouvé après `apollia-os agent install`, pourquoi ?

Trois causes courantes :

1. La classe n'est pas décorée par `@agent`. Vérifiez avec `python -m apollia inspect <fichier>`.
2. Le module a une erreur d'import (un package manquant, un import circulaire). `apollia inspect` la montrera.
3. Le manifeste a un champ invalide (par exemple `version` non-semver). `apollia inspect` lèvera `AgentConfigError` avec la cause exacte.

### Faut-il toujours déclarer `tools_required` ?

Non strictement, mais c'est recommandé. Le manifeste sert au fail-fast au boot (un outil manquant fait planter le démarrage avec un message clair) et à la lisibilité (`apollia inspect` montre ce que l'agent peut faire). Le mur de permissions runtime est appliqué par le permission engine, indépendamment du manifeste. Cf. [chapitre 6](../part-ii-the-decorators/06-agent-decorator.md).

### Comment lire un fichier sur le disque ?

Passez par `ctx.tools.call("file_read", input={"path": ...})`. N'utilisez **jamais** `open()` ou `pathlib.Path.read_text()` directement : ces appels contournent la sandbox, le budget, et l'audit trail.

### Comment écrire dans `~/Documents/...` ?

Le sandbox limite par défaut à `~/.apollia/sandboxes/<agent>/`. Pour sortir, configurez `apollia.toml` au niveau `[tools]` pour autoriser un répertoire spécifique. Ou exposez le besoin à l'opérateur qui acceptera via le permission engine en mode HITL.

### Mon agent boucle, comment arrêter ?

Le `StepBudget` du runtime coupera après `max_steps` (défaut 30) ou `wall_clock_secs` (défaut illimité). Vous pouvez forcer plus tôt :

```python
if ctx.budget.steps_remaining <= 0:
    raise DomainError("BUDGET_EXHAUSTED", "Step budget consumed")
```

Pour arrêter une tâche depuis l'extérieur : `apollia-os task cancel <task-id>`.

### Comment partager du code entre deux workers ?

Trois options :

1. **Module Python partagé.** Mettez le code commun dans un fichier `_shared.py` à la racine du dossier de l'agent. Importez avec un import absolu.
2. **Package PyPI.** Si le code est suffisamment générique, packagez-le et déclarez-le dans `@agent(packages=("my-shared-lib>=1.0",))`.
3. **A2A.** Si le code commun est un comportement métier (par exemple "résume un texte en 3 phrases"), exposez-le comme une `@skill` d'un agent dédié et appelez via A2A.

### Comment gérer une réponse LLM mal formée ?

Le boundary du dispatcher trappe automatiquement les `JSONDecodeError` issues du parsing de tool calls et émet `emit_action_parse_error`. Si vous parsez du JSON manuellement, catchez explicitement et levez `DomainError("INVALID_LLM_OUTPUT", ...)` pour ne pas remonter en erreur générique.

### Pourquoi `ctx.a2a.skill_as_tool` est-il async ?

Parce que le bridge interroge le registre A2A en interne (le runtime Rust enveloppe l'appel dans un `future_into_py`). Du point de vue de l'agent Python, la méthode est `async def` : oublier le `await` insère une coroutine dans la liste `tools=[...]` et le LLM router lèvera `tool spec dict missing 'name' key`. Toujours `await ctx.a2a.skill_as_tool(...)`.

### Quand utiliser `@orchestrated` vs `apollia.react` ?

Si la mission tient en quelques phrases d'intention en langue naturelle et que vous voulez minimiser le code, c'est `@orchestrated`. Si vous voulez du contrôle (workflow connu, branches, pré et post-traitement), c'est `apollia.react` dans un `@on_message`. Cf. tableau comparatif au [chapitre 5](../part-i-getting-started/05-quickstart-orchestrated.md).

### Mon TypedDict ne génère pas le bon schéma

Avez-vous `from __future__ import annotations` en haut du fichier `schemas.py` ? Retirez-le. PEP 563 casse `__required_keys__` et le SDK ne peut plus distinguer requis et optionnel. Cf. [chapitre 21](../part-iv-llm-friendly-design/21-typeddict-schemas.md).

### Le LLM ne respecte pas mon `input_schema`

Trois leviers à activer :

1. `Annotated[T, "description claire"]` sur les paramètres ambigus.
2. `@skill(examples=[{...}])` avec un payload réaliste.
3. `TypedDict` pour les structures imbriquées (au lieu de `dict[str, Any]`).

Cf. [Partie IV](../part-iv-llm-friendly-design/19-annotated-descriptions.md). Un agent qui combine les trois passe de ~62 % à ~97 % de payloads valides du premier coup sur Haiku-4.5.

---

## Côté runtime / opérateur

### Apollia tourne sur Windows ?

Pas en v0.1. Le runtime Linux et macOS est officiellement supporté. Windows est sur la roadmap (cf. ADR-052), mais le sandbox via user namespaces n'a pas d'équivalent natif. Solution intermédiaire : WSL2.

### Mes données quittent-elles la machine ?

Par défaut, non. Le runtime, la mémoire, l'audit trail, les secrets sont locaux. **Sauf** si vous configurez un backend LLM cloud (Anthropic, OpenAI, Vertex) : les prompts et les réponses transitent par ce service. C'est explicitement déclaré au niveau de chaque LLM call (`backend=...`). Pour le local-first strict, utilisez le backend `local` (llama.cpp bundled) ou Ollama.

### Comment sauvegarder mes données ?

Tout vit dans `~/.apollia/`. Sauvegardez ce dossier (rsync, time machine, restic, etc.). Les bases SQLite supportent les snapshots à chaud sans corruption.

### Puis-je faire tourner Apollia sur un serveur sans UI ?

Oui. La CLI `apollia-os start` démarre le runtime en arrière-plan. L'API REST sur `localhost:7771` permet de piloter depuis un script ou un autre service. Le Desktop est optionnel.

### Comment savoir ce qu'un agent a fait ?

Trois outils :

- `apollia-os audit list --limit N` : les N derniers appels d'outils, datés et résultatés.
- `apollia-os task inspect <task-id>` : la trajectoire complète d'une tâche orchestrée (plan, étapes, observations).
- `~/.apollia/memory/<agent>.db` : la mémoire propre à l'agent, consultable en SQLite.

### Combien Apollia coûte-t-il ?

Le runtime et le SDK sont open-source MIT, gratuit. Le coût d'opération vient des LLM cloud si vous en utilisez : compte chez Anthropic ou OpenAI. Comptez ~$0.03 par invocation d'un director ReAct moyen, ~$0.001 par appel LLM atomique sur Haiku-4.5. Local llama.cpp : $0 (mais coût matériel d'une machine assez puissante).

### Mon agent `@orchestrated` retourne `[NO_LLM]` à chaque appel

Le moteur ORIA, qui pilote les agents `@orchestrated`, a besoin d'un backend LLM pour la planification. Le code `[NO_LLM]` signifie qu'aucun backend n'est résolu pour le rôle `precise`.

Vérifiez d'abord qu'au moins un backend est actif :

```bash
apollia-os llm status
# Doit afficher au moins un backend en STATUS `ready`.
```

Si la liste est vide ou si aucun backend n'est marqué actif, installez-en un (cf. [chapitre 1, section « Configurer un backend LLM »](../part-i-getting-started/01-installation.md)) puis déclarez-le par défaut :

```bash
apollia-os llm backends set-default <name>
```

En setup mono-backend, ça suffit : le runtime utilise le défaut pour le rôle `precise`. Pour un split précis/rapide multi-backend, ajoutez aussi une section `[llm.routing]` dans `~/.config/apollia/apollia.toml` :

```toml
[llm.routing]
precise = "anthropic"   # backend pour la planification ORIA
fast    = "local"       # backend pour les appels rapides
```

Puis redémarrez le runtime (`apollia-os stop && apollia-os start`). Les agents `@skill` et `@on_message` continuent de fonctionner avec n'importe quel backend par défaut ; seul `@orchestrated` consomme le rôle `precise`. Détail des trois rôles au [chapitre 31](../part-viii-runtime-rust/31-rest-api-config.md).

### Comment déboguer un agent qui ne répond pas ?

```bash
apollia-os agent logs <agent-name> --last 100
```

Affiche les logs `ctx.logger.*` du worker, plus les erreurs runtime. Combinez avec `apollia-os status` pour voir l'état (`ACTIVE`, `DEGRADED`, `STOPPED`) et `apollia-os task list --pending-approval` pour voir les tâches en attente HITL.

### Mon agent reste en `STOPPING` éternellement

Probablement une tâche qui ne se termine pas et qui dépasse le drain de 30s. Forcez :

```bash
apollia-os stop --force
```

Puis investiguez avec `apollia-os task inspect <last-task-id>` pour comprendre où elle s'est bloquée. Causes courantes : appel HTTP sans timeout, boucle infinie côté agent, deadlock avec un autre agent (A2A cyclique).

### Comment intégrer Apollia dans une stack CI ?

Trois patterns :

1. **`apollia inspect` en pre-commit** ou en CI pour valider les manifestes (cf. [chapitre 27](../part-vii-tooling/27-apollia-inspect.md)).
2. **`pytest` sur les agents** avec `apollia.testing.mock` (cf. [chapitre 24](../part-vi-testing/24-testing-isomorphic-mock.md)).
3. **Eval suite nightly** qui interroge un LLM réel et vérifie la qualité (cf. [chapitre 26](../part-vi-testing/26-eval-suites.md)).

---

## Migration depuis v0.4

### J'ai un agent v0.4 qui hérite de `BaseReActAgent`, comment le migrer ?

La v0.4 n'a pas été publiée publiquement. Si vous avez un agent qui hérite encore d'une classe parente legacy, c'est qu'il vient d'une branche interne. La migration consiste à :

1. Remplacer la classe parente par une classe simple décorée `@agent(...)`.
2. Remplacer `def manifest(self)` par les arguments du décorateur `@agent`.
3. Remplacer `async def run(self, task, ctx)` par une ou plusieurs `@skill` ou un `@on_message`.
4. Remplacer `AIPResult.completed(data).to_dict()` par `return {...}`.
5. Remplacer `AIPResult.failed(code, msg).to_dict()` par `raise DomainError(code, msg)`.

Comptez 30 minutes par agent. Validez via `apollia inspect`.

---

## Performance

### Quelle latence pour une invocation typique ?

- **Soumission via PyO3** (Rust ↔ Python en mémoire) : ~3 ms.
- **`ctx.tools.call("file_read", ...)`** sandboxé : ~10 à 30 ms selon la taille du fichier.
- **`ctx.llm.complete(...)` Haiku-4.5 cloud** : ~500 ms à 2 s selon la longueur.
- **`apollia.react` 5 tours** (4 tool calls + 1 réponse finale) : ~5 à 10 s.

Le runtime n'est pas le bottleneck. C'est le LLM.

### Combien d'agents en parallèle ?

Le runtime supporte plusieurs centaines d'agents actifs simultanément (limité par la RAM des venvs Python isolés). En pratique, sur un poste de travail moderne, 20 à 50 agents installés sans souci.

### Combien de tâches concurrentes par agent ?

Par défaut 1 (séquentiel). Configurable via `@agent(step_budget={"max_concurrent_tasks": ...})` jusqu'à la limite système. Au-delà de 5 ou 10 par agent, vous saturerez probablement le LLM avant le runtime.

---

## Communauté

### Où poser une question ?

- **GitHub Discussions** sur le repo `apollia-os` : questions ouvertes, idées.
- **GitHub Issues** : bugs et feature requests.
- **Discord** : à venir post-v0.1.0.

### Comment contribuer ?

Trois axes :

1. **Code.** Pull requests bienvenues sur le runtime Rust, le SDK Python, les agents bundled, les templates.
2. **Documentation.** Le book vit dans `book/src/`, le wiki dans `docs/wiki/`. Toute correction de typo ou amélioration de clarté est appréciée.
3. **Agents partagés.** Si vous écrivez un agent intéressant (open-source MIT), proposez-le pour le futur marketplace.

### Apollia est-il vraiment open-source ?

Oui, licence MIT. Le runtime Rust, le SDK Python, le Desktop Tauri, et les agents bundled sont sous MIT. Pas de version "Enterprise" payante prévue. Le modèle de monétisation est le service (prestation, support), pas la licence.
