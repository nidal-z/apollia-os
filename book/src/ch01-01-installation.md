# Installation

Apollia OS est distribué en code source. La première étape est de le compiler sur votre machine. Ce n'est pas aussi intimidant que ça en a l'air : `cargo build` gère tout.

> **Première compilation :** 5–10 min sur Linux, 15–25 min sur macOS (Rust compile toutes les dépendances depuis les sources).

---

## Prérequis

Vérifiez que ces trois outils sont présents :

```bash
rustc --version    # >= 1.75 (stable)
python3 --version  # >= 3.11
sqlite3 --version  # >= 3.35
```

Si Rust n'est pas installé :

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env
```

> **Pourquoi SQLite ?** Apollia OS utilise SQLite avec l'extension FTS5 (recherche plein texte) pour stocker la mémoire des agents. La version 3.35+ est requise pour certaines fonctionnalités de FTS5.

---

## Cloner et compiler

```bash
git clone https://github.com/nidal-z/apollia-os.git
cd apollia-os
cargo build --workspace --release
```

La première compilation télécharge et compile toutes les dépendances Rust. Les compilations suivantes sont beaucoup plus rapides grâce au cache de Cargo.

Ajoutez le binaire à votre PATH :

```bash
export PATH="$PWD/target/release:$PATH"
```

Pour une installation permanente :

```bash
cargo install --path crates/apollia-cli
```

Vérifiez que l'installation a fonctionné :

```bash
apollia-os --version
# apollia-os 0.1.0
```

---

## macOS uniquement : configurer PyO3

Apollia OS embarque un interpréteur Python via PyO3. Sur macOS, vous devez indiquer explicitement quel Python utiliser avant de compiler — sinon la compilation échoue avec une erreur `python3 not found`.

```bash
export PYO3_PYTHON=$(brew --prefix python@3.13)/bin/python3.13
cargo build --workspace --release
```

Pour rendre ce réglage permanent :

```bash
echo 'export PYO3_PYTHON=$(brew --prefix python@3.13)/bin/python3.13' >> ~/.zshrc
```

> Sur Linux, PyO3 localise automatiquement le Python système. Cette étape est spécifique à macOS.

---

## Option : activer l'inférence locale

Par défaut, `ctx.llm` (l'accès au LLM depuis vos agents) est disponible uniquement si vous configurez un backend `type = "api"` dans `apollia.toml`. Si vous voulez faire tourner un modèle `.gguf` directement sur votre machine, recompilez avec la feature correspondante :

```bash
# CPU (tout matériel)
cargo build --workspace --release --features local

# GPU Apple Silicon
cargo build --workspace --release --features local-metal
```

Ne vous inquiétez pas de cette option pour l'instant — les chapitres 2 et 6 expliquent comment configurer les LLM en détail. Pour ce chapitre, votre premier agent n'utilise pas de LLM.

---

## Vérification rapide

Le dépôt cloné contient déjà un agent de démonstration dans `agents/hello_agent.py`. Testons que tout fonctionne :

```bash
# 1. Démarrer le runtime
apollia-os start

# 2. Déployer l'agent de démo
apollia-os agent start agents/hello_agent.py

# 3. Envoyer une tâche
apollia-os run hello-agent "Bonjour Apollia"

# 4. Arrêter
apollia-os stop
```

Résultat attendu à l'étape 3 :

```
Done in 0.3s (1 step, 0 tool calls)
RESULT
Bonjour ! J'ai reçu : Bonjour Apollia
```

Si vous voyez ce résultat, l'installation est complète. Dans la section suivante, vous allez écrire cet agent vous-même — ligne par ligne.

---

## Résolution de problèmes

| Symptôme | Cause probable | Solution |
|---|---|---|
| `python3 not found` (macOS) | PYO3_PYTHON non définie | Voir section macOS ci-dessus |
| `sqlite3: FTS5 not available` | Version SQLite trop ancienne | Mettre à jour SQLite >= 3.35 |
| `apollia-os: command not found` | PATH non configuré | `export PATH="$PWD/target/release:$PATH"` |
| Compilation échoue sur linking | Bibliothèques système manquantes | `sudo apt install build-essential libssl-dev` (Debian/Ubuntu) |

Pour les problèmes non listés ici, consultez le [guide d'installation complet](../appendix-b-glossary.md) ou ouvrez une issue sur le dépôt.
