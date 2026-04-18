# packaging/ — scripts de release DMG/AppImage (ADR-073, ADR-074)

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

## Version pinnée

- Python : **3.13.1**
- python-build-standalone release tag : **20250205**

Mettre à jour manuellement (éditer `fetch-python-standalone.sh`) — pas d'auto-update.

## Targets supportées

| Target triple | Plate-forme |
|---|---|
| `aarch64-apple-darwin` | macOS Apple Silicon |
| `x86_64-apple-darwin` | macOS Intel |
| `universal-apple-darwin` | macOS universal2 (lipo ARM + Intel) |
| `x86_64-unknown-linux-gnu` | Linux x86_64 |
| `aarch64-unknown-linux-gnu` | Linux ARM64 |

## Build de release local (macOS)

```bash
# 1. Ajouter les toolchains Rust si absent (universal2 only)
rustup target add aarch64-apple-darwin x86_64-apple-darwin

# 2. Préparer le bundle Python (~5 min la 1ère fois, cached ensuite)
./packaging/build-python-bundle.sh universal-apple-darwin target/python-bundle/universal-apple-darwin

# 3. Builder l'app Tauri (invoque bundle-cli.sh qui refait le step 2 si besoin)
cd crates/apollia-desktop
cargo tauri build --target universal-apple-darwin

# 4. Post-bundle : patcher install_names + signer ad-hoc
bash scripts/after-bundle.sh
codesign --force --deep --sign - \
  --options runtime \
  --entitlements entitlements.plist \
  "../../target/universal-apple-darwin/release/bundle/macos/Apollia OS.app"

# 5. DMG produit dans target/universal-apple-darwin/release/bundle/dmg/
```

## Build de release local (Linux)

```bash
./packaging/build-python-bundle.sh x86_64-unknown-linux-gnu target/python-bundle/x86_64-unknown-linux-gnu

cd crates/apollia-desktop
cargo tauri build

bash scripts/after-bundle.sh
# AppImage: target/release/bundle/appimage/*.AppImage
# .deb:     target/release/bundle/deb/*.deb
```

## Taille attendue des artefacts

| Composant | Taille |
|---|---|
| Python 3.13.1 stripped (macOS, install_only) | ~40 MB |
| + site-packages (pandas + openpyxl + pypdf + httpx + bs4 + markdownify) | ~65 MB |
| Rust binary (apollia-desktop + apollia-os, release, universal2) | ~60 MB |
| **DMG universal2 total** | **~135 MB** |
| **AppImage Linux x86_64 total** | **~100 MB** |

## Ajout d'une nouvelle dépendance Python

1. Ajouter la ligne dans `requirements-bundled.txt` avec une version pinnée.
2. Rebuild local : `./packaging/build-python-bundle.sh <target> target/python-bundle/<target>`.
3. Vérifier la taille : `du -sh target/python-bundle/<target>/python/`.
4. Documenter dans KNOWN-ISSUES.md si la dép ajoute > 10 MB.

## Debug : Python refuse de se charger au runtime

```bash
# macOS : inspecter les load commands du binaire
otool -L target/universal-apple-darwin/release/apollia-desktop | grep python

# Doit ressortir : @executable_path/../Resources/python/lib/libpython3.13.dylib

# Linux : inspecter le RPATH
patchelf --print-rpath target/release/apollia-desktop

# Doit ressortir : $ORIGIN/../lib/apollia-os/python/lib
```

Si c'est `/opt/homebrew/...` ou `/Users/...`, c'est que le post-build patch n'a
pas tourné ou a échoué. Relancer `scripts/after-bundle.sh`.
