---
sidebar_position: 10
title: Points d'accès compatibles
description: "Le serveur compatible : un second écouteur du runtime, sur son propre port, que les autres applications appellent aux formats d'OpenAI et d'Anthropic, pour les modèles chargés et les agents et workflows rendus disponibles. Éteint par défaut."
---

# Points d'accès compatibles

Le runtime peut ouvrir un second écouteur, le **serveur compatible**, pour les
autres applications de la machine. Elles l'appellent comme elles appelleraient
OpenAI ou Anthropic : les chemins commencent par `/v1/`, les corps suivent ces
API et leurs noms de champs (`snake_case`), et une erreur revient dans
l'enveloppe de l'API que le client a appelée.

Le serveur compatible n'est pas l'API native. L'API native, documentée dans la
[référence de l'API HTTP](/reference/api/apollia-os-runtime-api), reste sur le
port 7771 et le socket local, sous `/api/v1/` ; l'application de bureau et la
CLI pilotent le runtime par elle. Le serveur compatible a son propre port et ne
sert que les points d'accès listés plus bas, si bien que le démarrer ou
l'arrêter ne touche jamais au runtime ni au bureau.

## Éteint par défaut

Le serveur n'écoute qu'une fois que quelqu'un l'allume. Cinq clés
d'`apollia.toml` portent son état :

```toml
[server]
enabled = false        # le serveur compatible écoute-t-il
port = 7772            # son port TCP ; jamais le 7771 de l'API native
require_auth = false   # chaque requête doit-elle présenter un jeton
cors = false           # une page web d'une autre origine peut-elle l'appeler
serve_on_lan = false   # les autres appareils du réseau local l'atteignent-ils
```

Quatre autres disent comment il charge et décharge les modèles à la demande ;
voir [Chargement à la demande](#chargement-à-la-demande).

Par défaut, il écoute sur l'interface de boucle locale, `127.0.0.1`, si bien que
seules les applications de cette machine l'atteignent.

## Appels depuis une page web (CORS)

Un navigateur ne laisse une page appeler un serveur d'une autre origine que si
ce serveur l'autorise, et il le demande d'abord : avant une requête qui porte
un jeton ou un corps JSON, il envoie un pré-vol `OPTIONS`. Avec
`cors = false`, la valeur par défaut, le serveur compatible n'envoie aucun
en-tête d'origine croisée, et le navigateur refuse l'appel. Avec
`cors = true`, une page de n'importe quelle origine peut l'appeler :

- un pré-vol reçoit `204 No Content` avec
  `Access-Control-Allow-Origin: *`,
  `Access-Control-Allow-Methods: GET, POST, OPTIONS` et
  `Access-Control-Allow-Headers: authorization, content-type, x-api-key, anthropic-version`,
  les en-têtes qu'envoient les SDK d'OpenAI et d'Anthropic ;
- toute autre réponse, erreur comprise, porte
  `Access-Control-Allow-Origin: *`.

Le pré-vol n'exige aucun jeton, puisqu'un navigateur n'en envoie jamais sur
lui ; la requête qui suit est vérifiée comme toute autre. Le réglage est lu au
démarrage du serveur : après l'avoir changé, arrêtez puis redémarrez le
serveur. L'API native, sur le port 7771, ne porte jamais ces en-têtes, quel que
soit le réglage.

## Le démarrer et l'arrêter

Quatre routes de l'API native le pilotent pendant que le runtime tourne :

| Route | Effet |
|---|---|
| `GET /api/v1/server` | S'il écoute, son port, s'il sert le réseau local, l'adresse à copier, pourquoi son dernier démarrage a échoué, combien de jetons sont actifs, et lesquels de ses réglages la [politique de l'organisation](#politique-de-lorganisation) fige |
| `POST /api/v1/server/start` | L'ouvre sur le port configuré et écrit `enabled = true` |
| `POST /api/v1/server/stop` | Le ferme et écrit `enabled = false` ; l'API native continue de répondre |
| `POST /api/v1/server/restart` | Le ferme et le rouvre pour qu'il applique les clés `[server]` telles qu'elles sont ; seul cet écouteur est rouvert, l'API native garde ses connexions. Un serveur arrêté reste arrêté |

Comme démarrer et arrêter écrivent `enabled`, l'état survit à une relance : un
runtime qui démarre avec `enabled = true` rouvre le serveur compatible. Si un
autre programme tient déjà le port, le démarrage est refusé par une erreur
lisible « port occupé » (HTTP 409 sur la route de démarrage), et le runtime
continue sans le serveur compatible.

## Jetons

Avec `require_auth = true`, chaque requête doit présenter un jeton du serveur.
Chaque application reçoit le sien, à son nom, si bien qu'on peut en retirer un
sans toucher aux autres. Le jeton par lequel le bureau parle à son runtime n'en
fait pas partie et n'ouvre jamais le serveur compatible ; un jeton du serveur
n'ouvre jamais l'API native.

| Route | Effet |
|---|---|
| `POST /api/v1/server/tokens` | Crée un jeton à partir de son `name` et répond sa valeur, `apo_` suivi de 64 caractères hexadécimaux. La valeur est montrée cette seule fois et gardée nulle part |
| `GET /api/v1/server/tokens` | Liste chaque jeton : nom, quatre derniers caractères, création, dernier usage, révocation. Jamais la valeur |
| `DELETE /api/v1/server/tokens/{token}` | Révoque un jeton par identifiant ou par nom ; il reste listé, marqué révoqué |

Seule l'empreinte SHA-256 d'une valeur est gardée, dans `system.db`. Un client
envoie son jeton comme sa propre API l'attend, l'un ou l'autre en-tête convient :

```text
Authorization: Bearer apo_...   # clients OpenAI
x-api-key: apo_...              # clients Anthropic
```

Une requête sans jeton actif, ou avec un jeton révoqué, reçoit `401` au format
du point d'accès appelé. Chaque succès date le dernier usage du jeton. Le
réglage est lu à l'ouverture du serveur : le relancer applique un changement.

## Réseau local

Avec `serve_on_lan = true`, le serveur compatible écoute sur toutes les
interfaces, si bien que les autres appareils du réseau local l'appellent à
l'adresse de la machine. Le champ `address` de `GET /api/v1/server` donne
l'adresse à copier : `http://127.0.0.1:7772` quand le serveur reste sur la
machine, `http://<adresse IPv4 de la machine sur le réseau>:7772` quand il sert
le réseau. La clé est lue à l'ouverture du serveur : le relancer applique un
changement.

Le réseau local et les jetons sont deux interrupteurs distincts :

- **Les modèles** répondent au réseau avec ou sans jeton, comme le dit
  `require_auth`. Ouvrir le réseau sans jeton est permis, comme d'autres
  serveurs d'inférence locaux le permettent ; tout appareil du réseau peut alors
  se servir des modèles.
- **Les agents et les workflows** ne répondent jamais à un appel venu du réseau
  sans jeton actif, quoi que dise `require_auth` : ils atteignent les connexions
  et les documents de l'utilisateur. Le refus est un `401` qui se lit « A token
  is required to call an agent or a workflow from the local network. »
- **L'API native** reste sur la machine dans tous les cas : le port 7771 et la
  socket locale n'écoutent jamais le réseau à cause de cette clé.

Chaque requête est classée, à l'acceptation de sa connexion, comme venue de la
machine (l'interface de boucle locale) ou du réseau local (toute autre adresse).
La décision et sa lecture au regard du principe du local d'abord sont écrites
dans les [décisions d'architecture](/architecture/decisions#local-server).

## Politique de l'organisation

Une organisation qui dépose une politique signée sur le poste ferme trois
réglages du serveur compatible, par la table `[settings]` de son `policy.toml` :

```toml
[settings]
"server.serve_on_lan" = false        # le serveur reste sur la machine
"server.require_auth" = true         # chaque requête présente un jeton
"server.allow_request_mcp" = false   # aucune requête n'apporte de serveur MCP
```

| Clé | La valeur qu'une politique fige | Ce qu'elle ferme |
|---|---|---|
| `server.serve_on_lan` | `false` | Le réseau local : le serveur écoute sur la seule interface de boucle locale |
| `server.require_auth` | `true` | Un serveur sans jeton : chaque requête en présente un |
| `server.allow_request_mcp` | `false` | Les serveurs MCP apportés par une requête : une telle requête répond `400`, et le message nomme la politique |

Une politique ne fait que fermer. Celle qui fige l'une des trois clés à l'autre
valeur est refusée en entier à sa lecture, comme une politique qui nomme une clé
inconnue : ouvrir le serveur reste un geste de l'utilisateur, jamais celui d'un
fichier déposé sur le poste.

Une clé que la politique fige est lue dans la politique, quoi que dise
`apollia.toml`, et son écriture est refusée par « setting 'server.serve_on_lan'
is locked by the organization policy ». `GET /api/v1/server` dit quelles clés
sont figées dans son champ `imposedByPolicy`, un booléen par clé : `serveOnLan`,
`requireAuth`, `allowRequestMcp`.

Le runtime lit la politique à son démarrage, avant que le serveur n'ouvre. Un
serveur qu'`apollia.toml` avait laissé allumé et ouvert contre la politique
ouvre donc fermé, et le journal d'audit garde une entrée par clé fermée ainsi,
de nature `policy_setting_enforced`, dans l'exécution
`governance:policy:organisation`. L'entrée nomme la clé et ne porte aucune
valeur. La façon dont une politique se dépose et se signe est écrite dans les
[décisions d'architecture](/architecture/decisions#organisation-policy).

## Journaux du serveur

Le runtime garde en mémoire les lignes les plus récentes du serveur, 2000 au
plus, la plus ancienne retirée d'abord : une par requête à laquelle il a
répondu (heure, niveau, méthode, chemin, modèle, statut, durée) et chaque ligne
qu'écrit le `llama-server` local. Une requête refusée (4xx) est au niveau
`warn`, une requête en échec (5xx) au niveau `error`.

| Route | Effet |
|---|---|
| `GET /api/v1/server/logs?level=warn` | Un flux d'événements : une trame `log` par ligne, celles gardées d'abord, puis chaque nouvelle. `level` est le niveau le plus bas envoyé, `debug` (par défaut), `info`, `warn` ou `error` |
| `DELETE /api/v1/server/logs` | Vide les lignes gardées et répond combien il y en avait. Le journal d'audit n'est pas touché |

Ces lignes sont une vue pour qui développe contre le serveur, pas un
enregistrement : elles sont perdues à l'arrêt du runtime.

## Journal d'audit

L'enregistrement, c'est le journal d'audit. Chaque requête que reçoit le
serveur y laisse une entrée, de nature `server_call`, dans l'exécution
`server:calls`, chaînée et signée comme toutes les autres : une requête servie,
une requête refusée (`401`, `400`) et une requête en échec, de la même façon.

| Champ | Ce qu'il porte |
|---|---|
| `method`, `endpoint` | La méthode et le chemin demandés, sans la chaîne de requête |
| `target`, `target_kind` | Le modèle nommé, ou l'agent ou le workflow (`model` ou `agent`) ; absent quand la requête n'en nommait aucun que le serveur ait pu lire |
| `token_name` | Le nom du jeton présenté, jamais sa valeur ; absent sans jeton actif |
| `caller_address`, `origin` | L'adresse depuis laquelle la connexion a été acceptée, et `loopback` ou `local_network` |
| `status` | Le statut répondu |
| `prompt_tokens`, `completion_tokens` | Les jetons que la réponse a déclarés, lus au passage dans un flux ; absents quand elle n'en déclarait pas |
| `duration_ms` | La durée de la réponse ; pour un flux, jusqu'à sa fermeture |
| `task_id`, `run_id` | Pour un appel à un agent ou à un workflow, la tâche et l'exécution qu'il a lancées |

Une entrée ne porte jamais un message, une réponse, un en-tête ni la valeur
d'un jeton. Un en-tête qui prétend une autre adresse n'est pas lu : l'adresse
est celle de la connexion. Les entrées de l'exécution d'un agent sont gardées
sous le `run_id` que nomme son appel, si bien que ce qu'une application a
demandé à un agent se suit depuis l'appel.

## Erreurs

Chaque point d'accès répond ses erreurs au format de sa propre API. Les points
d'accès d'OpenAI, et tout chemin que le serveur ne sert pas, suivent
l'enveloppe d'OpenAI :

```json
{
  "error": {
    "message": "Unknown endpoint: /v1/unknown",
    "type": "invalid_request_error",
    "code": "unknown_url"
  }
}
```

Les points d'accès d'Anthropic (`/v1/messages` et `/v1/messages/count_tokens`)
suivent celle d'Anthropic : `{"type": "error", "error": {"type", "message"}}`.

| Statut | `code` OpenAI | `type` Anthropic | Quand |
|---|---|---|---|
| `400` | `invalid_request` | `invalid_request_error` | Le corps n'est pas un objet JSON, ou ne nomme aucun `model` |
| `401` | `invalid_api_key` | `authentication_error` | `require_auth` est allumé et aucun jeton en service n'a été présenté |
| `404` | `model_not_loaded` | `not_found_error` | Le modèle nommé dans `model` n'est pas chargé, et n'est pas chargé à la demande |
| `404` | `endpoint_not_served` | `not_found_error` | Le moteur qui fait tourner le modèle ne sert pas ce point d'accès |
| `404` | `unknown_url` | | Aucun point d'accès ne vit à ce chemin |
| `502` | `engine_unavailable` | `api_error` | Le moteur qui fait tourner le modèle n'a pas répondu, ou le modèle n'a pas pu être chargé à la demande |

Un agent ou un workflow appelé comme un modèle ajoute ses propres erreurs, sur
`/v1/chat/completions` :

| Statut | `code` OpenAI | Quand |
|---|---|---|
| `400` | `invalid_request` | La requête apporte des `tools`, ne contient aucun message de l'utilisateur, ou, pour un workflow, son dernier message de l'utilisateur n'est pas un objet JSON conforme au schéma d'entrées ; l'erreur porte alors ce schéma dans `schema` |
| `401` | `invalid_api_key` | L'appel vient du réseau local sans jeton en service |
| `404` | `model_not_found` | Aucun agent ni workflow disponible pour d'autres applications ne porte cet identifiant `apollia/...` |
| `409` | `approval_pending` | L'agent attend une décision sur un accès que personne n'a accordé (voir plus bas) |
| `500` | `run_failed` | L'exécution a échoué ou a été arrêtée avant sa fin |
| `503` | `run_unavailable` | Ce runtime ne peut pas exécuter d'agents ni de workflows |

## Les modèles servis

Le serveur compatible sert les modèles locaux de cette machine, et eux
seuls. Une requête en nomme un dans son champ `model`, par l'identifiant que
liste `GET /v1/models`, qui est son identifiant dans le registre des modèles
d'Apollia. Le serveur transmet la requête au moteur qui fait tourner le
modèle, `llama-server` pour un modèle GGUF, `vllm-mlx` pour un modèle MLX, avec
`model` remplacé par ce que ce moteur attend, et rend la réponse du moteur
telle quelle : un corps JSON, ou un flux d'événements relayé morceau par
morceau quand la requête demandait `"stream": true`.

- Un modèle de langage du registre qui n'est pas chargé est chargé pour la
  requête, sauf si le chargement à la demande est éteint ; il répond alors
  `404 model_not_loaded`, et doit d'abord être chargé dans Apollia.
- Un modèle d'un fournisseur distant n'est jamais relayé : le serveur
  compatible n'est pas une passerelle vers un nuage.
- Rien n'est traduit d'un format dans un autre. Un point d'accès que le moteur
  du modèle ne sert pas répond `404 endpoint_not_served` plutôt qu'une
  approximation. Le modèle de recherche par le sens sert `/v1/embeddings` et
  rien d'autre ; un modèle de conversation sert tous les autres points d'accès
  ci-dessous, sauf que `vllm-mlx` ne compte pas les jetons.

Un modèle reste chargé tant qu'une requête qui lui est adressée tourne, flux
compris : le décharger attend la fin de la réponse.

## Chargement à la demande

Le serveur charge le modèle qu'une requête nomme quand il n'est pas encore
chargé, et rend de lui-même la mémoire des modèles qu'il a chargés.

```toml
[server]
jit_loading = true            # une requête charge le modèle de cette machine qu'elle nomme
jit_auto_unload = true        # les modèles chargés par le serveur ont leur propre délai
jit_idle_ttl_minutes = 60     # ce délai ; 0 les garde chargés
jit_keep_last_only = true     # en charger un décharge les autres que le serveur a chargés
```

| Clé | Défaut | Effet |
|---|---|---|
| `jit_loading` | `true` | Une requête qui nomme un modèle de langage du registre non chargé le charge d'abord. Éteinte, elle répond `404 model_not_loaded`. Allumée, `GET /v1/models` liste les modèles de langage de cette machine en plus des modèles chargés |
| `jit_auto_unload` | `true` | Un modèle chargé par le serveur est déchargé après `jit_idle_ttl_minutes` sans requête, au lieu du délai général de `[llm.residency]`. Éteinte, il suit le délai général |
| `jit_idle_ttl_minutes` | `60` | Ce délai, en minutes ; `0` garde ces modèles chargés |
| `jit_keep_last_only` | `true` | Quand le serveur charge un modèle, il décharge les autres modèles qu'il a chargés |

La liste des modèles chargés de l'API native, `GET /api/v1/llm/loaded-models`,
les distingue : un modèle chargé par le serveur porte `"origin": "server"`, les
autres `"origin": "app"`. L'origine est fixée au chargement : un modèle que
l'application a chargé reste celui de l'application quand une requête s'en
sert.

Aucune de ces règles ne décharge un modèle épinglé par l'utilisateur, un modèle
qu'une conversation ouverte utilise, ni un modèle qui sert une réponse, et ne
garder que le dernier ne touche jamais un modèle chargé par l'application.
Chaque déchargement est publié sous `LlmModelUnloaded`, avec la raison
`idle`, ou `server_replaced` quand le serveur a chargé un autre modèle. Les
clés sont relues à chaque requête et à chaque vérification : un changement
s'applique sans redémarrage.

## Raisonnement d'un modèle qui réfléchit

Un modèle qui réfléchit écrit son raisonnement avant sa réponse. Le moteur
sépare les deux, et une complétion de chat porte le raisonnement dans
`reasoning_content`, dans le message d'une réponse entière et dans chaque delta
d'un flux, à part de `content`. C'est ce que le serveur relaie par défaut.

```toml
[server]
separate_reasoning_content = true   # false : le raisonnement revient dans content
```

Avec `separate_reasoning_content = false`, le serveur remet le raisonnement en
tête de `content`, entre `<think>` et `</think>`, et retire `reasoning_content`,
pour un client qui ne lit que `content`. Dans un flux, la balise s'ouvre sur le
premier delta de raisonnement et se ferme sur le premier delta de la réponse, ou
sur le delta qui termine le choix. Seul `/v1/chat/completions` est concerné :
les formats responses et messages portent leurs propres blocs de raisonnement,
relayés tels que le moteur les a écrits. La clé est lue à chaque requête.

## Les agents et workflows comme modèles

Un agent ou un workflow que l'utilisateur a rendu disponible pour d'autres
applications, le même interrupteur que lit le pont MCP, est servi lui aussi
comme un modèle. `GET /v1/models` le liste après les modèles locaux, sous
`apollia/<id>` avec le `type` `agent` ou `workflow` : un agent par son
identifiant (`apollia/demand-clerk`), un workflow par son paquet et son nom
(`apollia/office/close-month`). Un identifiant d'agent ne porte jamais de `/`,
si bien qu'un agent et un workflow ne répondent jamais au même nom.

Un client l'appelle comme il appellerait un modèle, avec le client qu'il
emploie déjà : `POST /v1/chat/completions`, `POST /v1/responses` ou
`POST /v1/messages`. Les règles ci-dessous valent pour les trois ; l'exemple et
les noms de champs sont ceux de chat completions, et
[les deux autres formats](#responses-et-messages) disent où chacun range les
mêmes choses.

```bash
curl http://127.0.0.1:7772/v1/chat/completions \
  -H "content-type: application/json" \
  -d '{
    "model": "apollia/demand-clerk",
    "messages": [{"role": "user", "content": "Qualifie la demande de Durand."}]
  }'
```

- **L'appel ne garde aucun état.** Pour un agent, le dernier message de
  l'utilisateur est la demande ; les messages qui le précèdent sont la
  conversation d'où elle vient. Pour un workflow, le dernier message de
  l'utilisateur est un objet JSON conforme au schéma d'entrées du workflow
  (`{"month": "2026-09"}`) ; tout autre contenu répond `400` avec le schéma.
- **L'agent travaille comme dans Apollia.** Il s'exécute sans surveillance,
  avec son propre budget, ses accès et ses connexions, par la même exécution
  qu'un élément de lot. Des outils envoyés par le client sont refusés par un
  `400` : l'agent a ses accès.
- **La réponse est la réponse finale de l'agent**, dans
  `choices[0].message.content`. `usage` additionne les jetons de chaque tour du
  modèle pendant l'exécution.
- **Les étapes sont du raisonnement.** Chaque outil que l'agent appelle, et le
  résumé de ce qu'il a rendu, est une ligne de `reasoning_content`, le champ
  qu'emploient les serveurs d'inférence locaux pour le raisonnement d'un
  modèle : dans le message d'une réponse entière, et, avec `"stream": true`,
  dans des morceaux qui précèdent le morceau `content` de la réponse. Un client
  qui ne lit pas ce champ l'ignore.
- **Un client qui s'en va arrête l'exécution.** Fermer la connexion avant la
  réponse, ou quitter un flux, arrête l'agent ou le workflow.
- **Le réseau local exige un jeton.** Un appel à un agent ou à un workflow
  depuis un autre appareil présente un jeton en service, quoi que dise
  `require_auth`, et `GET /v1/models` ne les liste pas à un appareil qui n'en a
  pas.

### Responses et messages

La même exécution répond à un client de l'API Responses d'OpenAI et à un client
de l'API Messages d'Anthropic. Seules changent la lecture de la demande et
l'écriture de la réponse :

| | `POST /v1/responses` | `POST /v1/messages` |
|---|---|---|
| La demande | Le dernier élément d'`input` dont le rôle est `user`, ou `input` lui-même quand c'est un texte | Le dernier message de `messages` dont le rôle est `user` |
| La conversation | Les éléments message qui le précèdent ; un élément qui n'est pas un message est laissé de côté | Les messages qui le précèdent |
| Consignes du client | `instructions` | `system`, un texte ou des blocs de texte |
| La réponse | Le dernier élément d'`output`, un `message` qui porte un `output_text` | Le dernier bloc de `content`, un bloc `text` |
| Les étapes | Un élément `reasoning` par étape, avant le message, son texte dans `summary` | Un bloc `thinking` par étape, avant le texte |
| `usage` | `input_tokens`, `output_tokens`, `total_tokens` | `input_tokens`, `output_tokens` |

```bash
curl http://127.0.0.1:7772/v1/messages \
  -H "content-type: application/json" \
  -H "anthropic-version: 2023-06-01" \
  -d '{
    "model": "apollia/demand-clerk",
    "max_tokens": 1024,
    "system": "Réponds en une ligne.",
    "messages": [{"role": "user", "content": "Qualifie la demande de Durand."}]
  }'
```

- **Les consignes sont un contexte, jamais la mission.** `system` et
  `instructions` ouvrent la conversation que lit l'agent, comme le fait un
  message `system` dans une chat completion. Elles ne remplacent pas la mission
  que l'agent a reçue dans Apollia.
- **Une réponse ne garde aucun état non plus.** `previous_response_id` et
  `conversation` sont refusés par un `400` lisible : envoyez les messages
  précédents dans `input`. `store` est accepté et ignoré, rien n'est gardé à
  relire.
- **Un flux suit les événements de son API.** Pour une réponse :
  `response.created`, puis pour chaque étape un élément de raisonnement
  (`response.output_item.added`, `response.reasoning_summary_text.delta`,
  `response.output_item.done`), puis l'élément message avec la réponse dans
  `response.output_text.delta`, puis `response.completed` qui porte l'usage.
  Pour un message : `message_start`, puis pour chaque étape un bloc `thinking`
  (`content_block_start`, un `thinking_delta`, `content_block_stop`), puis le
  bloc `text` avec la réponse dans un `text_delta`, puis `message_delta` qui
  porte `stop_reason` et l'usage, et `message_stop`.
- **Un bloc `thinking` a une `signature` vide.** Les étapes sont le compte
  rendu que fait Apollia du travail, pas le raisonnement signé d'un modèle.
- **Les jetons ne se comptent pas d'avance.** `/v1/messages/count_tokens` ne
  sert que les modèles locaux ; sur un modèle `apollia/...`, il répond `404`.

### Une décision que l'agent attend

Un agent qui recourt à un accès que personne n'a accordé, comme écrire un
fichier sans « toujours » répondu pour cela, ne l'exécute pas et n'échoue pas.
Son exécution s'arrête et attend une décision, comme un élément de lot : elle
paraît dans les décisions en attente d'Apollia, avec sa notification, et
l'appel répond `409` avec le code `approval_pending` :

```json
{
  "error": {
    "message": "The agent waits for your approval to use an access nobody granted. ...",
    "type": "approval_pending",
    "code": "approval_pending",
    "pending_id": "batch:3f1c...:0",
    "batch_id": "3f1c...",
    "task_id": "9a7e...",
    "access": "file_write"
  }
}
```

Sur `/v1/messages`, les mêmes champs sont dans l'objet d'erreur de l'enveloppe
Anthropic, dont le `type` est `approval_pending` :

```json
{
  "type": "error",
  "error": {
    "type": "approval_pending",
    "message": "The agent waits for your approval to use an access nobody granted. ...",
    "pending_id": "batch:3f1c...:0",
    "batch_id": "3f1c...",
    "task_id": "9a7e...",
    "access": "file_write"
  }
}
```

Dans un flux, le même objet d'erreur est le dernier événement : tel quel pour
une chat completion, en événement `error` pour un message, et pour une réponse
en événement `error` dont les champs (`code`, `message`, `pending_id` et les
autres) sont à côté de son `type`. Terminer la
réponse ne retire pas la décision : elle reste en attente jusqu'à ce que
quelqu'un y réponde, dans Apollia ou par l'API native avec
`POST /api/v1/pending-decisions/decide` et le `pending_id`. Une fois autorisée,
l'exécution reprend ; son résultat se lit sur la tâche
(`GET /api/v1/tasks/{task_id}`) ou sur le lot (`GET /api/v1/batches/{batch_id}`).

## Serveurs MCP apportés par une requête

Une réponse ou un message peut apporter son propre serveur MCP, pour essayer
un outil avec un modèle sans ajouter le serveur comme connexion. Le serveur
compatible lit les deux formes publiques : l'outil `mcp` de l'API Responses et
les `mcp_servers` de l'API Messages.

```toml
[server]
allow_request_mcp = false   # une requête peut-elle apporter un serveur MCP
```

Le réglage est éteint par défaut, et lu à chaque requête. Tant qu'il est
éteint, une requête qui apporte un serveur répond `400` : « MCP servers brought
by a request are turned off. »

```bash
curl http://127.0.0.1:7772/v1/responses \
  -H "content-type: application/json" \
  -d '{
    "model": "acme/chat",
    "input": "List the open tickets.",
    "tools": [{
      "type": "mcp",
      "server_label": "tickets",
      "server_url": "http://localhost:3000/mcp",
      "authorization": "the-token-of-that-server"
    }]
  }'
```

| | `POST /v1/responses` | `POST /v1/messages` |
|---|---|---|
| Où le serveur est nommé | Une entrée de `tools` dont le `type` est `mcp` | Une entrée de `mcp_servers` dont le `type` est `url` |
| Son adresse | `server_url` | `url` |
| Son secret | `authorization`, envoyé en bearer ; `headers`, un objet de chaînes | `authorization_token`, envoyé en bearer |
| Les outils gardés | `allowed_tools`, une liste de noms ou `{"tool_names": [...]}` | `tool_configuration.allowed_tools` ; `tool_configuration.enabled: false` écarte le serveur |
| Son libellé | `server_label`, lu et jamais pris pour nom | `name`, lu et jamais pris pour nom |

Réglage allumé, une telle requête n'est pas relayée au moteur du modèle.
Apollia se connecte au serveur, propose ses outils au modèle, exécute chaque
outil que le modèle appelle et répond quand le modèle tient sa réponse finale,
au format du point d'accès : la réponse et les étapes sont là où les met un
[agent appelé comme un modèle](#responses-et-messages). Ce qui la borne :

- **Un modèle seulement.** Sur un modèle `apollia/...`, la requête répond
  `400` : un agent ou un workflow travaille avec les connexions qu'il a reçues
  dans Apollia.
- **HTTP seulement.** Le serveur est joint en streamable HTTP, à une adresse
  `http` ou `https`. Une requête ne lance jamais de processus : un serveur
  `stdio`, un `command`, ou une adresse d'un autre schéma répond `400`. Un
  `connector_id` n'est pas servi.
- **Ce poste ou un hôte public.** L'adresse est `localhost`, une adresse de
  boucle locale, ou un hôte public. Un serveur du réseau privé, une adresse de
  lien local ou un domaine interne répond `400`, et une redirection vers l'un
  d'eux aussi : un serveur public ne peut pas envoyer la connexion vers une
  adresse interne, et un serveur de ce poste ne peut pas rediriger du tout.
- **La politique de l'organisation l'admet.** Chaque serveur passe l'admission
  que passe une connexion quand quelqu'un l'ajoute. Avec
  `[connections] remote = "deny"` dans la politique, un serveur qui n'est pas
  sur ce poste répond `403` avec la raison de la politique, avant toute
  connexion.
- **La connexion dure le temps de l'appel.** Elle est ouverte pour la requête
  et retirée quand la réponse se termine, et quand le client s'en va avant la
  fin. Rien n'est écrit dans les connexions d'Apollia.
- **Ses outils sont non classés.** Quoi que le serveur déclare d'un outil,
  Apollia le tient pour sensible, et personne ne surveille l'appel pour
  l'approuver. Le premier outil que le modèle appelle termine l'appel sur
  `409 approval_pending`, qui nomme l'outil dans `access`, sauf si une règle
  posée dans le mode développeur d'Apollia autorise cet outil. Rien n'attend
  de décision ensuite, puisque la connexion a disparu : posez la règle, puis
  renvoyez la requête.
- **Aucun autre outil.** Les appels d'outils sont exécutés par Apollia, donc
  la requête ne peut pas apporter d'outils de fonction à côté du serveur ; elle
  répond `400`.
- **Un serveur par hôte, quatre au plus.** Un serveur porte le nom de l'hôte
  de son adresse, `request-mcp:<hôte>`, et ses outils
  `mcp:request-mcp:<hôte>/<outil>` : c'est le nom que lisent une règle et le
  journal. Deux serveurs sur un même hôte dans une requête répondent `400`.
- **Le réseau local exige un jeton**, comme l'appel d'un agent, quoi que dise
  `require_auth`.
- **Les champs d'échantillonnage ne sont pas appliqués.** L'exécution prend
  les réglages que le modèle a dans Apollia ; `temperature`, `max_tokens` et
  les champs semblables sont lus et laissés de côté. Les instructions
  (`instructions`, `system`) ouvrent la conversation.

| Statut | `code` OpenAI | `type` Anthropic | Quand |
|---|---|---|---|
| `400` | `invalid_request` | `invalid_request_error` | Le réglage est éteint, le modèle est un agent ou un workflow, le serveur demande un processus, son adresse n'est pas permise, ou la requête apporte d'autres outils |
| `401` | `invalid_api_key` | `authentication_error` | L'appel vient du réseau local sans jeton en service |
| `403` | `refused_by_policy` | `permission_error` | La politique de l'organisation refuse le serveur |
| `409` | `approval_pending` | `approval_pending` | Le modèle a appelé un outil qu'aucune règle n'autorise ; `access` le nomme |
| `502` | `mcp_server_unavailable` | `api_error` | Le serveur n'a pas répondu à la poignée de main |
| `503` | `run_unavailable` | `api_error` | Ce runtime ne peut pas exécuter la requête, le modèle n'est pas encore de ceux sur lesquels Apollia exécute des agents, ou une autre requête utilise un serveur du même hôte |

## Points d'accès

La table liste chaque chemin que sert le serveur compatible.

<!-- compat-endpoints -->

| Méthode | Chemin | Format |
|---|---|---|
| `GET` | `/v1/models` | OpenAI. Les modèles chargés, en liste : `id` (identifiant du registre), `object`, `created`, `owned_by`, et `type`, `llm` ou `embeddings` ; puis les agents et workflows disponibles pour d'autres applications, `apollia/<id>`, de `type` `agent` ou `workflow` |
| `POST` | `/v1/chat/completions` | OpenAI. Relayé au moteur du modèle ; sur un modèle `apollia/...`, exécuté par Apollia (agents et workflows ci-dessus) |
| `POST` | `/v1/responses` | OpenAI. Relayé au moteur du modèle ; sur un modèle `apollia/...`, ou avec un outil `mcp`, exécuté par Apollia |
| `POST` | `/v1/embeddings` | OpenAI. Relayé au modèle de recherche par le sens |
| `POST` | `/v1/messages` | Anthropic. Relayé au moteur du modèle ; sur un modèle `apollia/...`, ou avec `mcp_servers`, exécuté par Apollia |
| `POST` | `/v1/messages/count_tokens` | Anthropic. Relayé au moteur du modèle (`llama-server` seulement) |

Mesuré sur `llama-server` à la version qu'épingle Apollia (`b11541`) : les
quatre points d'accès de conversation répondent `200` sur un modèle de
conversation, et `/v1/embeddings` ne répond que sur l'instance lancée pour la
recherche par le sens.

### Pas la boîte aux lettres des agents

`/v1/messages` est ici l'API Messages d'Anthropic : un tour de conversation
avec un modèle. Elle n'a rien à voir avec `/api/v1/agents/{name}/messages` de
l'API native, la boîte aux lettres par laquelle les agents s'envoient des
messages. Les deux partagent un mot et rien d'autre.
