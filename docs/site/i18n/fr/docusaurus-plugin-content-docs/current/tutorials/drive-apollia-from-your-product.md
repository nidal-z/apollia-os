---
sidebar_position: 1
title: Piloter Apollia depuis votre produit
description: Démarrer un vrai démon Apollia, s'y authentifier et lancer un workflow depuis votre propre code via l'API HTTP, du premier appel au résultat.
---

# Piloter Apollia depuis votre produit

Dans ce tutoriel, vous démarrez un vrai daemon Apollia, vous vous authentifiez
auprès de lui, et vous le pilotez depuis un programme hôte via le client
Python généré. À la fin, vous aurez lancé un workflow et lu son résultat en
retour, entièrement via l'API HTTP locale.

C'est le chemin d'intégration : votre produit dialogue avec un runtime
Apollia de la même manière que n'importe quelle application hôte, sans
embarquer de Rust ni faire de rétro-ingénierie du format d'échange.

## Ce que vous allez construire

Un petit script Python qui :

1. lit le jeton API du daemon,
2. ouvre un client authentifié,
3. lance un workflow,
4. interroge en boucle jusqu'à la fin de l'exécution, puis affiche le
   résultat.

Vous utiliserez le paquet `echo`, dont l'unique workflow renvoie le message
qu'il reçoit. Un workflow exécute ses étapes telles qu'elles sont écrites : le
tutoriel n'a donc besoin d'aucun modèle et fonctionne sur n'importe quelle
machine.

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

## Étape 2 : construire le paquet echo

Le dépôt fournit un paquet minimal dans `clients/examples/echo` : un
`apollia.toml` et un workflow, `echo`, déclaré avec `@workflow` dans
`workflows/echo.py`. Il ne déclare aucun agent. Construisez son archive :

```sh
apollia-os package build clients/examples/echo
```

La commande écrit `echo-0.1.0.apo` dans le dossier courant. Aucun code Python
ne s'exécute : la construction lit le workflow dans le descripteur que
`python -m apollia describe clients/examples/echo` a écrit dans `apollia.toml`.

## Étape 3 : démarrer le daemon

<!-- claim:daemon-binds-tcp-by-default -->
Démarrez le runtime. Il écoute sur un socket Unix (un tube nommé sous Windows,
qui n'en a pas) et sur `127.0.0.1:7771`. Le port TCP est toujours lié ;
`--port` choisit le numéro :

```sh
apollia-os start --port 7771
```

Au premier démarrage, le daemon génère un jeton API et l'écrit dans
`~/.apollia/api-token` (lisible par vous seul). Les appelants TCP doivent
présenter ce jeton comme identifiant bearer. Le socket Unix repose sur une
confiance locale et n'en a besoin d'aucun.

Laissez ce processus tourner et ouvrez un second terminal pour les étapes
suivantes.

## Étape 4 : installer le paquet

Dans le second terminal, installez l'archive dans le daemon en cours :

```sh
apollia-os package install echo-0.1.0.apo
```

Un paquet non signé s'installe avec un avertissement. Vérifiez que le workflow
est présent :

```sh
apollia-os workflow show echo/echo
```

La commande affiche l'unique étape et l'entrée `message`.

## Étape 5 : configurer le client Python

Le client généré se trouve dans `clients/python`. Ses dépendances
d'exécution sont `httpx`, `attrs` et `python-dateutil` :

```sh
python3 -m venv clients/.venv
clients/.venv/bin/pip install httpx attrs python-dateutil
```

## Étape 6 : piloter le daemon

Enregistrez ceci sous `drive.py`. Le script lit le jeton, ouvre un client
authentifié, lance le workflow `echo` du paquet `echo`, puis interroge
l'exécution en boucle jusqu'à sa fin :

```python
import pathlib
import sys
import time

# Rend le client généré importable.
sys.path.insert(0, "clients/python")

from apollia_runtime_client import AuthenticatedClient
from apollia_runtime_client.api.health import health_handler
from apollia_runtime_client.api.workflows import get_workflow_run, launch_workflow
from apollia_runtime_client.models import WorkflowLaunch, WorkflowLaunchInputs

TOKEN = (pathlib.Path.home() / ".apollia" / "api-token").read_text().strip()

client = AuthenticatedClient(base_url="http://127.0.0.1:7771", token=TOKEN)

health = health_handler.sync(client=client)
print("health:", health.status)

launched = launch_workflow.sync(
    "echo",
    "echo",
    client=client,
    body=WorkflowLaunch(
        inputs=WorkflowLaunchInputs.from_dict({"message": "hello from the host SDK"})
    ),
)
print("launched:", launched.run_id, launched.status)

run = launched
for _ in range(120):
    run = get_workflow_run.sync(launched.run_id, client=client).run
    if run.status != "running":
        break
    time.sleep(0.5)

print("status:", run.status)
print("result:", run.output.to_dict())
```

Exécutez-le avec l'interpréteur de l'environnement virtuel :

```sh
clients/.venv/bin/python drive.py
```

Vous verrez le contrôle de santé réussir, l'exécution lancée avec un
identifiant, puis le résultat `{'echo': 'hello from the host SDK'}` lu en
retour. C'est une application hôte qui pilote un vrai runtime Apollia de bout
en bout.

## Ce qui vient de se passer

Chaque appel est passé par le client généré, lui-même produit à partir de la
spécification OpenAPI du runtime. Rien n'a été écrit à la main contre le
format d'échange, si bien que le client ne peut pas dériver du contrat. Vous
vous êtes authentifié via TCP avec un jeton bearer, vous avez lancé un
workflow, et vous en avez récupéré le résultat : c'est exactement la forme
que prend une intégration produit.

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
  `clients/examples/demo_python.sh`, qui compile, installe, démarre, pilote
  et arrête le daemon en une seule commande. Le script refuse de tourner tant
  qu'un autre runtime est en marche : arrêtez d'abord le vôtre.
