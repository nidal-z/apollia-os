---
sidebar_position: 4.5
title: Manifeste de paquet (apollia.toml)
description: "Chaque table et chaque clé du fichier apollia.toml placé à la racine d'un paquet, les règles qu'un manifeste doit tenir, et un exemple complet."
---

# Manifeste de paquet (apollia.toml)

Un paquet est un dossier qui porte un `apollia.toml` à sa racine. Le manifeste
déclare ce que le paquet livre : des agents, qui sont des données, des outils
Python, des workflows Python, les connexions dont il a besoin, les serveurs MCP
qu'il apporte, ses dépendances Python et les déclencheurs qu'il propose.
`apollia-os package build` fait du dossier une archive `.apo`, et lire le
manifeste n'exécute jamais le code du paquet.

Ce fichier porte le même nom que le fichier de configuration du runtime, décrit
dans [Configuration (apollia.toml)](/reference/configuration). Ce sont deux
fichiers différents : celui-ci se trouve à la racine d'un dossier de paquet et
voyage dans l'archive.

## Schéma strict

Le schéma est strict. Une clé que le schéma ne connaît pas est refusée, et le
refus nomme le chemin complet de la clé :

```text
apollia.toml: unknown key `agents[0].defaults.model.temprature`
```

Rien n'est ignoré : ce que vous écrivez est exactement ce qu'Apollia lit.

## Tables

| Table | Cardinalité | Déclare |
| --- | --- | --- |
| `[package]` | une, obligatoire | L'identité du paquet. |
| `[[agents]]` | zéro ou plus | Des agents : une mission, des exemples et des valeurs par défaut. Aucun code. |
| `[[tools]]` | zéro ou plus | Des outils Python, chacun joignable sous `pkg:<paquet>/<nom>`. |
| `[[workflows]]` | zéro ou plus | Des workflows Python avec leurs étapes déclarées. |
| `[[requires]]` | zéro ou plus | Les connexions dont le paquet a besoin. |
| `[[mcp_servers]]` | zéro ou plus | Les serveurs MCP que le paquet apporte. |
| `[pip]` | zéro ou une | Les dépendances Python de l'environnement du paquet. |
| `[[triggers]]` | zéro ou plus | Les déclencheurs que le paquet propose. |

Un paquet déclare au moins un agent, un outil ou un workflow.

## `[package]`

| Clé | Type | Obligatoire | Sens |
| --- | --- | --- | --- |
| `name` | chaîne | oui | Identifiant du paquet, `[a-z0-9_-]+`. Préfixe de ses outils : `pkg:<name>/<outil>`. |
| `version` | chaîne | oui | Version semver du paquet, par exemple `1.2.0`. |
| `publisher` | chaîne | oui | Éditeur affiché à l'installation du paquet. |
| `description` | chaîne | non | Ce que le paquet livre. |
| `sdk` | chaîne | oui | Contrainte semver sur le SDK pour lequel le paquet a été écrit, par exemple `">=0.3, <0.4"`. |

## `[[agents]]`

Un agent est une donnée. Le moteur d'agent intégré l'exécute ; le paquet ne
livre aucun code pour lui.

| Clé | Type | Obligatoire | Sens |
| --- | --- | --- | --- |
| `id` | chaîne | oui | Identifiant, `[a-z0-9_-]+`, unique dans le paquet. |
| `name` | chaîne | oui | Nom affiché. |
| `mission` | chaîne | oui | Ce à quoi sert l'agent, dans les mots de son auteur. Sert d'instructions. |
| `examples` | liste de chaînes | non | Exemples de demandes affichés sur la fiche de l'agent. |

### `[agents.defaults]`

Chaque valeur est ici une valeur par défaut que le paquet recommande, jamais une
contrainte : une surcharge posée par l'utilisateur sur sa machine l'emporte, et
une mise à jour du paquet la conserve.

| Clé | Type | Sens |
| --- | --- | --- |
| `access` | liste de chaînes | Outils accordés par défaut : le nom d'un outil natif, `mcp:<serveur>/<outil>`, ou `pkg:<paquet>/<outil>`. Une entrée `pkg:` doit nommer un outil de ce paquet. |
| `sources` | liste de chaînes | Sources cochées par défaut. `"profile"` laisse l'agent lire le profil de l'utilisateur. |
| `autonomy` | chaîne | Palier d'autonomie : `assisted`, `supervised`, `bounded_autonomous` ou `long_autonomous`. Sans elle, le défaut global s'applique. |
| `planning` | chaîne | Si l'agent planifie une tâche avant d'agir : `never`, `auto` ou `always`. Sans elle, `auto`. |

### `[agents.defaults.model]`

