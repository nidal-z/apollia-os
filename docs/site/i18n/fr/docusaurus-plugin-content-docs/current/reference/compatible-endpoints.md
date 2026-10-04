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

Le serveur n'écoute qu'une fois que quelqu'un l'allume. Quatre clés
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
| `GET /api/v1/server` | S'il écoute, son port, s'il sert le réseau local, l'adresse à copier, pourquoi son dernier démarrage a échoué, et combien de jetons sont actifs |
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

## Les agents et workflows comme modèles

Un agent ou un workflow que l'utilisateur a rendu disponible pour d'autres
applications, le même interrupteur que lit le pont MCP, est servi lui aussi
comme un modèle. `GET /v1/models` le liste après les modèles locaux, sous
`apollia/<id>` avec le `type` `agent` ou `workflow` : un agent par son
identifiant (`apollia/demand-clerk`), un workflow par son paquet et son nom
(`apollia/office/close-month`). Un identifiant d'agent ne porte jamais de `/`,
si bien qu'un agent et un workflow ne répondent jamais au même nom.

Un client l'appelle par `POST /v1/chat/completions`, comme il appellerait un
modèle :

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

Dans un flux, le même objet d'erreur est le dernier événement. Terminer la
réponse ne retire pas la décision : elle reste en attente jusqu'à ce que
quelqu'un y réponde, dans Apollia ou par l'API native avec
`POST /api/v1/pending-decisions/decide` et le `pending_id`. Une fois autorisée,
l'exécution reprend ; son résultat se lit sur la tâche
(`GET /api/v1/tasks/{task_id}`) ou sur le lot (`GET /api/v1/batches/{batch_id}`).

## Points d'accès

La table liste chaque chemin que sert le serveur compatible.

<!-- compat-endpoints -->

| Méthode | Chemin | Format |
|---|---|---|
| `GET` | `/v1/models` | OpenAI. Les modèles chargés, en liste : `id` (identifiant du registre), `object`, `created`, `owned_by`, et `type`, `llm` ou `embeddings` ; puis les agents et workflows disponibles pour d'autres applications, `apollia/<id>`, de `type` `agent` ou `workflow` |
| `POST` | `/v1/chat/completions` | OpenAI. Relayé au moteur du modèle ; sur un modèle `apollia/...`, exécuté par Apollia (agents et workflows ci-dessus) |
| `POST` | `/v1/responses` | OpenAI. Relayé au moteur du modèle |
| `POST` | `/v1/embeddings` | OpenAI. Relayé au modèle de recherche par le sens |
| `POST` | `/v1/messages` | Anthropic. Relayé au moteur du modèle |
| `POST` | `/v1/messages/count_tokens` | Anthropic. Relayé au moteur du modèle (`llama-server` seulement) |

Mesuré sur `llama-server` à la version qu'épingle Apollia (`b10092`) : les
quatre points d'accès de conversation répondent `200` sur un modèle de
conversation, et `/v1/embeddings` ne répond que sur l'instance lancée pour la
recherche par le sens.

### Pas la boîte aux lettres des agents

`/v1/messages` est ici l'API Messages d'Anthropic : un tour de conversation
avec un modèle. Elle n'a rien à voir avec `/api/v1/agents/{name}/messages` de
l'API native, la boîte aux lettres par laquelle les agents s'envoient des
messages. Les deux partagent un mot et rien d'autre.
