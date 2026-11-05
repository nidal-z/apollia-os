---
sidebar_position: 8.5
title: Activer CUDA sur un serveur
---

# Activer CUDA sur un serveur

Ce guide donne aux modèles GGUF d'un serveur Linux distant une carte NVIDIA
sur laquelle tourner : il vérifie la carte et son pilote, installe le build
CUDA de `llama-server` depuis le canal des moteurs, en fait le moteur en
service, et prouve que la carte est utilisée. Un second chemin fait de même sur
un serveur sans réseau. Il suppose que le daemon tourne déjà comme service,
comme dans [Déployer en production](/how-to/deploy-in-production), sous
l'utilisateur `apollia` dont le dossier personnel est `/var/lib/apollia`.

Les commandes `engine` parlent au daemon par son socket, qui se trouve dans le
dossier de données de l'utilisateur qui le lance. Lancez-les sous cet
utilisateur, avec son dossier personnel :

```sh
sudo -u apollia -H apollia-os engine list
```

## Ce que la carte et le pilote demandent

Le canal des moteurs porte deux builds CUDA, `cuda-12` et `cuda-13`, à côté de
`rocm`, `vulkan` et `cpu`. Chaque paquet CUDA porte les bibliothèques
d'exécution CUDA que le llama.cpp amont publie pour son build, posées à côté du
serveur, sous Linux comme sous Windows : le serveur n'a besoin que du pilote
NVIDIA de la machine, d'aucun toolkit CUDA.

Le runtime classe chaque variante pour la machine, `recommended`, `compatible`
ou `incompatible`, d'après les cartes qu'il détecte et leur pilote, et en
recommande une au plus :

| Variante | Carte | Pilote pour convenir pleinement | Pilote pour tourner |
|---|---|---|---|
| `cuda-13` | capacité de calcul 7.5 ou plus | R615 ou plus | R580 ou plus |
| `cuda-12` | toute carte NVIDIA, Maxwell, Pascal et Volta comprises | 570.26 ou plus sous Linux, 551.61 ou plus sous Windows | 525 sous Linux, 527.41 sous Windows, sur une carte dont le build porte le code natif |

Quand les deux builds CUDA conviennent pleinement, c'est `cuda-13` qui est
recommandé. Quand aucune variante ne convient pleinement, la plus capable qui
tourne est recommandée quand même, avec sa réserve, le pilote à mettre à jour
par exemple. En détail :

- **Les cartes Maxwell, Pascal et Volta ne tournent qu'en CUDA 12.** CUDA 13 a
  retiré leurs architectures, et le runtime classe `cuda-13` incompatible sur
  une carte de capacité de calcul inférieure à 7.5, avec cette raison.
- **Un pilote sous le seuil est une raison, pas une supposition.** Sous R580,
  `cuda-13` est incompatible ; entre R580 et R615 il est compatible, et
  `cuda-12` est recommandé quand son propre seuil de pilote est atteint. Sous
  ce seuil, `cuda-12` reste compatible jusqu'au plancher de la famille CUDA 12
  sur une carte dont le build porte le code natif (capacité de calcul 8.6 à
  8.9, et 12.0 sous Linux), et il est incompatible sur toute autre carte, que
  le build sert par PTX et qui demande le pilote du toolkit.
- **CUDA 12 sur les cartes les plus récentes.** Sur une carte de capacité de
  calcul 10.0 ou plus dont le build ne porte pas le code natif (toutes sous
  Windows, toutes sauf 12.0 sous Linux), `cuda-12` tourne mais ne convient
  jamais pleinement. Il n'est recommandé que si aucune autre variante ne
  convient pleinement : une carte de capacité de calcul 12.0 sous Windows avec
  le pilote 575 reçoit `cuda-12` recommandé, puisque `cuda-13` demande R580.

Quand la machine a plusieurs cartes NVIDIA, les builds CUDA sont classés pour
celle dont la capacité de calcul est la plus haute, puis la mémoire la plus
grande.

## Vérifier ce que le runtime voit

```sh
sudo -u apollia -H apollia-os engine check
sudo -u apollia -H apollia-os engine list
```

`engine check` sonde de nouveau les cartes et lit le canal suivi, `stable` sauf
si `engine channel` dit autre chose. Le runtime lit le canal sur
`https://registry.apollia.fr/engines-<canal>/`, puis, quand le registre ne
répond pas, sur la release GitHub `engines-<canal>` du projet, qui en est le
miroir. Aucune des deux adresses n'est de confiance : le manifeste
`engines.json` n'est accepté que si sa signature `engines.json.minisig` tient
face à la clé de distribution compilée dans le binaire, et un build qui ne
porte pas cette clé refuse tout moteur. Un canal hors d'atteinte ou une
signature refusée termine `engine check` avec le code 2 et la raison.

`engine list` affiche quatre blocs : les cartes détectées (nom, pilote,
mémoire, capacité de calcul), les moteurs installés avec celui en service, les
variantes que le canal propose pour cette plateforme avec leur verdict et leur
raison, et Node.js. Les variantes qui ne tournent pas ici sont comptées plutôt
que listées ; `engine list --all` les liste avec leur raison, par exemple
`driver 560 < 580` ou `compute capability 6.1 is not built by CUDA 13`.

