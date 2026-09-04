---
title: Installer Apollia sur Linux
slug: /operator-help/installation/install-on-linux
sidebar_position: 3
---

# Installer Apollia sur Linux

Chacun des fichiers ci-dessous est attaché à chaque publication sur la page de
publication, `https://github.com/Apollia-OS/apollia-os/releases`.

- **`.AppImage`** (recommandé, x86_64) : application desktop portable, aucune installation requise.
- **`.deb`** (x86_64) : paquet Debian/Ubuntu (`sudo apt install ./Apollia-OS_<version>_amd64.deb`).
- **`Apollia-OS_<version>_amd64-cuda.deb`** : le même paquet avec une compilation CUDA du moteur d'inférence, pour une carte NVIDIA. Aucune AppImage CUDA n'est publiée.
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

Lancez ensuite **Apollia OS** depuis le menu des applications de votre bureau :
l'app démarre le daemon elle-même. La ligne de commande `apollia-os` est livrée
dans le paquet (`/usr/lib/apollia-os/`) mais n'est pas sur votre `PATH` tant que
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
`llama-server`, le `-cuda.deb` une compilation CUDA pour les cartes NVIDIA, et le
bundle en ligne de commande `apollia-os-linux-x86-vulkan.tar.gz` la version
Vulkan. Vulkan pilote indifféremment les
cartes NVIDIA, AMD et Intel. Rien à installer et rien à régler : avec un pilote
qui fonctionne, le moteur utilise la carte. Le bundle `-cpu` est celui à prendre
sur une machine sans pilote graphique.

**La dictée reste sur le processeur.** La reconnaissance vocale tourne dans le
sidecar `apollia-runner`, bâti sur whisper, et whisper n'a pas de backend
Vulkan : le binaire `apollia-runner-vulkan` livré dans l'archive Vulkan est
octet pour octet celui du CPU. Le copier par-dessus le runner embarqué ne change
rien. Aucun artefact Linux publié aujourd'hui ne porte de runner de
reconnaissance vocale accéléré par le GPU.

## Mettre à jour

Voir [Mettre à jour Apollia](./mettre-a-jour-apollia.md).

## Désinstallation

```sh
sudo apt remove apollia-os    # paquet .deb
# ou supprimez simplement l'AppImage
rm -rf ~/.apollia              # données utilisateur
```
