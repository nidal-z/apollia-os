---
sidebar_position: 10
title: Points d'accès compatibles
description: "Le serveur compatible : un second écouteur du runtime, sur son propre port, que les autres applications appellent aux formats d'OpenAI et d'Anthropic. Éteint par défaut."
---

# Points d'accès compatibles

Le runtime peut ouvrir un second écouteur, le **serveur compatible**, pour les
autres applications de la machine. Elles l'appellent comme elles appelleraient
OpenAI ou Anthropic : les chemins commencent par `/v1/`, les corps suivent ces
API et leurs noms de champs (`snake_case`), et une erreur revient dans
l'enveloppe d'OpenAI.

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
serve_on_lan = false   # les autres appareils du réseau local l'atteignent-ils
```

Par défaut, il écoute sur l'interface de boucle locale, `127.0.0.1`, si bien que
seules les applications de cette machine l'atteignent.

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

## Erreurs

Toute erreur suit l'enveloppe d'OpenAI, sauf un `401` sur le point d'accès
d'Anthropic (`/v1/messages`), qui suit celle d'Anthropic
(`{"type": "error", "error": {"type": "authentication_error", "message"}}`) :

```json
{
  "error": {
    "message": "Unknown endpoint: /v1/unknown",
    "type": "invalid_request_error",
    "code": "unknown_url"
  }
}
```

Un chemin que le serveur ne sert pas répond `404` avec ce corps.

## Points d'accès

La table liste chaque chemin que sert le serveur compatible. Elle est vide
aujourd'hui : l'écouteur est en place et ne sert encore aucun point d'accès, si
bien que tout chemin répond le `404` ci-dessus.

<!-- compat-endpoints -->

| Méthode | Chemin | Format |
|---|---|---|
