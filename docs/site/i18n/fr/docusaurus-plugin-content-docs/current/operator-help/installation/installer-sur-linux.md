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
apollia-os doctor --json | jq .gpu
```

Sortie attendue :

- NVIDIA RTX → `vendor: Nvidia, recommended_backend: Cuda`
- AMD Radeon → `vendor: Amd, recommended_backend: Rocm`
- Intel/autre → `vendor: ..., recommended_backend: Vulkan`

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
