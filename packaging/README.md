# packaging/ - scripts de release DMG/AppImage

Ce dossier contient tout ce qui est nécessaire pour assembler une release
auto-contenue d'Apollia OS : Python embarqué + site-packages curé, binaire CLI,
bundle Tauri signé.

## Fichiers

| Fichier | Rôle |
|---|---|
| `requirements-bundled.txt` | Set « batteries included » pip pré-installé dans le bundle Python (pandas, openpyxl, pypdf, httpx, bs4, markdownify, python-dateutil). |
| `fetch-python-standalone.sh` | Télécharge et extrait `python-build-standalone` d'Astral pour une target donnée. Cache sous `target/python-bundle/<triple>/.cache/`. |
| `build-universal-python.sh` | Compose un bundle macOS universal2 en `lipo -create` des Mach-O Python ARM + Intel. |
| `build-python-bundle.sh` | Orchestrateur : fetch + `pip install -r requirements-bundled.txt` + pruning + patch `install_name` (macOS). |
| `artifacts.json` | Contrat de nommage des artefacts de release : lu par `release.yml` (producteur), embarqué par `apollia-os update` (`include_str!`), cité par la page d'installation ; gardé par `scripts/check_release_artifacts.py`. |
| `fetch-llama-server.sh` | Télécharge le binaire `llama-server` amont pour une plateforme (`uname -s` et `uname -m`) et vérifie sa somme de contrôle. |
| `llama-server-checksums.txt` | Sommes de contrôle attendues, une par plateforme, lues par le script ci-dessus. |
| `launchers/` | Lanceurs par plateforme empaquetés avec l'application. |

## Version pinnée

- Python : **3.13.13**
- python-build-standalone release tag : **20260414**

Mettre à jour manuellement (éditer `fetch-python-standalone.sh`) - pas d'auto-update.

## Targets supportées

| Target triple | Plate-forme | Statut |
|---|---|---|
| `aarch64-apple-darwin` | macOS Apple Silicon | ✅ Actif (desktop + CLI) |
| `x86_64-unknown-linux-gnu` | Linux x86_64 | ✅ Actif (desktop + CLI) |
| `x86_64-pc-windows-msvc` | Windows x86_64 | ✅ Actif (desktop + CLI) |
| `aarch64-unknown-linux-gnu` | Linux ARM64 | ✅ Actif (CLI seulement, non bloquant en release) |
| `aarch64-pc-windows-msvc` | Windows ARM64 | ❌ Retiré de la 0.1.0 par `fc950bc0` (pandas ne publie pas de roue `win_arm64`) |
| `x86_64-apple-darwin` | macOS Intel | 🔜 Prévu v0.2.0 |
| `universal-apple-darwin` | macOS universal2 (lipo ARM + Intel) | 🔜 Prévu v0.2.0 |

**Note :** la release v0.1.0 livre les installeurs desktop macOS ARM, Linux
x86_64 et Windows x86_64, variantes CUDA Linux et Windows comprises, plus les
**six** bundles CLI de `release.yml`. Six des deux côtés, et pas par
convention : `artifacts.json` déclare six entrées `cli`, la matrice `FULL` du
job `setup` de `release.yml` en énumère les six mêmes, et
`scripts/check_release_artifacts.py` refuse toute divergence entre les deux
listes (contrôle `matrix-bijection`). Les noms publiés sont ceux de
`artifacts.json`, extension comprise : `.tar.gz` sur macOS et Linux, `.zip`
sur Windows.

## Build de release local (macOS)

```bash
# 1. Ajouter la toolchain Rust si absent
rustup target add aarch64-apple-darwin

# 2. Préparer le bundle Python (~3 min la 1ère fois, cached ensuite)
./packaging/build-python-bundle.sh aarch64-apple-darwin target/python-bundle/aarch64-apple-darwin

# 3. Builder l'app Tauri
cd crates/apollia-desktop
cargo tauri build --target aarch64-apple-darwin

# 4. Vérifier la signature posée par Tauri (le patch libpython et la
#    signature ad hoc sont appliqués pendant le build, avant le scellement
#    du .dmg, par scripts/patch-prebundle-libpython.sh et tauri.conf.json)
codesign --verify --verbose=2 \
  "../../target/aarch64-apple-darwin/release/bundle/macos/Apollia OS.app"

# 5. DMG produit dans target/aarch64-apple-darwin/release/bundle/dmg/
```

**Alternative recommandée :** utiliser `scripts/test-build-macos.sh` qui automatise
les 5 étapes ci-dessus avec validation des prérequis.

```bash
./scripts/test-build-macos.sh
```

## Build de release local (Linux)

```bash
./packaging/build-python-bundle.sh x86_64-unknown-linux-gnu target/python-bundle/x86_64-unknown-linux-gnu

cd crates/apollia-desktop
cargo tauri build

# AppImage: target/release/bundle/appimage/*.AppImage
# .deb:     target/release/bundle/deb/*.deb
```

## Taille attendue des artefacts

| Composant | Taille |
|---|---|
| Python 3.13.13 stripped (macOS ARM64, install_only) | ~17 MB |
| + site-packages (pandas + openpyxl + pypdf + httpx + bs4 + markdownify) | ~48 MB |
| Bundle Python complet une fois stagé (mesuré en staging local) | ~122 MB |
| Moteur `llama-server` + bibliothèques (répertoire `runners/`) | ~30 MB |
| Rust binary (apollia-desktop + apollia-os, release, ARM64) | ~35 MB |

*Note : tailles indicatives, à re-mesurer sur un artefact réel ; les anciens
totaux (« DMG ~75 MB ») ignoraient le moteur et le bundle Python stagé.*

## Ajout d'une nouvelle dépendance Python

1. Ajouter la ligne dans `requirements-bundled.txt` avec une version pinnée.
2. Rebuild local : `./packaging/build-python-bundle.sh <target> target/python-bundle/<target>`.
3. Vérifier la taille : `du -sh target/python-bundle/<target>/python/`.
4. Noter dans le CHANGELOG si la dépendance ajoute plus de 10 Mo au bundle.

## Debug : Python refuse de se charger au runtime

```bash
# macOS : inspecter les load commands du binaire
otool -L target/aarch64-apple-darwin/release/apollia-desktop | grep python

# Doit ressortir : @executable_path/../Resources/python/lib/libpython3.13.dylib

# Linux : inspecter le RPATH
patchelf --print-rpath target/release/apollia-desktop

# Doit ressortir : $ORIGIN/../lib/apollia-os/python/lib
```

Si c'est `/opt/homebrew/...` ou `/Users/...`, c'est que le patch d'avant
scellement n'a pas tourné ou a échoué. Relancer le build, ou
`crates/apollia-desktop/scripts/patch-prebundle-libpython.sh` à la main.
