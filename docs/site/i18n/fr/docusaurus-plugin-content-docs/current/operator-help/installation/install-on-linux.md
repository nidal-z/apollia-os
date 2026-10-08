---
title: Installer Apollia Desktop sur Linux
slug: /operator-help/installation/install-on-linux
sidebar_position: 3
---

# Installer Apollia Desktop sur Linux

L'application s'appelle Apollia Desktop. Ses installeurs et les fichiers qu'ils
déposent sur le disque portent encore le nom du moteur qu'elle embarque,
Apollia OS : c'est le nom que vous lirez ci-dessous.

Chacun des fichiers ci-dessous est attaché à chaque publication sur la page de
publication, `https://github.com/Apollia-OS/apollia-os/releases`.

- **`.AppImage`** (recommandé, x86_64) : application desktop portable, aucune installation requise.
- **`.deb`** (x86_64) : paquet Debian/Ubuntu (`sudo apt install ./Apollia-OS_<version>_amd64.deb`).
- **`apollia-os-linux-x86-cpu.tar.gz`** et **`apollia-os-linux-x86-vulkan.tar.gz`** : les deux bundles en ligne de commande pour x86_64, un par moteur, CPU ou Vulkan.
- **`apollia-os-linux-arm-cpu.tar.gz`** : le bundle en ligne de commande pour aarch64. Aucun installeur de bureau n'est publié pour cette architecture.

## Pré-requis

- Distribution glibc 2.39+ (Ubuntu 24.04, Debian 13, Fedora 40, etc.) : les
  binaires publiés sont construits sur Ubuntu 24.04 sans lien statique.
- 4 Go de RAM libres minimum.
- Pour l'inférence sur GPU : un pilote Vulkan 1.3+, Mesa pour AMD et Intel, le
  pilote NVIDIA 550+ pour NVIDIA.

## Installation (AppImage)

```sh
chmod +x Apollia-OS_<version>_amd64.AppImage
./Apollia-OS_<version>_amd64.AppImage
```

L'app démarre le daemon en arrière-plan. Le daemon sert l'inférence LLM locale via le moteur embarqué `llama-server` et lance le runner de reconnaissance vocale (STT).

Un point à connaître avant de choisir ce format : une AppImage est un montage en
lecture seule qui n'existe que pendant l'exécution de l'app. La ligne de commande
vit dans ce montage, et le lien `/usr/local/bin` proposé par **Réglages >
Système** y pointe, donc il pend dès que vous quittez. Pour une ligne de commande
qui survit, installez le `.deb` ou décompressez une des archives
`apollia-os-linux-*`.

## Installation (.deb)

```sh
sudo apt install ./Apollia-OS_<version>_amd64.deb
```

Lancez ensuite l'application depuis le menu des applications de votre bureau,
sous l'entrée **Apollia OS** :
l'app démarre le daemon elle-même. La ligne de commande `apollia-os` est livrée
dans le paquet (`/usr/lib/Apollia OS/`) mais n'est pas sur votre `PATH` tant que
vous ne l'activez pas depuis **Réglages > Système** dans l'app, qui crée le lien
`/usr/local/bin`. Les commandes de vérification ci-dessous supposent ce lien.

## Vérification

```sh
apollia-os --version
apollia-os doctor
```

`doctor` vérifie le répertoire de données, le fichier de configuration, les deux
bases, le répertoire des modèles, Python, la posture de bac à sable et la socket
du runtime. Il ne détecte **pas** votre GPU. La commande qui rapporte le matériel
détecté est une autre, et elle a besoin du daemon démarré :

```sh
apollia-os model hardware --json
```

Elle répond la RAM totale, le processeur et l'accélérateur détecté. Demandez
`--json` : le rendu texte de cette commande n'affiche rien aujourd'hui.

## Fermer l'application

La croix de fermeture quitte Apollia sous Linux, et le daemon s'arrête avec
elle, de même que le moteur `llama-server` et le runner `apollia-runner-*`.
L'icône de la zone de notification porte trois entrées, ouvrir la fenêtre,
afficher les approbations en attente, et quitter ; aucune ne masque la fenêtre en
gardant le runtime actif, donc réduisez la fenêtre plutôt que de la fermer quand
vous voulez qu'Apollia reste résident. macOS fait exception : la fermeture d'une
fenêtre y laisse l'app active derrière l'icône de la barre de menus.

## Accélération GPU

Deux accélérations, et Linux y répond différemment.

**L'inférence LLM locale tourne déjà sur le GPU.** Les bundles de bureau,
AppImage comme `.deb`, embarquent une compilation Vulkan du moteur
`llama-server`, et le bundle en ligne de commande
`apollia-os-linux-x86-vulkan.tar.gz` la version Vulkan. Vulkan pilote
indifféremment les cartes NVIDIA, AMD et Intel. Rien à installer et rien à
régler : avec un pilote qui fonctionne, le moteur utilise la carte. Le bundle
`-cpu` est celui à prendre sur une machine sans pilote graphique.

**Une carte NVIDIA peut aller plus loin.** Il y a un installeur par système, et
aucun installeur à part pour les cartes NVIDIA. Quand l'application détecte une
telle carte, son écran IA propose le moteur accéléré, **Accélérer avec votre
carte NVIDIA** : un clic le télécharge, une fois sa signature vérifiée, et le
prochain chargement d'un modèle l'utilise. Sous Linux, ce moteur s'appuie sur
les bibliothèques CUDA de la machine.

**La dictée tourne sur le processeur dans les paquets de bureau.** La
reconnaissance vocale tourne dans le sidecar `apollia-runner`, bâti sur
whisper. Les paquets de bureau n'en livrent qu'un seul, `apollia-runner-cpu` ;
l'archive Vulkan en ligne de commande porte `apollia-runner-vulkan`, whisper
sur la carte, que le daemon utilise quand le pilote fournit un chargeur Vulkan.

## Mettre à jour

Voir [Mettre à jour Apollia](./mettre-a-jour-apollia.md).

## Désinstallation

```sh
sudo apt remove apollia-os    # paquet .deb
# ou supprimez simplement l'AppImage
rm -rf ~/.apollia              # données utilisateur
```
