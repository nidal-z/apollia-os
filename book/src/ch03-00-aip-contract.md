# Le contrat AIP

Dans les chapitres précédents, vous avez écrit deux agents — `hello-agent` et `file-assistant` — sans jamais apprendre le nom du protocole qui les fait fonctionner. Il est temps de le nommer.

L'**Agent Interface Protocol** (AIP) est le contrat entre votre code Python et le runtime Apollia OS. Sa philosophie tient en une phrase :

> **Un agent est n'importe quel objet Python qui a `manifest()` et `async run()`.**

Pas de classe de base. Pas de framework à importer. Pas de décorateur. Le runtime utilise le duck typing pour vérifier que votre objet respecte le contrat — et si c'est le cas, il peut l'exécuter.

---

## Les quatre composants de l'AIP

| Composant | Rôle | Où |
|---|---|---|
| **AgentManifest** | Carte d'identité et déclaration de capacités | Retourné par `manifest()` |
| **AIPTask** | Ce que le runtime envoie à l'agent | Paramètre `task` de `run()` |
| **AIPResult** | Ce que l'agent retourne | Valeur de retour de `run()` |
| **RuntimeContext** | Services injectés par le runtime | Paramètre `ctx` de `run()` |

Vous avez déjà utilisé ces quatre composants dans `file-assistant` — sans le savoir. Ce chapitre en donne la spécification complète, avec tous les champs et toutes les options.

---

## Ce que vous allez apprendre

- **Section 1 — `manifest()`** : tous les champs du manifest, ce qui est obligatoire, ce qui est optionnel, et comment ils modifient le comportement du runtime
- **Section 2 — `run()`** : la structure complète de `task` (AIPTask), tous les formats de retour (AIPResult), et comment gérer les cas avancés comme l'approbation humaine
- **Section 3 — Cycle de vie** : les états d'un agent (ProcessState) et d'une tâche (TaskState), et comment les observer depuis la CLI

Chaque exemple de ce chapitre s'appuie sur `file-assistant` du chapitre 2 — vous verrez comment étendre cet agent avec de nouvelles capacités à mesure qu'on introduit de nouveaux champs.
