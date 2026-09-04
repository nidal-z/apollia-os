---
sidebar_position: 1
title: Piloter Apollia depuis votre produit
description: Démarrer un vrai démon Apollia, s'y authentifier et piloter une tâche depuis votre propre code via l'API HTTP, du premier appel au résultat.
---

# Piloter Apollia depuis votre produit

Dans ce tutoriel, vous démarrez un vrai daemon Apollia, vous vous authentifiez
auprès de lui, et vous le pilotez depuis un programme hôte via le client
Python généré. À la fin, vous aurez soumis une tâche à un agent et lu son
résultat en retour, entièrement via l'API HTTP locale.

C'est le chemin d'intégration : votre produit dialogue avec un runtime
Apollia de la même manière que n'importe quelle application hôte, sans
embarquer de Rust ni faire de rétro-ingénierie du format d'échange.

## Ce que vous allez construire

Un petit script Python qui :

1. lit le jeton API du daemon,
2. ouvre un client authentifié,
3. soumet une tâche à un agent,
4. interroge en boucle jusqu'à ce que la tâche se termine, puis affiche le
   résultat.

Vous utiliserez un agent `echo` sans LLM afin que ce tutoriel fonctionne sur
n'importe quelle machine, sans téléchargement de modèle requis.

## Avant de commencer

Il vous faut un clone du dépôt Apollia, une chaîne d'outils Rust pour
compiler le daemon, et Python 3.13, la version que le clone épingle dans
`.python-version` et celle qu'utilise chaque travail qui compile le runtime.
Chaque commande ci-dessous s'exécute depuis la racine du dépôt.

## Étape 1 : compiler le daemon

```sh
cargo build -p apollia-cli
```

Cela produit le binaire `apollia-os` dans `target/debug/apollia-os`. Par
souci de concision, le reste du tutoriel l'appelle simplement `apollia-os` ;
utilisez le chemin complet s'il n'est pas dans votre `PATH`.

## Étape 2 : installer l'agent echo

Le dépôt fournit un agent minimal qui renvoie son entrée telle quelle.
Installez-le pour que le daemon puisse le charger :

```sh
apollia-os agent install clients/examples/echo_agent.py --skip-tests
```

## Étape 3 : démarrer le daemon

<!-- claim:daemon-binds-tcp-by-default -->
Démarrez le runtime. Il écoute sur un socket Unix et sur `127.0.0.1:7771`. Le
port TCP est toujours lié ; `--port` choisit le numéro :

```sh
apollia-os start --port 7771
```

Au premier démarrage, le daemon génère un jeton API et l'écrit dans
`~/.apollia/api-token` (lisible par vous seul). Les appelants TCP doivent
présenter ce jeton comme identifiant bearer. Le socket Unix repose sur une
confiance locale et n'en a besoin d'aucun.

Laissez ce processus tourner et ouvrez un second terminal pour les étapes
suivantes.

## Étape 4 : configurer le client Python

Le client généré se trouve dans `clients/python`. Ses dépendances
d'exécution sont `httpx`, `attrs` et `python-dateutil` :

```sh
python3 -m venv clients/.venv
clients/.venv/bin/pip install httpx attrs python-dateutil
```

## Étape 5 : piloter le daemon

Enregistrez ceci sous `drive.py`. Le script lit le jeton, ouvre un client
authentifié, soumet une tâche à l'agent `echo`, puis interroge en boucle
jusqu'au résultat :

```python
import pathlib
import sys
import time

# Rend le client généré importable.
sys.path.insert(0, "clients/python")

from apollia_runtime_client import AuthenticatedClient
from apollia_runtime_client.api.health import health_handler
from apollia_runtime_client.api.tasks import get_task, submit_task
from apollia_runtime_client.models import SubmitTaskRequest, SubmitTaskRequestInput

TOKEN = (pathlib.Path.home() / ".apollia" / "api-token").read_text().strip()
TERMINAL = {"completed", "succeeded", "done", "failed", "error", "cancelled"}

client = AuthenticatedClient(base_url="http://127.0.0.1:7771", token=TOKEN)

health = health_handler.sync(client=client)
print("health:", health.status)

submitted = submit_task.sync(
    client=client,
    body=SubmitTaskRequest(
        agent_id="echo",
        input_=SubmitTaskRequestInput.from_dict(
            {"parts": [{"type": "text", "text": "hello from the host SDK"}]}
        ),
    ),
)
print("submitted:", submitted.task_id, submitted.status)

task = submitted
for _ in range(120):
    task = get_task.sync(id=submitted.task_id, client=client)
    if task.status.lower() in TERMINAL or task.result:
        break
    time.sleep(0.5)

print("status:", task.status)
print("result:", task.result)
```

Exécutez-le avec l'interpréteur de l'environnement virtuel :

```sh
clients/.venv/bin/python drive.py
```

Vous verrez le contrôle de santé réussir, la tâche soumise avec un
identifiant, puis le résultat renvoyé par l'écho. C'est une application
hôte qui pilote un vrai runtime Apollia de bout en bout.

## Ce qui vient de se passer

Chaque appel est passé par le client généré, lui-même produit à partir de la
spécification OpenAPI du runtime. Rien n'a été écrit à la main contre le
format d'échange, si bien que le client ne peut pas dériver du contrat. Vous
vous êtes authentifié via TCP avec un jeton bearer, vous avez soumis du
travail à un agent, et vous en avez récupéré le résultat : c'est exactement
la forme que prend une intégration produit.

## Nettoyage

Arrêtez le daemon dans le premier terminal, ou exécutez :

```sh
apollia-os stop
```

## Pour aller plus loin

- Pour intégrer cela dans un vrai produit, lisez
  [Intégrer Apollia via le contrat de pilotage](/how-to/integrate-via-driving-contract).
- Pour chaque endpoint, requête et réponse, consultez la
  [référence de l'API HTTP](/reference/api/apollia-os-runtime-api).
- Une version exécutable de ce flux se trouve dans
  `clients/examples/demo_python.sh`, qui compile, démarre, pilote et arrête
  le daemon en une seule commande.
