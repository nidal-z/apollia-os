---
sidebar_position: 4
title: Configuration (apollia.toml)
---

# Configuration (apollia.toml)

Référence de la surface de configuration `apollia.toml`.

Le fichier a un seul emplacement, `~/.apollia/apollia.toml`, et toutes les
surfaces lisent et écrivent celui-là : le démon lancé par `apollia-os start`,
l'application desktop, et chaque commande qui touche la configuration
(`apollia-os config`, `apollia-os tools config`, `apollia-os doctor`). Une clé réglée par l'une est la clé que lisent les autres.

Seul un chemin donné explicitement le remplace :

| Commande | Option |
| --- | --- |
| `apollia-os start` | `--config <CHEMIN>`. Le fichier doit exister, sinon le démarrage échoue |
| `apollia-os config get`, `set`, `show`, `validate`, `edit` | `--file <CHEMIN>` |

Le dossier courant n'est pas consulté : un paquet d'agent range son manifeste
sous le même nom, `apollia.toml`, et lancer le démon dans le dossier d'un paquet
ne le lit pas. Un fichier laissé à un ancien emplacement (`./apollia.toml`,
`$XDG_CONFIG_HOME/apollia/apollia.toml`, `~/.config/apollia/apollia.toml`) n'est
jamais lu ni importé. Le démon émet pour lui un avertissement
`config.legacy_location.ignored` au démarrage, et `apollia-os doctor` le nomme
avec la commande qui le déplace vers `~/.apollia/apollia.toml`.

Chaque section est optionnelle : une section absente retombe sur ses valeurs par
défaut. Les données d'exécution (bases de données, jeton API, modèles) vivent
séparément sous `~/.apollia/`.

## Le fichier par défaut

Quand `~/.apollia/apollia.toml` est absent, la première surface qui démarre
l'écrit : l'application de bureau, le démon lancé par `apollia-os start` (sans
`--config`), ou toute commande `apollia-os config` autre que `reset`. Un fichier
qui existe n'est jamais réécrit.

Le fichier par défaut liste chaque clé des tableaux ci-dessous, section par
section, chacune sous une ligne qui dit ce qu'elle fait, en ligne commentée qui
porte sa valeur par défaut :

```toml
[api]

# Address the daemon's TCP listener binds to.
# bind = "127.0.0.1"
```

Une ligne commentée vaut sa valeur par défaut, et suit les valeurs par défaut
des versions suivantes. Pour changer un réglage, décommentez sa ligne et
modifiez la valeur ; pour revenir à la valeur par défaut, commentez-la de
nouveau. Chaque ligne peut être décommentée seule sans que le fichier cesse
d'être valide. Une clé sans valeur par défaut est écrite en commentaire qui le
dit. `[llm]` est écrite en une phrase, sans clé : les modèles de langage se
gèrent dans les réglages IA de l'application, qui les gardent dans la base du
moteur. Le fichier est écrit en anglais, comme ses clés.

`apollia-os config keys` affiche la même liste, et `--json` donne pour chaque clé
son chemin, son type, sa valeur par défaut, sa documentation, sa classe de
rechargement, si la politique de l'organisation peut l'imposer, et si elle est
retirée.

## Changements pendant que le moteur tourne

