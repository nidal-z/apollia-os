# Installation

Trois éléments à installer : le runtime Rust (le binaire `apollia`), le SDK Python (le package `apollia`), et un LLM accessible (local llama.cpp bundlé, Ollama, ou une clé d'API cloud).

L'objectif : à la fin de ce chapitre, vous tapez `apollia --version` et obtenez une réponse, puis `python -m apollia inspect un-agent.py` valide un fichier que vous avez sous la main.

---

## Pré-requis

- Python 3.10 ou supérieur
- Rust toolchain (`rustup`) si vous compilez depuis les sources. Sinon, téléchargez le binaire précompilé pour votre OS.
- Un système d'exploitation : macOS (Apple Silicon ou Intel) ou Linux (Debian / Ubuntu / Arch). Sur Windows, utilisez WSL2 (cf. l'annexe G FAQ, section « Apollia tourne sur Windows ? »).

Pas de Docker. Pas de Kubernetes. Pas de compte cloud.

---

## Installer le runtime Rust

### Depuis les sources

```bash
git clone https://github.com/nidal-z/apollia-os.git
cd apollia-os
```

Choisissez les **feature flags** selon votre plateforme. Le build par défaut n'inclut que les backends LLM cloud. Pour un LLM local (llama.cpp), il faut ajouter le bon flag d'accélération :

| Plateforme | Commande |
|---|---|
| macOS Apple Silicon | `cargo build --release --features local-metal` |
| macOS Intel | `cargo build --release --features local-accelerate` |
| Linux NVIDIA (CUDA) | `cargo build --release --features local-cuda` |
| Linux AMD (ROCm) | `cargo build --release --features local-rocm` |
| Linux ou portable (Vulkan) | `cargo build --release --features local-vulkan` |
| Cloud uniquement (pas de LLM local) | `cargo build --release` |

Sans `--features local-*`, le runtime démarre avec `LlmRouter failed to initialize` et `ctx.llm` lèvera une erreur. C'est attendu si vous n'utilisez que des backends cloud (Anthropic, OpenAI, Ollama distant).

Le binaire est produit en `target/release/apollia-os`. Ajoutez-le à votre `PATH` :

```bash
sudo cp target/release/apollia-os /usr/local/bin/   # macOS / Linux
```

Vérification :

```bash
apollia-os --version
# apollia-os 0.1.0
```

### Binaire précompilé

Si vous ne voulez pas compiler, téléchargez le `.dmg` (macOS) ou le `.deb` (Debian / Ubuntu) depuis la page de release. Le binaire est livré déjà signé, sans dépendance Python à installer côté runtime (PyO3 embarque l'interpréteur).

---

## Installer le SDK Python

Le SDK `apollia` est requis pour **écrire** des agents. Le runtime peut en charger sans qu'il soit installé globalement, mais pour développer localement, installez-le dans votre environnement :

```bash
pip install apollia
```

Vérification :

```bash
python -c "import apollia; print(apollia.__version__)"
# 0.5.0
```

Le SDK est stdlib-only en surface : aucune dépendance PyPI lourde n'est tirée. Les packages spécifiques à un agent (par exemple `pypdf` pour un worker PDF) sont déclarés dans `@agent(packages=("pypdf>=4",))` et installés au boot dans un venv isolé.

---

## Configurer un backend LLM

La gestion des backends passe par `apollia-os llm backends`. Trois options, au choix selon votre contexte.

### Local (zéro réseau)

Le runtime embarque `llama.cpp` et peut charger n'importe quel modèle GGUF. Téléchargez un modèle depuis Hugging Face (par exemple `mistral-7b-instruct-v0.2.Q4_K_M.gguf`) puis :

```bash
apollia-os llm backends create local \
  --provider llama-cpp \
  --model /chemin/absolu/mistral-7b-instruct-v0.2.Q4_K_M.gguf
apollia-os llm backends set-default local
```

Pas de clé d'API à configurer. Vos prompts ne quittent pas la machine.

### Ollama

Si vous avez déjà Ollama installé, déclarez-le :

```bash
apollia-os llm backends create ollama \
  --provider ollama \
  --model llama3:8b
apollia-os llm backends set-default ollama
```

Le runtime appelle Ollama sur `http://localhost:11434` par défaut. Pour pointer ailleurs, voyez `apollia-os llm backends create --help`.

### Cloud (Anthropic, OpenAI)

Pour la qualité ou les capacités multimodales (vision) :

```bash
# Recommandé : passer la clé par variable d'environnement.
export ANTHROPIC_API_KEY=sk-ant-...

apollia-os llm backends create anthropic \
  --provider anthropic \
  --model claude-sonnet-4 \
  --api-key-env ANTHROPIC_API_KEY
apollia-os llm backends set-default anthropic
```

`--api-key-env` est la voie recommandée : le runtime relit la clé depuis l'environnement à chaque appel, sans la persister en base. Si vous utilisez `--api-key <valeur>` directement, la clé est stockée en clair dans `~/.apollia/system.db` (déconseillé hors environnement de développement).

### Diagnostiquer

```bash
apollia-os llm status                  # vue rapide : backends configurés, défaut, modèles prêts
apollia-os llm backends list           # tableau des backends enregistrés
apollia-os llm backends show <name>    # détail (provider, modèle, source de la clé)
```

---

## Hello, agent

Créez un fichier `hello.py` :

```python
from apollia import agent, skill
from apollia.types import Ctx


@agent(name="hello", version="0.1.0", description="A tiny hello-world agent.")
class Hello:
    @skill("hello.greet", description="Greet a person by name.")
    async def greet(self, name: str, ctx: Ctx) -> dict:
        return {"message": f"Bonjour, {name} !"}
```

Inspectez-le :

```bash
python -m apollia inspect hello.py
```

Vous devriez voir un récapitulatif du manifeste : un agent nommé `hello`, version `0.1.0`, une skill `hello.greet` avec son input_schema inféré (`{"name": "string"}`).

Si le rapport est vert, vous êtes prêt à passer aux quickstarts qui suivent.

---

## En cas de problème

- **`apollia-os: command not found`** : le binaire n'est pas dans le `PATH`. Vérifiez avec `which apollia-os` puis ajoutez le bon dossier à votre `PATH` (souvent `/usr/local/bin` ou `~/.cargo/bin`).
- **`ModuleNotFoundError: No module named 'apollia'`** : le SDK Python n'est pas installé dans l'environnement actif. Vérifiez avec `pip show apollia` et `python -c "import sys; print(sys.executable)"`.
- **`python -m apollia inspect` lève `AgentConfigError`** : un argument du décorateur `@agent` ou `@skill` est invalide. Le message indique précisément la cause. Voir le [chapitre 6](../part-ii-the-decorators/06-agent-decorator.md) pour les contraintes.
- **Pas de réponse LLM** : vérifiez la configuration backend avec `apollia-os llm status`. Si vous utilisez un backend cloud, vérifiez la source de la clé d'API avec `apollia-os llm backends show <name>`.

Pour plus de détails sur la CLI, voir le [chapitre 33](../part-viii-runtime-rust/33-cli-complete.md). Pour la commande `apollia inspect` en détail, voir le [chapitre 27](../part-vii-tooling/27-apollia-inspect.md).