## Installer le moteur CUDA

```sh
sudo -u apollia -H apollia-os engine install llama-server
```

Sans variante, `engine install` prend celle que le classement recommande pour
cette machine. Nommez-la pour en choisir une autre,
`engine install llama-server cuda-12` ; une variante classée incompatible est
refusée sauf avec `--force`. La commande suit l'installation jusqu'à sa fin :
la signature du manifeste est vérifiée avant que la moindre archive soit
demandée, et une archive dont le SHA-256 diffère du manifeste est supprimée.
`--detach` rend la main dès que le runtime a accepté l'installation, et
`engine list` la suit ensuite.

## En faire le moteur en service

Le moteur des modèles GGUF est le réglage `engines.gguf`, `auto` par défaut :
le choix automatique prend le build installé classé recommandé, sinon le
meilleur compatible, sinon le moteur livré avec le binaire. Pour fixer le
build CUDA à la place :

```sh
sudo -u apollia -H apollia-os engine use cuda-13
```

`engine use` accepte `auto`, `embedded`, ou une variante installée ; `cuda`
désigne le meilleur build CUDA installé. Une variante non installée est
refusée, une variante installée classée incompatible est écrite avec un
avertissement. Le choix s'applique au prochain chargement d'un modèle, sans
redémarrer le service. Une variante choisie puis retirée retombe sur le choix
automatique.

## Vérifier que la carte est utilisée

```sh
sudo -u apollia -H apollia-os engine list
```

Le build en service porte `in service` dans le bloc Installed. Le runtime lance
aussi `--list-devices` sur chaque build GPU installé : un build qui ne liste
aucun périphérique est classé incompatible, avec la raison
`the installed build sees no device`. Pour lire cette liste vous-même, lancez
le binaire du build, qui se trouve sous le dossier de données de l'utilisateur
du service :

```sh
sudo -u apollia -H find /var/lib/apollia/.apollia/engines/llama-server -name llama-server -type f
sudo -u apollia -H <chemin affiché ci-dessus> --list-devices
```

Un build CUDA qui voit la carte affiche une ligne par périphérique, de la forme
`CUDA0: <nom> (<total> MiB, <libre> MiB free)`. Chargez ensuite un modèle et
lisez le journal du daemon : `llama.server.device` nomme le périphérique sur
lequel le moteur s'est chargé, et `llama.server.offload` les couches qui y sont
placées face au total du modèle.

## Sans réseau

Un serveur qui n'atteint aucune des deux adresses installe les mêmes paquets
depuis un dossier, par les mêmes contrôles. Sur une machine qui a le réseau,
copiez depuis la release `engines-stable` du projet (ou `engines-preview`, pour
le canal preview) dans un même dossier :

- le manifeste, `engines.json` ;
- sa signature, `engines.json.minisig` ;
- l'archive de chaque paquet à installer, nommée
  `<engine>-<tag>-<variant>-<platform>.zip`, par exemple l'archive
  `llama-server` de `cuda-13` pour `linux-x64`.

Le manifeste liste chaque paquet avec son moteur, son tag, sa variante, sa
plateforme, son adresse, sa taille et son SHA-256 : l'archive qui convient au
serveur est celle dont il nomme la `platform` et la `variant`. Apportez le
dossier sur le serveur, là où l'utilisateur du service peut le lire, puis :

```sh
sudo -u apollia -H apollia-os engine import /srv/engines-stable
```

Le daemon vérifie la signature avant de lire la moindre archive, refuse un
manifeste plus ancien que le dernier accepté sur ce canal, et installe chaque
paquet du manifeste pour cette plateforme dont l'archive est dans le dossier,
après avoir vérifié son empreinte. Un dossier sans manifeste, à la signature
refusée, ou dont une archive ne correspond pas est refusé avec le code 2.
`engine use` et la vérification ci-dessus s'appliquent ensuite sans
changement.

## Une carte AMD sous Linux

La variante `rocm` sert les cartes AMD dont la cible gfx est dans la liste du
build amont. Sous Linux, elle ne porte pas `libamdhip64` : elle charge le
runtime ROCm de l'hôte, si bien que ROCm doit être installé sur le serveur pour
qu'elle démarre. Le runtime cherche `libamdhip64` là où cherche le chargeur
dynamique (le dossier d'installation de ROCm, les dossiers que nomme
`ld.so.conf`, `LD_LIBRARY_PATH`, les dossiers de bibliothèques par défaut).
Sans elle, `rocm` est classée incompatible avec la raison
`no ROCm runtime on this machine`, et Vulkan est la variante recommandée pour
la carte, que `engine install llama-server` prend alors.

Après l'installation de ROCm sur l'hôte, `engine check` sonde de nouveau la
machine, et `engine list` affiche le nouveau verdict ; `engine list --all`
liste `rocm` avec sa raison tant qu'elle reste incompatible.

## Voir aussi

- [Tirer le meilleur parti de l'inférence locale](/how-to/accelerate-local-inference)
  pour les moteurs que les installeurs embarquent et les variables qui règlent
  le moteur.
- [Déployer en production](/how-to/deploy-in-production) pour le service que
  ce guide prolonge.
- La [référence CLI](/reference/cli) pour chaque commande `engine` et ses codes
  de sortie.
