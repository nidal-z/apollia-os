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

Le serveur n'écoute qu'une fois que quelqu'un l'allume. Trois clés
d'`apollia.toml` portent son état :

```toml
[server]
enabled = false   # le serveur compatible écoute-t-il
port = 7772       # son port TCP, sur 127.0.0.1 ; jamais le 7771 de l'API native
cors = false      # une page web d'une autre origine peut-elle l'appeler
```

Il écoute sur l'interface de boucle locale, `127.0.0.1`, si bien que seules les
applications de cette machine l'atteignent.

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

Trois routes de l'API native le pilotent pendant que le runtime tourne :

| Route | Effet |
|---|---|
| `GET /api/v1/server` | S'il écoute, son port, et pourquoi son dernier démarrage a échoué |
| `POST /api/v1/server/start` | L'ouvre sur le port configuré et écrit `enabled = true` |
| `POST /api/v1/server/stop` | Le ferme et écrit `enabled = false` ; l'API native continue de répondre |

Comme démarrer et arrêter écrivent `enabled`, l'état survit à une relance : un
runtime qui démarre avec `enabled = true` rouvre le serveur compatible. Si un
autre programme tient déjà le port, le démarrage est refusé par une erreur
lisible « port occupé » (HTTP 409 sur la route de démarrage), et le runtime
continue sans le serveur compatible.

## Erreurs

Toute erreur suit l'enveloppe d'OpenAI, quel que soit le point d'accès :

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
