# Installation Quickstart — Apollia OS

> Démarrer Apollia OS rapidement sur Linux ou macOS.
> Public cible : développeur qui veut tester Apollia OS rapidement
>
> Première compilation : 5–10 min sur Linux, 15–25 min sur macOS (compilation Rust + dépendances).

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

PyO3 doit localiser l'interpréteur Python avant de compiler. Sans cette variable, la compilation
échoue avec une erreur `python3 not found` ou lie la mauvaise version.

```bash
export PYO3_PYTHON=$(brew --prefix python@3.13)/bin/python3.13
cargo build --workspace --release
```

Ajouter cette ligne dans `~/.zshrc` pour la rendre permanente :

```bash
echo 'export PYO3_PYTHON=$(brew --prefix python@3.13)/bin/python3.13' >> ~/.zshrc
```

---

## Test rapide

Le fichier `agents/hello_agent.py` est inclus dans le dépôt cloné. Après `git clone`, le chemin
relatif `agents/hello_agent.py` est disponible depuis la racine du dépôt.

```bash
# 1. Démarrer le runtime
apollia-os start

# 2. Déployer l'agent de demo (inclus dans le dépôt cloné)
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
