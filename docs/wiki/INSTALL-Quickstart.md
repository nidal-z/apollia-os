# Installation Quickstart — Apollia OS

> Démarrer en moins de 10 minutes sur Linux ou macOS.
> Public cible : développeur qui veut tester Apollia OS rapidement

---

## Prérequis

```bash
# Vérifier les versions
rustc --version    # >= 1.75 (stable)
python3 --version  # >= 3.11
sqlite3 --version  # >= 3.35 (FTS5 requis)
```

Installer Rust si nécessaire :
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env
```

---

## Build

```bash
# Cloner et compiler
git clone https://github.com/nidal-z/apollia-os.git
cd apollia-os
cargo build --workspace --release
```

Ajouter au PATH :
```bash
export PATH="$PWD/target/release:$PATH"
# Ou installer définitivement :
cargo install --path crates/apollia-cli
```

---

## Configuration macOS uniquement (PyO3)

```bash
export PYO3_PYTHON=/opt/homebrew/bin/python3.13
cargo build --workspace --release
```

Ajouter `export PYO3_PYTHON=...` dans `~/.zshrc` pour le rendre permanent.

---

## Test rapide (5 minutes)

```bash
# 1. Démarrer le runtime
apollia-os start

# 2. Déployer l'agent de demo
apollia-os agent start agents/hello_agent.py

# 3. Envoyer une tâche
apollia-os run hello-agent "Bonjour Apollia"

# 4. Vérifier l'état
apollia-os status

# 5. Arrêter
apollia-os stop
```

Résultat attendu pour l'étape 3 :
```
Done in 0.3s (1 step, 0 tool calls)
RESULT
Bonjour ! J'ai reçu : Bonjour Apollia
```

---

## Ce qui vient ensuite

- Pour une installation en production : [INSTALL Production](./INSTALL-Production)
- Pour écrire votre premier agent : [Agents Quickstart](./Agents-Quickstart)
- Pour la configuration avancée : [Config apollia.toml](./Config-apollia-toml)
- Pour les problèmes d'installation : [INSTALL.md](./INSTALL) — dépannage complet
