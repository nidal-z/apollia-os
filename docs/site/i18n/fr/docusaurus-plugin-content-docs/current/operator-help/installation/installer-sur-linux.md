---
title: Installer Apollia sur Linux
sidebar_position: 3
---

# Installer Apollia sur Linux

Apollia est distribué pour Linux x86_64 sous trois formats :

- **`.AppImage`** (recommandé) : application desktop portable, aucune installation requise.
- **`.deb`** : paquet Debian/Ubuntu (`sudo apt install ./apollia-os_<version>_amd64.deb`).
- **`apollia-os-linux-x86-*.tar.gz`** : bundles CLI par accélérateur (CPU, CUDA, ROCm, Vulkan).

## Pré-requis

- Distribution glibc 2.31+ (Ubuntu 22.04, Debian 12, Fedora 38, etc.).
- 4 Go de RAM libres minimum.
- Pour GPU : driver à jour (NVIDIA 550+, ROCm 6.0+, ou Mesa Vulkan 1.3+).

## Installation (AppImage)

```sh
chmod +x Apollia-OS_<version>_amd64.AppImage
./Apollia-OS_<version>_amd64.AppImage
```

L'app démarre le daemon en arrière-plan. Le daemon sert l'inférence LLM locale via le moteur embarqué `llama-server` et lance le runner de reconnaissance vocale (STT) adapté à votre GPU.

## Installation (.deb)

```sh
sudo apt install ./apollia-os_<version>_amd64.deb
apollia-os start
```

## Vérification

```sh
apollia-os --version
apollia-os doctor
```

`doctor` vérifie le répertoire de données, le fichier de configuration, les deux
bases, le répertoire des modèles, Python, la posture de bac à sable et la socket
du runtime. Il ne détecte **pas** votre GPU, et aucune commande ne le rapporte :
le périphérique d'inférence se configure au lieu de se sonder. Sous Linux il vaut
CPU par défaut, donc l'accélération GPU est quelque chose que vous posez, pas que
vous vérifiez ici. Voir la section ci-dessous.

## Fermer l'application

La croix de fermeture quitte Apollia sous Linux, et le daemon s'arrête avec
elle, de même que le moteur `llama-server` et le runner `apollia-runner-*`. Pour
garder le runtime résident en masquant la fenêtre, passez par l'icône de la zone
de notification plutôt que par la croix. macOS fait exception : la fermeture
d'une fenêtre y laisse l'app active derrière l'icône de la barre de menus.

## Accélération GPU

L'inférence LLM locale passe par le moteur embarqué `llama-server`, livré avec le bundle. La reconnaissance vocale (STT) utilise le runner `apollia-runner`, dont l'AppImage / paquet `.deb` embarque la variante CPU. Pour accélérer la dictée sur GPU :

1. Téléchargez le bundle CLI dédié : `apollia-os-linux-x86-cuda.tar.gz` (ou rocm/vulkan).
2. Décompressez et copiez `apollia-runner-<backend>` à côté du binaire `apollia-os`.
3. Redémarrez : `apollia-os stop && apollia-os start`.

Le daemon détecte automatiquement le runner ajouté.

## Mettre à jour

Voir [Mettre à jour Apollia](./mettre-a-jour-apollia.md).

## Désinstallation

```sh
sudo apt remove apollia-os    # paquet .deb
# ou supprimez simplement l'AppImage
rm -rf ~/.apollia              # données utilisateur
```