| Clé | Type | Sens |
| --- | --- | --- |
| `model` | chaîne | Modèle que le paquet recommande. |
| `preset` | chaîne | Préréglage de réglages de requête que le paquet recommande. |

`[agents.defaults.model.request]` porte les réglages de requête eux-mêmes :
`temperature`, `top_p`, `top_k`, `repetition_penalty`, `max_tokens` et `seed`.

## `[[tools]]`

Une entrée par outil Python. `python -m apollia describe <dossier>` écrit ces
entrées depuis les fonctions décorées par `@tool`, si bien que le manifeste et
le code ne peuvent pas se contredire.

| Clé | Type | Obligatoire | Sens |
| --- | --- | --- | --- |
| `name` | chaîne | oui | Identifiant, `[a-z0-9_-]+`, unique dans le paquet. |
| `module` | chaîne | oui | Module Python, relatif à la racine du paquet, par exemple `tools/invoices.py`. |
| `description` | chaîne | oui | Ce que fait l'outil, pour le modèle et pour la fiche de l'agent. |
| `action` | chaîne | oui | Famille d'action, voir ci-dessous. |
| `inputs` | table | non | Schéma JSON des entrées. |

### Familles d'action {#action-families}

Un outil et un workflow déclarent chacun une famille d'action : ce que l'appeler
fait au monde. Toute famille sauf `read` rend l'accès sensible.

| Famille | Sens | Sensible |
| --- | --- | --- |
| `read` | Lit des données qui ne sont pas personnelles. | non |
| `read_personal` | Lit des données personnelles : courrier, agenda, documents d'un utilisateur. | oui |
| `write_or_delete` | Écrit ou supprime. | oui |
| `send_external` | Envoie quelque chose hors de la machine. | oui |
| `execute_code` | Exécute du code. | oui |

## `[[workflows]]`

Une entrée par workflow Python, écrite par `python -m apollia describe <dossier>`
depuis les fonctions décorées par `@workflow`.

| Clé | Type | Obligatoire | Sens |
| --- | --- | --- | --- |
| `name` | chaîne | oui | Identifiant, `[a-z0-9_-]+`, unique dans le paquet. Le workflow s'adresse par `<paquet>/<name>`. |
| `module` | chaîne | oui | Module Python, relatif à la racine du paquet. |
| `description` | chaîne | oui | Ce que fait le workflow. |
| `action` | chaîne | oui | Famille d'action : ce que le workflow fait de plus sensible. |
| `inputs` | table | non | Schéma JSON des entrées. Sa clé `x-order` les liste dans l'ordre que suit le formulaire de lancement. |
| `steps` | liste de chaînes | non | Noms des étapes, dans l'ordre. |

## `[[requires]]`

| Clé | Type | Obligatoire | Sens |
| --- | --- | --- | --- |
| `type` | chaîne | oui | Type de connexion, tel que le catalogue des connexions le nomme, par exemple `gmail`. |
| `reason` | chaîne | non | Pourquoi le paquet en a besoin, affiché quand la connexion manque. |

## `[[mcp_servers]]`

Les clés, leurs défauts et leur sens sont ceux d'un serveur de `mcp.toml`.

| Clé | Type | Défaut | Sens |
| --- | --- | --- | --- |
| `name` | chaîne | obligatoire | Nom du serveur, `[a-z0-9_-]+`. Préfixe de ses outils : `mcp:<name>/<outil>`. |
| `command` | chaîne | vide | Exécutable à lancer ; vide pour un transport réseau. |
| `args` | liste de chaînes | vide | Arguments passés à la commande. |
| `env` | table | vide | Environnement du processus du serveur ; les emplacements `${VAR}` sont admis. |
| `transport` | chaîne | `stdio` | `stdio`, `streamable-http` ou `sse`. |
| `url` | chaîne | aucune | URL du serveur pour un transport réseau. |
| `init_timeout_secs` | entier | `30` | Délai de la poignée de main `initialize`. |
| `call_timeout_secs` | entier | `60` | Délai d'un `tools/call`. |
| `max_response_bytes` | entier | `8388608` | Plafond d'une réponse du serveur. |
| `max_tools` | entier | `256` | Plafond des outils retenus de `tools/list`. |
| `tags` | liste de chaînes | vide | Étiquettes appliquées à chaque outil du serveur. |

## `[pip]`

| Clé | Type | Sens |
| --- | --- | --- |
| `packages` | liste de chaînes | Dépendances installées dans l'environnement du paquet. Épinglez-les (`pypdf==4.3.1`). |

## `[[triggers]]` {#triggers}

Un paquet propose des déclencheurs ; l'installer ne les arme pas de lui-même.