Le moteur suit son fichier. Un texte enregistré par `PUT /api/v1/config`, ou le
fichier modifié sur le disque par tout autre moyen, est relu une fois qu'il est
resté inchangé un instant. Un texte valide est appliqué : une clé de classe de
rechargement `Reloadable` prend effet sans redémarrage, au prochain usage du
réglage (la prochaine conversation, le prochain échange, la prochaine exécution
d'un agent, le prochain passage sur les tâches en attente). Une clé de classe
`Restart` garde son ancienne valeur jusqu'au prochain démarrage du moteur ;
`GET /api/v1/config` la liste sous `pendingRestart`, avec sa ligne, et la liste
est vide après un démarrage. Un texte invalide ne change rien : la dernière
configuration valide reste en vigueur, la route de lecture rend les fautes à
leur ligne, et un événement `ConfigInvalid` est publié. Chaque application publie
un événement `ConfigReloaded` avec les sections appliquées et les clés en attente
de redémarrage.

## Sections

Le fichier est lu strictement, par la même validation partout. Une clé
qu'aucune section ne déclare, ou une valeur du mauvais type, est une faute
nommée à sa ligne : `apollia-os config validate` liste chaque faute,
`apollia-os config set` refuse de l'écrire, et `apollia-os start` refuse de
démarrer, avec le code de sortie 1. L'application de bureau démarre alors sur
les valeurs par défaut, journalise chaque faute et inscrit le refus au journal
d'audit, au lieu d'écarter le fichier en silence. Une section retirée (voir plus bas) reste acceptée, avec un
avertissement `config.key.retired`.

| Section | Objet |
|---|---|
| `[llm]` | Configuration du backend LLM. |
| `[api]` | Écouteur TCP et authentification (`bind`, `require_token`, `tls_cert`, `tls_key`). |
| `[runtime]` | Capacités de l'EventBus et des mailbox, et le temps accordé au démarrage du runtime. |
| `[hitl]` | Délai d'expiration et intervalle de scan du human-in-the-loop. |
| `[tools]` | Outils natifs : limites, désactivation statique, et configuration par outil `[tools.web_search]` / `[tools.web_read]`. |
| `[mcp]` | Configuration du module MCP, y compris `[[mcp.servers]]` (voir plus bas). |
| `[hooks]` | Gestionnaires de hooks de cycle de vie (commande ou HTTP). |
| `[chat]` | Valeurs par défaut au niveau session du sous-système de chat (par exemple `plan_mode_default`). |
| `[workflows]` | Le budget avec lequel démarre l'exécution d'un workflow, partagé par chaque agent et chaque workflow qu'elle lance. |
| `[models]` | L'index signé des fiches de modèles, le dossier des modèles (`dir`), et la règle appliquée à un fichier importé (`[models.import]`). |
| `[observability]` | Capture et rétention des traces, documentées plus bas. |
| `[desktop]` | Réglages de l'application de bureau : `developer_mode`, `update_channel` (`stable` ou `preview`), `background_service_allowed`, `chat_auto_naming` (`never`, `auto` ou `always`), et `link_preview_enabled`, qui récupère l'aperçu d'un lien affiché dans une conversation. Désactivé par défaut, puisque la récupération sort de la machine. |
| `[server]` | Le serveur compatible : `enabled`, `port`, `require_auth`. |
| `[engines]` | Réglages des moteurs d'inférence locaux (`log_level`). |
| `[author]` | Le nom d'éditeur des agents exportés depuis cette machine (`name`). |

Les tableaux ci-dessous suivent le registre des clés du moteur, la liste dont
sont faits le fichier par défaut et `apollia-os config keys`, si bien qu'aucune
clé ne peut s'en écarter. Ils couvrent chaque clé, tables imbriquées comprises.
Une entrée d'une liste de tables, comme `[[llm.backends]]` ou `[[mcp.servers]]`,
a ses propres champs : celle de MCP est documentée intégralement plus bas, les
autres se lisent depuis les types qu'elles nomment.

### Des lignes que les tableaux ne peuvent pas nuancer

Les tableaux sont dérivés du registre des clés, et une entrée dit ce qu'une clé
signifie, pas si quelque chose la lit. Quatre points demandent une réserve que la
dérivation ne peut pas porter.

**`[api]` est partagée entre deux chargeurs, et aucun ne la lit en entier.** Le
démon lancé par `apollia-os start` et le runtime embarqué dans l'application de
bureau construisent leur écouteur à partir de champs différents de la même
section. Seule `port` est lue par les deux. Toute clé de la section prend effet
au démarrage suivant.

| Clé | `apollia-os start` | L'application de bureau |
| --- | --- | --- |
| `bind` | lue | ignorée, l'écouteur embarqué est toujours sur `127.0.0.1` |
| `port` | lue, après l'option `--port` | lue, voir plus bas |
| `require_token` | lue | ignorée, l'écouteur TCP embarqué exige toujours le jeton |
| `unix_socket` | ignorée, le démon prend sa socket de l'option `--socket` d'`apollia-os start` et retombe sur la même valeur par défaut | lue |
| `tls_cert` | lue | ignorée, l'écouteur embarqué ne termine jamais TLS |
| `tls_key` | lue | ignorée, l'écouteur embarqué ne termine jamais TLS |

Une clé qu'un chargeur ignore est tout de même analysée et validée, puis
abandonnée : la poser est silencieux, ce n'est pas une erreur. Donc
`require_token = false` ne désarme pas l'application de bureau, et `tls_cert` y
est inerte.

`[api] port` règle le port de l'API native, 7771 par défaut. Le démon prend
d'abord l'option `--port` d'`apollia-os start`, puis cette clé, puis 7771. Une
fois à l'écoute, le démon inscrit son port dans le dossier de données ;
l'application de bureau se rattache à ce port quand un démon tourne, et écoute
sinon sur cette clé. Le port doit différer de `[server] port`, celui du serveur
compatible : un fichier qui leur donne le même port est refusé, à la ligne de
`[api] port`.

L'inférence texte locale passe par le llama-server embarqué, qui se règle
depuis les [variables d'environnement](/reference/environment-variables), pas
depuis ce fichier. Le moteur de transcription vocale est choisi d'après le
matériel détecté, sans clé pour le forcer.

<!-- BEGIN GENERATED: config-fields -->

### `[llm]`

Backends LLM, routage, et le runner de transcription. Le fichier par défaut laisse cette section en commentaire : les backends se gèrent dans les réglages IA de l'application, qui les gardent dans la base du moteur.

| Clé | Type | Défaut | Rôle |
| --- | --- | --- | --- |
| `llm.format_version` | entier | `1` | Version du format de [llm] ; lue comme 1 en son absence. |
| `llm.default` | chaîne | non définie | Nom du backend utilisé quand aucun n'est demandé ; doit nommer une entrée de llm.backends. |
| `llm.backends` | liste de tables | non définie | Backends compatibles OpenAI, migrés dans la base du moteur au premier démarrage ; exigés avec llm.default. |
| `llm.observability.debug_log_prompt` | booléen | `false` | Journaliser le prompt complet au niveau trace. |
| `llm.routing` | table | non définie | Backends qui répondent aux requêtes précises et rapides, et le plafond du routage hybride. |
| `llm.guardrails.level` | l'une de disabled, lenient, balanced, strict, ceiling | `"balanced"` | Sévérité avec laquelle le lancement d'un modèle local est tenu à la mémoire de la machine ; un niveau inconnu vaut balanced. La politique de l'organisation peut l'imposer. |
| `llm.guardrails.ceiling_gb` | décimal | non définie | Plus grande estimation, en gigaoctets, que le niveau ceiling lance. La politique de l'organisation peut l'imposer. |
| `llm.residency.idle_unload_minutes` | entier | `60` | Minutes sans requête au bout desquelles un modèle local que rien ne retient est déchargé ; 0 ne décharge jamais. La politique de l'organisation peut l'imposer. |

### `[api]`

Écouteur TCP, authentification, TLS, socket Unix.

| Clé | Type | Défaut | Rôle |
| --- | --- | --- | --- |
| `api.bind` | chaîne | `"127.0.0.1"` | Adresse sur laquelle se lie l'écouteur TCP du démon. |
| `api.port` | entier | `7771` | Port TCP de l'API native, lu au démarrage suivant ; l'option --port d'apollia-os start l'emporte sur lui, et il doit différer de server.port. |
| `api.require_token` | booléen | `true` | Exiger un jeton bearer sur chaque requête TCP au démon. |
| `api.unix_socket` | chemin | `"~/.apollia/runtime.sock"` | Socket locale du runtime embarqué ; son dossier doit exister. |
| `api.tls_cert` | chemin | non définie | Chaîne de certificats PEM pour TLS sur l'écouteur TCP du démon ; à poser avec api.tls_key. |
| `api.tls_key` | chemin | non définie | Clé privée PEM associée à api.tls_cert. |

### `[runtime]`

Capacités de l'EventBus et des mailbox.

| Clé | Type | Défaut | Rôle |
| --- | --- | --- | --- |
| `runtime.eventbus_capacity` | entier | `1024` | Événements mis en tampon par l'EventBus avant qu'un lecteur lent décroche (64 à 65536). |
| `runtime.mailbox_capacity` | entier | `100` | Messages mis en tampon par la mailbox d'un acteur (10 à 10000). |
| `runtime.mailbox_visibility_timeout_secs` | entier | `60` | Secondes pendant lesquelles un message d'agent loué reste invisible avant d'être redistribué. |
| `runtime.mailbox_message_ttl_secs` | entier | `86400` | Secondes pendant lesquelles un message d'agent non distribué est gardé. |
| `runtime.mailbox_send_quota_per_run` | entier | `50` | Messages d'agent qu'une exécution peut envoyer. |
| `runtime.mailbox_max_payload_bytes` | entier | `65536` | Plus grande charge utile d'un message d'agent, en octets. |
| `runtime.mailbox_audit_full_payload` | booléen | `false` | Copier la charge utile complète des messages d'agent dans le journal d'audit. |
| `runtime.startup_timeout_secs` | entier | `300` | Secondes que le moteur peut prendre pour démarrer. |
| `runtime.agent_idle_unload_secs` | entier | `600` | Secondes sans tâche après lesquelles le module d'un agent est déchargé. |

### `[hitl]`

Délai d'expiration et intervalle de scan du human-in-the-loop.

| Clé | Type | Défaut | Rôle |
| --- | --- | --- | --- |
| `hitl.timeout_hours` | entier | non définie | Heures après lesquelles une approbation sans réponse annule sa tâche (1 à 168) ; sans valeur, l'attente est infinie. |
| `hitl.scan_interval_secs` | entier | `60` | Secondes entre deux recherches d'approbations expirées (10 à 3600). |

### `[tools]`

Outils natifs : limites, désactivation statique, et réglages par outil.

| Clé | Type | Défaut | Rôle |
| --- | --- | --- | --- |
| `tools.max_output_chars` | entier | `30000` | Plus grande sortie d'outil transmise au modèle, en octets (10 à 1000000). |
| `tools.disabled` | liste de chaînes | `[]` | Outils natifs retirés de tous les agents. |
| `tools.web_search.backend` | l'une de auto, duck_duck_go, brave | `"auto"` | Moteur de recherche de l'outil web_search. |
| `tools.web_search.require_configured` | booléen | `false` | Refuser une recherche quand le moteur choisi n'est pas configuré. |
| `tools.web_search.brave.api_key_env_var` | chaîne | `"BRAVE_SEARCH_API_KEY"` | Variable d'environnement qui porte la clé Brave Search. |
| `tools.web_search.brave.timeout_secs` | entier | `15` | Secondes qu'une requête Brave Search peut prendre (1 à 120). |
| `tools.web_search.brave.max_results` | entier | `10` | Résultats demandés à Brave Search (1 à 20). |
| `tools.web_search.duckduckgo.timeout_secs` | entier | `15` | Secondes qu'une requête DuckDuckGo peut prendre (1 à 120). |
| `tools.web_search.duckduckgo.max_response_kb` | entier | `1024` | Plus grande réponse DuckDuckGo lue, en Kio (16 à 16384). |
| `tools.web_read.timeout_secs` | entier | `20` | Secondes qu'une requête web_read peut prendre (1 à 120). |
| `tools.web_read.max_response_kb` | entier | `2048` | Plus grande page que lit web_read, en Kio (64 à 32768). |
| `tools.web_read.ssrf_guard` | booléen | `true` | Refuser une adresse du réseau local ou de la machine elle-même. |
| `tools.python_interpreter` | chemin | non définie | Interpréteur Python utilisé à la place de celui embarqué. |

### `[mcp]`

Client MCP : approbations, chargement des outils, et le pont qui sert les agents et workflows exposés.

| Clé | Type | Défaut | Rôle |
| --- | --- | --- | --- |
| `mcp.approval_ttl_hours` | entier | `24` | Heures pendant lesquelles l'approbation d'un outil MCP reste valide ; 0 n'expire jamais (0 à 8760). La politique de l'organisation peut l'imposer. |
| `mcp.tool_loading` | l'une de eager, deferred | `"deferred"` | Charger tous les schémas d'outils MCP d'emblée, ou au premier usage. La politique de l'organisation peut l'imposer. |
| `mcp.tool_search_limit` | entier | `20` | Résultats rendus par l'outil tool_search (1 à 500). La politique de l'organisation peut l'imposer. |
| `mcp.server_enabled` | booléen | `false` | Laisser un client MCP atteindre les agents et workflows marqués exposés, par apollia-os mcp server. La politique de l'organisation peut l'imposer. |

### `[hooks]`

Gestionnaires de hooks de cycle de vie. `PreToolUse` est appliqué au mieux : un gestionnaire qui expire, ne répond pas ou répond quelque chose d'illisible retombe sur `allow`, et l'appel d'outil continue.

| Clé | Type | Défaut | Rôle |
| --- | --- | --- | --- |
| `hooks.handlers` | liste de tables | `[]` | Gestionnaires de hooks de cycle de vie, une commande ou un point d'accès HTTP chacun. |

### `[chat]`

Valeurs par défaut des sessions de chat.

| Clé | Type | Défaut | Rôle |
| --- | --- | --- | --- |
| `chat.plan_mode_default` | booléen | `false` | Démarrer chaque nouvelle conversation en mode plan. |
| `chat.tool_turn_temperature` | décimal | non définie | Température du modèle pour un tour qui propose des outils ; sans valeur, 0.3. |

### `[models]`

L'index signé des fiches de modèles, son miroir, le dossier des modèles, et la règle appliquée à un fichier de modèle importé.

| Clé | Type | Défaut | Rôle |
| --- | --- | --- | --- |
| `models.registry_url` | chaîne | `"https://registry.apollia.fr/models/stable/index.json"` | Adresse de l'index signé des fiches de modèles. La politique de l'organisation peut l'imposer. |
| `models.registry_mirror_url` | chaîne | `"https://github.com/Apollia-OS/apollia-os/releases/latest/download/models-stable-index.json"` | Adresse essayée quand l'index ne répond pas ; vide, elle est désactivée. La politique de l'organisation peut l'imposer. |
| `models.registry_enabled` | booléen | `true` | Lire l'index en ligne des fiches de modèles. La politique de l'organisation peut l'imposer. |
| `models.dir` | chemin | `""` | Dossier où sont gardés les modèles de langage, leurs projecteurs et les modèles d'embeddings ; vide, il vaut &lt;dossier de données&gt;/models. Le changer ne déplace aucun fichier. La politique de l'organisation peut l'imposer. |
| `models.import.require_verified` | booléen | `false` | Refuser un fichier de modèle importé qui ne correspond à aucune fiche signée. La politique de l'organisation peut l'imposer. |

### `[workflows]`

Le budget avec lequel démarre l'exécution d'un workflow. Il est partagé : chaque agent qu'une étape lance et chaque workflow qu'une étape appelle y puisent.

| Clé | Type | Défaut | Rôle |
| --- | --- | --- | --- |
| `workflows.max_steps` | entier | `200` | Étapes qu'une exécution de workflow peut faire. |
| `workflows.max_tool_calls` | entier | `400` | Appels d'outils qu'une exécution de workflow peut faire. |
| `workflows.wall_clock_secs` | entier | `3600` | Secondes que peut durer une exécution de workflow. |

### `[desktop]`

Réglages de l'application de bureau que le moteur lit.

| Clé | Type | Défaut | Rôle |
| --- | --- | --- | --- |
| `desktop.developer_mode` | booléen | `false` | Afficher les surfaces avancées de l'application. La politique de l'organisation peut l'imposer. |
| `desktop.link_preview_enabled` | booléen | `false` | Récupérer l'aperçu d'un lien affiché dans une conversation. La politique de l'organisation peut l'imposer. |
| `desktop.update_channel` | l'une de stable, preview | `"stable"` | Canal de mise à jour de l'application : stable reçoit les versions publiées, preview aussi les versions d'essai. La politique de l'organisation peut l'imposer. |
| `desktop.background_service_allowed` | booléen | `true` | Autoriser l'application à installer le moteur comme service de la session de l'utilisateur. La politique de l'organisation peut l'imposer. |
| `desktop.chat_auto_naming` | l'une de never, auto, always | `"auto"` | Nommer une nouvelle conversation d'après son premier message : jamais, automatiquement quand le modèle génère assez vite, ou toujours. La politique de l'organisation peut l'imposer. |

### `[server]`

Le serveur compatible, qui répond aux formats OpenAI et Anthropic sur l'interface de boucle locale.

| Clé | Type | Défaut | Rôle |
| --- | --- | --- | --- |
| `server.enabled` | booléen | `false` | Démarrer le serveur compatible (formats OpenAI et Anthropic) avec le moteur. La politique de l'organisation peut l'imposer. |
| `server.port` | entier | `7772` | Port TCP du serveur compatible. La politique de l'organisation peut l'imposer. |
| `server.require_auth` | booléen | `false` | Exiger un jeton du serveur compatible à chaque requête ; lu à l'ouverture de l'écoute. La politique de l'organisation peut l'imposer. |
| `server.serve_on_lan` | booléen | `false` | Ouvrir le serveur compatible au réseau local, sur toutes les interfaces ; l'API native reste sur l'interface de boucle locale. Lu à l'ouverture de l'écoute. La politique de l'organisation peut l'imposer. |

### `[engines]`

Réglages des moteurs d'inférence locaux.

| Clé | Type | Défaut | Rôle |
| --- | --- | --- | --- |
| `engines.log_level` | l'une de error, warning, info, debug | `"debug"` | Niveau de journal du moteur local, appliqué au prochain chargement d'un modèle. La politique de l'organisation peut l'imposer. |

### `[author]`

Le nom d'éditeur des agents exportés depuis cette machine.

| Clé | Type | Défaut | Rôle |
| --- | --- | --- | --- |
| `author.name` | chaîne | `""` | Nom d'éditeur des agents exportés depuis cette machine ; vide, il est demandé à l'export. La politique de l'organisation peut l'imposer. |

### `[observability]`

Capture et rétention des traces. Lue par le démon et par l'application de bureau.

| Clé | Type | Défaut | Rôle |
| --- | --- | --- | --- |
| `observability.max_input_bytes` | entier | `32768` | Plus grande entrée d'exécution gardée, en octets. |
| `observability.max_output_bytes` | entier | `32768` | Plus grande sortie d'exécution gardée, en octets. |
| `observability.max_tool_output_bytes` | entier | `10240` | Plus grande sortie d'outil gardée, en octets. |
| `observability.capture_thoughts` | booléen | `true` | Garder le raisonnement du modèle dans l'historique des exécutions. |
| `observability.capture_tool_args` | booléen | `true` | Garder les arguments des outils dans l'historique des exécutions. |
| `observability.capture_tool_outputs` | booléen | `true` | Garder les sorties des outils dans l'historique des exécutions. |
| `observability.capture_agent_logs` | booléen | `true` | Garder les journaux des agents dans l'historique des exécutions. |
| `observability.retention_days` | entier | `90` | Jours pendant lesquels l'historique des exécutions est gardé. |

### Clés retirées

Acceptées dans un fichier écrit pour une version antérieure, avec un avertissement `config.key.retired`, et lues par rien.

| Clé | À la place |
| --- | --- |
| `llm.vertex` | Le client Google Vertex AI a été retiré ; déclarez plutôt un serveur compatible OpenAI. |
| `memory` | Rien ne la lit ; retirez la section. |
| `budget` | Rien ne la lit ; retirez la section. |
| `a2a` | Rien ne la lit ; retirez la section. |
| `oria` | Rien ne la lit ; retirez la section. |
| `registry` | Rien ne la lit ; retirez la section. |
| `permissions` | Rien ne la lit ; retirez la section. |
| `triggers` | Gérez-la depuis l'application de bureau ou l'API. |
| `notifications` | Gérez-la depuis l'application de bureau ou l'API. |
| `stt` | Gérez-la depuis l'application de bureau ou l'API. |
| `llm.observability.log_token_usage` | Les tokens de chaque appel de modèle sont toujours journalisés ; retirez la clé. |
| `llm.observability.log_latency` | La latence de chaque appel de modèle est toujours journalisée ; retirez la clé. |
| `llm.observability.log_cost` | Rien ne la lit ; retirez la clé. |
| `llm.pricing_overrides` | Rien ne la lit ; retirez la section. |
| `llm.cost_alert_threshold_usd` | Rien ne la lit ; retirez la clé. |
| `llm.runner` | Le moteur est choisi d'après le matériel détecté ; retirez la section. |
| `tools.file_path_extraction_pattern` | Rien ne la lit ; retirez la clé. |
| `filesystem` | Les outils de fichiers atteignent les dossiers accordés à chaque agent ; les chemins listés ici ont été ajoutés une fois comme connexions dossier, accordez-les sur la fiche de l'agent. |
| `chat.default_workspace` | Le dossier a été ajouté une fois comme connexion dossier ; accordez-le à un agent sur sa fiche. |
| `ui` | Déplacée : écrivez desktop.link_preview_enabled ; ui.link_preview_enabled est encore lue tant qu'elle est absente. |
<!-- END GENERATED: config-fields -->

### Le socket Unix, en entier

La valeur par défaut de `[api].unix_socket` est `~/.apollia/runtime.sock`, et un
socket sous le répertoire temporaire de la plateforme lorsque aucun répertoire
personnel ne peut être résolu. La clé ne développe pas `~` : le fichier par
défaut écrit donc le chemin absolu du socket sous le dossier personnel où il a
été écrit. Le serveur passe le fichier en `0600` juste
après le bind, si bien que seul le compte qui a démarré le runtime peut
l'atteindre.

Sous Windows il n'y a pas de socket Unix, et cette clé est inerte : ni le
serveur ni le client ne lisent le chemin. Le transport local y est un tube
nommé, `\\.\pipe\apollia-runtime-<user>`, dont le runtime dérive le nom de
`USERNAME` afin que deux comptes d'une même machine ne se disputent pas le même
tube. Le tube est créé avec un descripteur de sécurité par défaut plutôt
qu'avec le `0600` du fichier de socket : il porte donc le même jeton Bearer que
l'écouteur TCP, et c'est ce jeton qui contrôle l'accès. L'option globale
`--socket` est acceptée sous Windows et ignorée.

### Les dossiers qu'un agent atteint

Aucune clé de ce fichier ne dit quels dossiers un agent atteint. Un dossier du
poste est une connexion, ajoutée depuis l'écran Connexions au sélecteur du
système, et ses outils de fichiers s'accordent à un agent sur sa fiche. Les
outils de fichiers d'un agent atteignent l'union des dossiers qui lui sont
accordés, agent de conversation intégré compris : les outils de lecture tous
les dossiers accordés, les outils d'écriture seulement ceux accordés pour
écrire. Le premier dossier accordé, par date d'ajout, est celui où arrive un
chemin relatif. Un agent sans dossier accordé a des outils de fichiers qui
refusent, en disant comment en accorder un.

`[filesystem] trusted_paths` et `[chat] default_workspace` nommaient ces
dossiers. Les deux sont retirées. Au premier démarrage d'une version qui
connaît les connexions dossier, chaque dossier qu'elles nommaient, dossier
personnel excepté, a été ajouté une fois comme connexion, accordée à aucun
agent ; un fichier qui les porte encore se charge avec un avertissement
`config.key.retired`.

### Sections retirées

`[a2a]`, `[oria]`, `[registry]`, `[permissions]`, `[memory]` et `[budget]`
étaient acceptées auparavant. Chacune se désérialisait dans une structure typée
que rien ensuite ne consultait, si bien qu'écrire une valeur dedans n'avait aucun
effet et ne produisait pas non plus d'erreur. Plus rien ne les lit : un
fichier qui en porte encore une est accepté, avec un avertissement
`config.key.retired`, et le tableau des clés retirées plus haut dit quoi faire à
la place. Les retirer n'a changé aucun comportement, puisque ces sections n'en avaient aucun.

Des clés ont été retirées pour la même raison, parce que rien ne les lisait. Un
fichier qui en porte encore une est accepté, avec un avertissement
`config.key.retired` qui la nomme, et `apollia-os config set` refuse de
l'écrire :

| Clé retirée | Que faire à la place |
| --- | --- |
| `[llm.runner]`, et sa clé `backend` | Rien : le moteur de transcription vocale est choisi d'après le matériel détecté. |
| `[llm] pricing_overrides`, `cost_alert_threshold_usd` | Rien : aucun démon en marche ne les appliquait. `apollia-os llm costs` lit l'historique des coûts et n'écrit plus ni l'une ni l'autre. |
| `[llm.observability] log_token_usage`, `log_latency`, `log_cost` | Rien : les tokens et la latence sont toujours journalisés. |
| `[tools] file_path_extraction_pattern` | Rien. |
| `[filesystem.journal]` | Rien. |
| `[ui] link_preview_enabled` | Écrire `[desktop] link_preview_enabled`. Tant qu'elle n'est pas écrite, l'ancienne clé reste lue, avec un avertissement `config.key.moved`. |

`[autonomy]` n'est pas une section : aucun chargeur ne la lit, et un fichier qui
en porte une est refusé comme clé inconnue.

`[permissions]` mérite d'être détaillée, car son nom suggère le contraire :
aucune de ses quatre clés n'a jamais eu de lecteur sur un chemin d'exécution. La
gouvernance qui s'exécute réellement, règles de préfixe et approbations, ne
prend rien de cette section. Voir
[concepts transverses](/architecture/crosscutting-concepts).

Les paramètres d'échantillonnage sont documentés séparément dans
[Valeurs par défaut d'échantillonnage](/reference/sampling-defaults). Les clés
`[tools.web_search]` et `[tools.web_read]` sont aussi modifiables depuis la CLI
avec `apollia-os tools config set <tool>.<key> <value>`.

## Capture de trace (`[observability]`)

Lu par le démon lancé avec `apollia-os start` et par l'application desktop.
Modifiable depuis Réglages, Observabilité, avec
`apollia-os config set observability.<clé> <valeur>`, ou à la main.

Ces interrupteurs décident de ce qu'une exécution d'agent laisse sur disque,
dans `runtime_events.db`. Tout reste sur la machine, la question n'est donc pas
qui d'autre le voit, mais ce qui reste lisible localement après une
exécution.

<!-- claim:observability-capture-switches-enforced -->

| Clé | Valeur par défaut | Effet |
|---|---|---|
| `capture_thoughts` | `true` | Persiste le texte de raisonnement de chaque tour ReAct. Désactivé, le tour ne laisse aucune ligne de pensée. |
| `capture_agent_logs` | `true` | Persiste les messages émis via `ctx.logger`. Désactivé, aucune ligne de log. |
| `capture_tool_args` | `true` | Persiste le JSON des arguments de chaque appel d'outil. Désactivé, l'appel reste tracé, mais sans ses arguments. |
| `capture_tool_outputs` | `true` | Persiste le JSON de sortie de chaque appel d'outil. Désactivé, l'appel et son résultat restent liés, mais sans le contenu. |
| `retention_days` | `90` | Nombre de jours d'événements conservés. La purge s'exécute au démarrage et ne supprime rien d'autre : la piste d'audit et le journal d'audit signé sont des magasins séparés. `0` signifie ne jamais purger. |
| `max_input_bytes` | `32768` | Seuil de troncature pour une entrée de tâche persistée. |
| `max_output_bytes` | `32768` | Seuil de troncature pour une sortie de tâche persistée. |

<!-- claim:retention-purges-runtime-events-only -->
Désactiver un interrupteur vide la partie correspondante de la timeline : la
piste d'audit et l'historique des coûts sont séparés et n'en sont pas
affectés. La même séparation vaut pour la rétention : la purge ne supprime que
depuis le journal d'événements. Le journal d'audit signé est une chaîne de
hachage que `audit verify` parcourt de bout en bout, il n'est donc jamais
purgé sur une base temporelle.

### Contenu des prompts (`[llm.observability]`)

<!-- claim:debug-log-prompt-logs-at-trace -->

Le texte du prompt n'est **jamais écrit dans une base de données**. Le seul
réglage capable de l'exposer est `debug_log_prompt`, et il vit sous
`[llm.observability]`, pas sous `[observability]` :

| Clé | Valeur par défaut | Effet |
|---|---|---|
| `debug_log_prompt` | `false` | Émet le prompt complet au niveau `TRACE`, sur le chemin de complétion comme sur le chemin de streaming. Rien n'est persisté ; l'exposition dépend de ce qui collecte le flux de logs. |

**L'interrupteur seul ne montre rien.** Le filtre de log par défaut est
`apollia=info`, et `TRACE` se situe en dessous, si bien que le prompt est émis
à un niveau qui est filtré. Le voir exige à la fois `debug_log_prompt = true`
et un filtre au niveau trace, par exemple `RUST_LOG=apollia=trace`. C'est un
second verrou délibéré, pas un oubli, et c'est pourquoi le réglage peut sans
risque rester visible dans l'interface.

La section est lue quand le runtime construit son routeur : un changement
s'applique au démarrage suivant. Un `debug_log_prompt` écrit sous
`[observability]` est refusé comme clé inconnue : les deux sections sont deux
types différents, et seul celui de `[llm.observability]` atteint le routeur.
Les autres clés que portait cette section, `log_token_usage`, `log_latency` et
`log_cost`, sont retirées : les tokens et la latence de chaque appel au modèle
sont toujours journalisés.

### Clés déclarées mais non implémentées

| Clé | État |
|---|---|
| `capture_thoughts` sur les étapes de plan ORIA | Partiel. Les entrées et sorties des étapes de plan sont persistées avec les valeurs par défaut compilées, si bien que les limites d'octets ci-dessus ne s'y appliquent pas. |
| `max_tool_output_bytes` | **Non implémenté.** N'a jamais eu de point d'usage. |

Ces clés sont listées plutôt que masquées parce que la page de réglages en
affiche encore certaines. Un interrupteur qui ressemble à un contrôle de
confidentialité et ne fait rien est pire qu'un interrupteur absent, donc tant
qu'elles ne sont pas implémentées ou retirées, ce tableau fait foi.

## Garde-fous de chargement (`[llm.guardrails]`)

Avant le lancement d'un modèle local, sa mémoire est estimée avec la fenêtre de
contexte et le nombre de requêtes parallèles avec lesquels il tournerait
vraiment, puis jugée face à ce poste : il tient, il pourrait tenir (partagé entre
une carte graphique et la mémoire système), ou il est trop gros. Le niveau décide
de ce qui est lancé.

```toml
[llm.guardrails]
level = "balanced"   # disabled, lenient, balanced, strict ou ceiling
ceiling_gb = 12.0    # lu par le niveau ceiling seulement
```

| Niveau | Lance |
| --- | --- |
| `disabled` | tous les modèles, sans estimation |
| `lenient` | tous les modèles, avec un avertissement au journal au-delà de « pourrait tenir » |
| `balanced` | jusqu'à « pourrait tenir ». Le défaut |
| `strict` | seulement un modèle qui tient |
| `ceiling` | un modèle estimé à `ceiling_gb` gigaoctets ou moins |

La section est relue à chaque lancement : un changement vaut pour le prochain
modèle chargé, sans redémarrage. Un modèle déjà chargé n'est pas arrêté. Un refus
donne l'estimation et la limite : `POST /api/v1/llm/chat` et
`POST /api/v1/llm/complete` y répondent par le statut 409, le code
`guardrail_refused`, le niveau, `estimatedGb` et `budgetGb`. Une section illisible, un niveau inconnu ou un
niveau `ceiling` sans `ceiling_gb` positif valent `balanced` : une faute de
frappe ne coupe jamais le garde-fou. Un modèle dont l'empreinte ne peut pas être
estimée est lancé.

Plusieurs modèles peuvent rester chargés ensemble, autant que le garde-fou en
admet : un modèle est jugé avec l'empreinte des modèles déjà chargés ajoutée à
la sienne. Quand cette somme est refusée, les modèles chargés que rien ne
retient (aucune conversation ouverte, aucune épingle, aucune réponse en cours)
sont déchargés, le moins récemment utilisé d'abord et seulement autant qu'il le
faut ; quand même cela ne suffirait pas, rien n'est déchargé et le chargement
est refusé.

## Résidence des modèles (`[llm.residency]`)

Un modèle local que rien n'utilise rend sa mémoire de lui-même.

```toml
[llm.residency]
idle_unload_minutes = 60   # 0 garde chargé un modèle inutilisé
```

| Règle | Décharge |
| --- | --- |
| Délai d'inactivité | un modèle qu'aucune requête n'a utilisé depuis `idle_unload_minutes`, vérifié toutes les trente secondes |
| Dernière conversation fermée | un modèle aussitôt, quand la dernière conversation ouverte qui l'utilise se ferme |

Un modèle que l'utilisateur a épinglé n'est déchargé par aucune des deux règles,
et un modèle qui sert une réponse n'est jamais déchargé. L'épingle est gardée
dans `system.db` et survit à un redémarrage. La clé est relue à chaque
vérification : un changement s'applique sans redémarrage. Une interface dit
quelles conversations sont ouvertes avec `POST /api/v1/llm/leases` et
`POST /api/v1/llm/leases/release`, et épingle un modèle avec
`PUT /api/v1/llm/pins`. Quand le flux d'événements qu'elle a ouvert avec
`GET /api/v1/events?client=<id>` se termine, les conversations de ce client
comptent comme fermées. Chaque déchargement est publié sur le bus d'événements
sous `LlmModelUnloaded`, avec sa raison : `idle`, `last_conversation`,
`requested` ou `make_room`.

## Dictée (`system.db`, pas `apollia.toml`)

La dictée vocale n'a pas de section `apollia.toml`. Ses dix réglages vivent
dans une unique ligne de `~/.apollia/system.db`, écrite depuis Réglages,
Speech-to-Text dans l'application desktop, ou avec
`apollia-os stt config get` et `apollia-os stt config update`. Écrire un bloc
`[stt]` dans `apollia.toml` ne change rien : rien ne le lit.

<!-- claim:stt-settings-apply-without-restart -->

Sauvegarder réarme le flux de capture, si bien qu'un changement prend effet
dès la prochaine dictée sans redémarrer l'application.

| Clé | Valeur par défaut | Effet |
|---|---|---|
| `enabled` | `false` | Indique si le moteur de dictée démarre et si le raccourci global est armé. |
| `model_path` | *(vide)* | Fichier de modèle Whisper. `~` est développé. L'application desktop scrute `~/.apollia/models` pour les fichiers `.bin` et `.gguf`. |
| `hotkey` | `ctrl+shift+space` | Raccourci global qui démarre et arrête la dictée. |
| `trigger_mode` | `toggle` | `toggle` (appui pour démarrer, appui pour arrêter) ou `push-to-talk` (maintien). |
| `input_device` | *(non défini)* | Nom du microphone tel que rapporté par le système. Non défini signifie l'entrée par défaut du système. |
| `language` | *(non défini)* | Langue forcée sur le moteur. Non défini signifie détection automatique. Valeurs acceptées ci-dessous. |
| `silence_threshold_db` | `-40.0` | Niveau RMS, en dB, en dessous duquel une fenêtre de 10 ms compte comme du silence. |
| `max_recording_sec` | `60` | Durée maximale d'enregistrement conservée. Au-delà, l'audio est tronqué. |
| `clipboard_mode` | `paste` | `paste`, `clipboard`, `memo` ou `both`. S'applique uniquement à la dictée par raccourci. |
| `clipboard_restore` | `true` | Restaure le contenu précédent du presse-papiers après un collage. |

### Codes de langue acceptés

<!-- claim:stt-language-hint-is-a-closed-list -->

`language` est un code ISO 639-1 issu de cette liste fermée, ou non défini
pour la détection automatique. L'application desktop propose exactement ces
langues dans un sélecteur ; une valeur en dehors de la liste est rejetée
plutôt que transmise telle quelle, si bien que deux machines ne peuvent pas
finir avec des orthographes différentes de la même langue.

| Code | Langue | Code | Langue |
|---|---|---|---|
| `fr` | Français | `pl` | Polonais |
| `en` | Anglais | `ru` | Russe |
| `es` | Espagnol | `zh` | Chinois |
| `de` | Allemand | `ja` | Japonais |
| `it` | Italien | `ko` | Coréen |
| `pt` | Portugais | `ar` | Arabe |
| `nl` | Néerlandais | | |

<!-- claim:stt-api-language-is-per-request -->

`POST /stt/transcribe` accepte aussi un champ `language`, qui ne s'applique
qu'à cette requête et écrase la valeur enregistrée ; l'envoyer vide signifie
détection automatique pour cette requête.

### Le silence n'est pas transcrit

<!-- claim:stt-refuses-silent-audio -->

Un enregistrement dont chaque fenêtre de 10 ms se situe sous
`silence_threshold_db` est écarté plutôt qu'envoyé au modèle. Ce n'est pas une
optimisation. Whisper ne répond pas au silence par une chaîne vide, il répond
par un remplissage appris de ses données d'entraînement, et ces inventions
arrivaient auparavant comme s'il s'agissait de transcriptions réelles.
L'interface signale que rien d'audible n'a été capté, et la ligne de log
`stt.audio.nothing_audible` enregistre le niveau de crête mesuré, ce qui
permet de distinguer un microphone coupé d'un seuil réglé trop haut.

## Serveurs MCP (`[[mcp.servers]]`)

Chaque entrée configure un serveur MCP. Les limites pertinentes pour la
sécurité :

### `max_response_bytes`

Nombre maximal d'octets acceptés depuis une seule réponse de serveur avant que
le transport n'interrompe la lecture avec une erreur.

- Type : entier (octets)
- Valeur par défaut : `8388608` (8 Mio)
- Bornes : `1024` à `1073741824` (1 Kio à 1 Gio)
- S'applique aux transports : `stdio`, `streamable-http` et `sse`

Les serveurs MCP sont non fiables. Un serveur qui ne termine jamais une ligne,
qui diffuse sans fin, ou qui renvoie un corps surdimensionné ferait sinon
croître la mémoire du démon sans limite. Le plafond borne une seule ligne
stdio, une lecture de corps HTTP, et le tampon de réception SSE. Augmentez-le
pour les serveurs dont les charges utiles d'outils sont légitimement
volumineuses.

### `max_tools`

Nombre maximal d'outils conservés depuis la liste d'outils d'un serveur. Les
outils au-delà du plafond sont écartés à la découverte.

- Type : entier (compte)
- Valeur par défaut : `256`
- Bornes : `1` à `8192`

Les serveurs MCP sont non fiables. Un serveur annonçant des milliers d'outils
saturerait sinon le registre d'outils et le catalogue d'outils du modèle,
épuisant contexte et mémoire. Les noms d'outils sont eux aussi validés
(écartés sauf s'ils correspondent à `[A-Za-z0-9_.-]`) et les descriptions
d'outils sont dépouillées de leurs caractères de contrôle, si bien qu'un
serveur ne peut ni forger de lignes de log ni glisser d'instructions dans le
contexte du modèle. Augmentez `max_tools` pour les serveurs d'agrégation qui
exposent légitimement de nombreux outils.

```toml
[[mcp.servers]]
name = "example"
transport = "streamable-http"
url = "https://mcp.example.com/mcp"
max_response_bytes = 16777216  # 16 Mio
max_tools = 512
```

### `tool_families`

La famille d'action de chaque outil, indexée par le nom de l'outil sur le
serveur. Le bureau l'écrit quand une connexion est ajoutée depuis la sélection
Apollia ; ne la renseignez pas pour un autre serveur.

- Type : table d'un nom d'outil vers une famille
- Familles : `read`, `read_personal`, `write_or_delete`, `send_external`,
  `execute_code`
- Valeur par défaut : vide
- Bornes : au plus `8192` entrées, chaque clé un nom d'outil valide, aucune
  valeur `unclassified`

Un outil listé ici prend cette famille. Un outil absent est classé d'après les
indications que déclare son serveur (`readOnlyHint`, `destructiveHint`) : en
lecture seule, il lit, ou lit des données personnelles quand la connexion porte
un jeton OAuth ou un secret enregistré ; destructif, ou déclaré non en lecture
seule, il écrit ou supprime. Un outil sans indication n'est pas classé et il est
traité comme sensible. Les indications viennent d'un serveur non fiable : elles
classent un outil sans jamais le laisser s'exécuter comme une lecture seule, et
aucune ne peut classer un outil comme un envoi hors de la machine.

```toml
[[mcp.servers]]
name = "notion"
transport = "streamable-http"
url = "https://mcp.notion.com/mcp"

[mcp.servers.tool_families]
notion-search = "read_personal"
notion-create-pages = "write_or_delete"
```
