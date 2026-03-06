# Guide d'installation — Apollia OS

---

## Prerequis

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

# Compiler tout le workspace
cargo build --workspace

# Ou en release (recommande pour la production)
cargo build --workspace --release
```

Le binaire produit est `target/debug/apollia-os` (ou `target/release/apollia-os`).

Pour l'ajouter au PATH :

```bash
# Ajouter temporairement
export PATH="$PWD/target/debug:$PATH"

# Ou installer directement
cargo install --path crates/apollia-cli
```

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
