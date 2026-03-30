# Guide d'installation — Apollia OS

---

## Prérequis

| Outil | Version minimale | Installation |
|---|---|---|
| Rust | 1.75+ | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| Python | 3.11+ | [python.org](https://www.python.org/downloads/) ou `brew install python@3.13` (macOS) |
| SQLite | 3.35+ (FTS5) | Inclus dans la plupart des distributions Linux et macOS |

Verifier les versions :

```bash
rustc --version        # >= 1.75
python3 --version      # >= 3.11
sqlite3 --version      # >= 3.35
```

---

## Build

```bash
# Cloner le depot
git clone https://github.com/nidal-z/apollia-os.git
cd apollia-os

# Compiler tout le workspace (binaire léger — backends cloud uniquement)
cargo build --workspace

# Ou en release (recommande pour la production)
cargo build --workspace --release
```

Le binaire produit est `target/debug/apollia-os` (ou `target/release/apollia-os`).

### Build avec moteur LLM local (inférence in-process)

Par défaut, le build n'inclut que les backends cloud (HTTP). Pour activer l'inférence locale (modèles `.gguf` sur la machine), choisir une feature selon le matériel :

| Feature | Matériel cible | Commande |
|---|---|---|
| `local` / `local-cpu` | CPU (tout matériel) | `cargo build --features local` (`local-cpu` est un alias de `local`) |
| `local-metal` | GPU Apple Silicon (M1/M2/M3/M4) | voir ci-dessous |
| `local-accelerate` | macOS CPU + BLAS vectorisé | `cargo build --features local-accelerate` |
| `local-cuda` | GPU NVIDIA | `cargo build --features local-cuda` (non testé) |

**Metal sur macOS (Apple Silicon) :**

```bash
# Build standard — fonctionne sans Xcode complet (Command Line Tools suffit)
# MISTRALRS_METAL_PRECOMPILE=0 est défini par défaut dans .cargo/config.toml
cargo build --workspace --release --features local-metal

# Combiner avec Accelerate (BLAS vectorisé) pour de meilleures performances
cargo build --workspace --release --features local-metal,local-accelerate
```

Le projet configure `MISTRALRS_METAL_PRECOMPILE=0` par défaut dans `.cargo/config.toml`. Cela signifie que les shaders Metal sont compilés **JIT par le driver Metal** au premier appel d'inférence plutôt que pendant le `cargo build` (ce qui nécessiterait `xcrun metal`, outil présent uniquement dans Xcode complet). Les performances GPU sont identiques après le premier appel.

Pour activer la précompilation au build (nécessite `/Applications/Xcode.app`) :
```bash
MISTRALRS_METAL_PRECOMPILE=1 cargo build --workspace --release --features local-metal
```

Pour l'ajouter au PATH :

```bash
# Ajouter temporairement
export PATH="$PWD/target/debug:$PATH"

# Ou installer directement (installe dans ~/.cargo/bin/)
cargo install --path crates/apollia-cli
# Vérifier que ~/.cargo/bin est dans votre PATH
```

---

## Installation application desktop

L'application desktop Apollia embarque le runtime complet dans une fenetre native (Tauri v2). Elle coexiste avec la CLI : les deux partagent le meme socket Unix.

### macOS

1. Telecharger `Apollia OS_0.1.0_aarch64.dmg` depuis la page [Releases](https://github.com/nidal-z/apollia-os/releases)
2. Ouvrir le fichier .dmg
3. Glisser **Apollia OS** dans le dossier Applications
4. Au premier lancement, clic droit → Ouvrir (l'application n'est pas encore signee)

Prerequis : Python 3.11+ installe (`brew install python@3.13`)

### Linux

**AppImage :**

1. Telecharger `apollia-desktop_0.1.0_amd64.AppImage` depuis la page [Releases](https://github.com/nidal-z/apollia-os/releases)
2. Rendre executable : `chmod +x apollia-desktop_*.AppImage`
3. Lancer : `./apollia-desktop_*.AppImage`

**Debian / Ubuntu :**

```bash
sudo dpkg -i apollia-desktop_0.1.0_amd64.deb
```

Prerequis : Python 3.11+ installe (`sudo apt install python3.13`)

### Build depuis les sources

```bash
# Installer le CLI Tauri
cargo install tauri-cli --version "^2"

# Installer les dependances frontend
cd crates/apollia-desktop/ui && npm ci && cd -

# Construire le package natif
cd crates/apollia-desktop && cargo tauri build
```

Les artefacts sont produits dans `target/release/bundle/` (dmg/, appimage/, deb/).

---

## Configuration macOS (PyO3)

Sur macOS, PyO3 doit savoir quelle installation Python utiliser. Definir la variable d'environnement avant de compiler ou de lancer les tests :

```bash
# Avec Homebrew Python 3.13
export PYO3_PYTHON=/opt/homebrew/bin/python3.13

# Puis compiler
cargo build --workspace
```

Pour rendre cette configuration permanente, ajouter la ligne dans `~/.zshrc` ou `~/.bashrc`.

---

## Verification

### Tests unitaires et d'integration

```bash
# Tous les tests (sans Python reel requis)
cargo test --workspace

# Tests exercant la chaine Python complete
PYO3_PYTHON=/opt/homebrew/bin/python3.13 \
  cargo test --workspace --features python-tests
```

### Verification du binaire

```bash
apollia-os --version
# apollia-os 0.1.0

apollia-os --help
```

### Smoke test complet

```bash
# 1. Demarrer le runtime
apollia-os start

# 2. Verifier l'etat
apollia-os status

# 3. Deployer l'agent de demo
apollia-os agent start agents/hello_agent.py

# 4. Executer une tache
apollia-os run hello-agent "Bonjour Apollia"

# 5. Arreter proprement
apollia-os stop
```

---

## Variables d'environnement

| Variable | Description | Defaut |
|---|---|---|
| `PYO3_PYTHON` | Chemin vers l'executable Python (macOS/dev) | Python du PATH |
| `APOLLIA_SOCKET` | Chemin du socket Unix | `/tmp/apollia.sock` |
| `APOLLIA_PORT` | Port TCP de l'API | `7771` |
| `RUST_LOG` | Niveau de log (`info`, `debug`, `trace`) | `info` |
| `MISTRALRS_METAL_PRECOMPILE` | `0` = shaders Metal compilés JIT (défaut projet via `.cargo/config.toml`). `1` = précompilation au build (nécessite Xcode) | `0` (défaut projet) |

---

## Depannage

### `error: no Python interpreter found` (macOS)

```bash
export PYO3_PYTHON=$(which python3)
cargo build --workspace
```

### `dylibError: Library not loaded: libpython3.XX.dylib` (macOS)

Verifier que Python a ete installe avec `--enable-shared` (Homebrew le fait par defaut). Si le probleme persiste :

```bash
brew reinstall python@3.13
```

### `address already in use` au demarrage

Un precedent processus apollia-os tourne encore :

```bash
apollia-os stop
# ou forcer :
pkill apollia-os && rm -f /tmp/apollia.sock
```

### `cargo test` echoue sur une crate specifique

```bash
# Relancer uniquement la crate concernee avec logs
cargo test -p apollia-runtime -- --nocapture
```

---

## Structure des fichiers de configuration

```
apollia.toml          # Configuration par defaut (racine du projet)
~/.config/apollia/    # Configuration utilisateur (priorite superieure)
```

Exemple `apollia.toml` minimal :

```toml
[runtime]
socket = "/tmp/apollia.sock"
port   = 7771
log_level = "info"

[memory]
path = "./data/memory.db"

[tools]
sandbox = true    # false sur macOS (namespaces Linux non disponibles)
```