| Clé | Type | Obligatoire | Sens |
| --- | --- | --- | --- |
| `id` | chaîne | oui | Identifiant, `[a-z0-9_-]+`, unique parmi les déclencheurs. |
| `target` | chaîne | oui | L'agent ou le workflow de ce paquet que le déclencheur lance. |
| `input` | chaîne | non | Message envoyé à un agent à chaque déclenchement. |
| `source` | table | oui | Quand il se déclenche, voir ci-dessous. |

`[triggers.source]` est étiquetée par `type` :

| `type` | Clés | Se déclenche |
| --- | --- | --- |
| `cron` | `schedule` | Sur une expression cron, par exemple `"0 8 * * MON"`. |
| `interval` | `every` | À intervalle fixe : `"30m"`, `"1h"`, `"1d"`. |
| `file_watch` | `recursive` (défaut `false`) | Quand un dossier change. L'utilisateur choisit le dossier à l'installation. |
| `webhook` | aucune | Sur un appel HTTP. Le secret se pose à l'installation, jamais dans le manifeste. |

## Validation

Au-delà du schéma, un manifeste tient les règles suivantes. La première qui est
enfreinte est signalée, et le paquet n'est ni construit ni installé.

| Règle | Message |
| --- | --- |
| Le paquet déclare au moins un agent, un workflow ou un outil. | `the package declares no agent, no workflow and no tool` |
| Le nom du paquet, chaque identifiant et chaque nom de serveur MCP respectent `[a-z0-9_-]+`. | `` `<field>` value `<value>` must match [a-z0-9_-]+ ``, ou `` MCP server name `<name>` must match [a-z0-9_-]+ `` pour un serveur MCP |
| `version` est semver. | `` package version `<value>` is not semver `` |
| `sdk` est une contrainte semver. | `` sdk requirement `<value>` is not a semver requirement `` |
| Agents, outils et workflows partagent un seul espace de noms, sans doublon. Les identifiants de déclencheur sont uniques, les noms de serveur MCP aussi. | `` identifier `<id>` is declared more than once `` |
| Un accès par défaut `pkg:<paquet>/<outil>` nomme un outil de ce paquet. | `` agent `<agent>` grants `<access>`, which is not a tool of package `<package>` `` |
| Un déclencheur vise un agent ou un workflow de ce paquet. | `` trigger `<trigger>` targets `<target>`, which is no agent or workflow of the package `` |

`apollia-os package build` ajoute un contrôle qui lui est propre : il refuse un
dossier dont les modules Python définissent des outils ou des workflows que le
manifeste ne décrit pas.

## Exemple complet

```toml
[package]
name = "invoices"
version = "1.2.0"
publisher = "Example Accounting"
description = "Reads supplier invoices and drafts the reminders."
sdk = ">=0.3, <0.4"

[[agents]]
id = "invoice-clerk"
name = "Invoice clerk"
mission = "Read the supplier invoices of the month and list the late ones."
examples = ["Which invoices are late?", "Draft a reminder for ACME."]

[agents.defaults]
access = ["file_read", "mcp:ledger/lookup", "pkg:invoices/extract_totals"]
sources = ["profile"]
autonomy = "supervised"
planning = "always"

[agents.defaults.model]
model = "qwen3-8b-instruct"
preset = "balanced"

[agents.defaults.model.request]
temperature = 0.2
max_tokens = 2048

[[tools]]
name = "extract_totals"
module = "tools/extract.py"
description = "Extract the total, the due date and the supplier of an invoice."
action = "read_personal"

[tools.inputs]
type = "object"
required = ["path"]

[tools.inputs.properties.path]
type = "string"
description = "Path of the invoice file."

[[workflows]]
name = "monthly-reminders"
module = "workflows/reminders.py"
description = "Find the late invoices and send one reminder per supplier."
action = "send_external"
steps = ["collect", "confirm", "send"]

[workflows.inputs]
type = "object"

[workflows.inputs.properties.month]
type = "string"

[[requires]]
type = "gmail"
reason = "Reminders are sent from the user's mailbox."

[[mcp_servers]]
name = "ledger"
command = "uvx"
args = ["ledger-mcp"]

[pip]
packages = ["pypdf==4.3.1"]

[[triggers]]
id = "first-monday"
target = "monthly-reminders"

[triggers.source]
type = "cron"
schedule = "0 8 1-7 * MON"
```

## Voir aussi

- [Empaqueter et distribuer](/how-to/package-and-distribute-an-agent) pour la
  séquence qui mène d'un dossier vide à un paquet installé.
- [Écrire un outil](/how-to/write-a-worker) et
  [Écrire un workflow](/how-to/write-a-director) pour le Python que le manifeste
  décrit.
- Les commandes `package` dans la [référence CLI](/reference/cli).
