---
sidebar_position: 1
title: Intégrer Apollia via le contrat de pilotage
---

# Intégrer Apollia via le contrat de pilotage

Ce guide s'adresse aux équipes qui embarquent un runtime Apollia dans un
produit. Il couvre le contrat que votre application hôte pilote : l'API HTTP,
sa garantie de stabilité, l'authentification et les clients générés. Il
suppose que vous savez faire tourner un daemon Apollia et que vous voulez le
raccorder à une application.

Si vous voulez simplement voir le flux fonctionner une fois, commencez par
[Piloter Apollia depuis votre produit](/tutorials/drive-apollia-from-your-product).

## Le contrat

Un runtime Apollia expose ses capacités via une API HTTP sous `/api/v1` :
soumettre des tâches, ouvrir des sessions de chat, diffuser des résultats en
continu, consulter le journal d'audit et administrer le runtime. Le daemon
écoute sur un socket Unix et, quand on lui fournit un port, sur `127.0.0.1`.

L'API est décrite par une spécification OpenAPI 3.1 générée à partir du code
source du runtime : elle ne peut donc pas diverger du code. Le runtime la
sert à cette adresse :

```
GET /api/v1/openapi.json
```

Une copie versionnée existe aussi dans `clients/openapi.json`. Pour une vue
navigable de chaque opération, consultez la
[référence de l'API HTTP](/reference/api/apollia-os-runtime-api), générée à
partir de cette même spécification.

## Garantie de stabilité

`/api/v1` est un contrat versionné et stable. Les changements incompatibles
sont livrés sous une nouvelle version majeure (`/api/v2`) ; `v1` n'est jamais
modifiée de façon incompatible. Vous pouvez figer votre intégration sur `v1`
et vous y fier.

## Authentification

Choisissez la surface adaptée à votre déploiement :

- **Socket Unix** : confiance locale. L'accès est régi par les permissions du
  système de fichiers, aucun jeton n'est requis. À utiliser quand l'hôte et le
  runtime partagent la même machine et le même périmètre de confiance.
- **TCP sur `127.0.0.1`** : authentifié par jeton. Chaque requête doit porter
  l'en-tête `Authorization: Bearer <token>`. Au premier démarrage, quand
  `[api] require_token` est activé (c'est le comportement par défaut), le
  daemon génère un jeton et l'écrit dans `~/.apollia/api-token` avec des
  permissions réservées au propriétaire. Votre hôte lit ce fichier et
  transmet le jeton.

Quand le runtime est embarqué, il ne se lie par défaut qu'au socket Unix et
n'ouvre aucun port TCP sauf si vous en configurez un. S'il se lie tout de
même à TCP, le jeton y est exigé aussi.

## Utiliser un client généré

Deux clients de pilotage côté hôte sont générés à partir de la spécification
OpenAPI et vivent sous `clients/`. Les deux sont générés, pas écrits à la
main, ce qui les maintient synchronisés avec le contrat. Ne modifiez jamais un
fichier généré à la main : changez le runtime, puis régénérez.

| Client | Chemin | Chaîne d'outils |
|---|---|---|
| TypeScript | `clients/ts` | types `openapi-typescript` plus `openapi-fetch` |
| Python | `clients/python` | `openapi-python-client` |

### TypeScript

```ts
import { readFileSync } from "node:fs";
import { homedir } from "node:os";
import { createApolliaClient } from "@apollia/runtime-client";

const token = readFileSync(`${homedir()}/.apollia/api-token`, "utf8").trim();
const apollia = createApolliaClient({ token }); // baseUrl vaut 127.0.0.1:7771 par défaut

const health = await apollia.GET("/api/v1/health");

const submit = await apollia.POST("/api/v1/tasks", {
  body: {
    agent_id: "echo",
    input: { parts: [{ type: "text", text: "hello from the host" }] },
  },
});
```

Chaque opération est typée contre le contrat, si bien que la forme des
requêtes et des réponses est vérifiée à la compilation.

### Python

```python
import pathlib
from apollia_runtime_client import AuthenticatedClient
from apollia_runtime_client.api.tasks import submit_task, get_task
from apollia_runtime_client.models import SubmitTaskRequest, SubmitTaskRequestInput

token = (pathlib.Path.home() / ".apollia" / "api-token").read_text().strip()
client = AuthenticatedClient(base_url="http://127.0.0.1:7771", token=token)

resp = submit_task.sync(client=client, body=SubmitTaskRequest(
    agent_id="echo",
    input_=SubmitTaskRequestInput.from_dict(
        {"parts": [{"type": "text", "text": "hello"}]}
    ),
))
task = get_task.sync(id=resp.task_id, client=client)
```

## Diffuser les résultats en continu

Au-delà du modèle soumettre-puis-interroger, le contrat expose des flux
d'événements serveur (server-sent events) pour les traitements longs : la
sortie d'une tâche à `GET /api/v1/tasks/{id}/stream` et la sortie d'une
session de chat à `GET /api/v1/sessions/{id}/stream`. Consultez la
[référence de l'API HTTP](/reference/api/apollia-os-runtime-api) pour la
forme de leurs événements.

## Régénérer les clients

Quand le contrat du runtime change, actualisez les clients à partir de la
spécification :

```sh
# À partir de la spécification versionnée :
bash clients/regen.sh

# Ou actualisez d'abord la spécification depuis un daemon en cours d'exécution :
bash clients/regen.sh --from-daemon
```

## Limitation connue

Trois endpoints acceptent un corps de requête brut, sans schéma JSON, et ne
sont donc pas exposés comme des méthodes de client typées :
`PUT /api/v1/stt/config`, `POST /api/v1/stt/transcribe` et
`POST /webhooks/{id}`. Ils restent présents dans la spécification et peuvent
être appelés directement avec un client HTTP classique si besoin.

## Voir aussi

- [Intégrer Apollia par fédération (MCP + REST)](/how-to/embed-via-federation)
  pour le schéma d'intégration en sidecar.
- [Auditer et vérifier une exécution](/how-to/audit-and-verify) pour le
  flux de responsabilisation autour de ce que votre intégration exécute.
- La [référence de l'API HTTP](/reference/api/apollia-os-runtime-api), la
  [référence CLI](/reference/cli) et la [référence SDK](/reference/sdk).
