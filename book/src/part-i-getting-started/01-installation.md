# Installation

Trois éléments à installer : le runtime Rust (le binaire `apollia`), le SDK Python (le package `apollia`), et un LLM accessible (local llama.cpp bundlé, Ollama, ou une clé d'API cloud).

L'objectif : à la fin de ce chapitre, vous tapez `apollia --version` et obtenez une réponse, puis `python -m apollia inspect un-agent.py` valide un fichier que vous avez sous la main.

---

## Pré-requis

- Python 3.10 ou supérieur
- Rust toolchain (`rustup`) si vous compilez depuis les sources. Sinon, téléchargez le binaire précompilé pour votre OS.
- Un système d'exploitation : macOS (Apple Silicon ou Intel), Linux (Debian / Ubuntu / Arch), Windows 11.

Pas de Docker. Pas de Kubernetes. Pas de compte cloud.

---

## Installer le runtime Rust

### Depuis les sources

```bash
git clone https://github.com/nidal-z/apollia-os.git
cd apollia-os
cargo build --release --workspace
```

Le binaire est produit en `target/release/apollia`. Ajoutez-le à votre `PATH` :

```bash
sudo cp target/release/apollia /usr/local/bin/   # macOS / Linux
```

Vérification :

```bash
apollia --version
# apollia 0.1.0
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

Trois options, au choix selon votre contexte.

### Local (zéro réseau)

Le runtime embarque `llama.cpp` et peut charger n'importe quel modèle GGUF. Téléchargez un modèle depuis Hugging Face (par exemple `mistral-7b-instruct-v0.2.Q4_K_M.gguf`) puis :

```bash
apollia llm add local --path ~/models/mistral-7b.gguf
apollia llm set-default local
```

Pas de clé d'API à configurer. Vos prompts ne quittent pas la machine.

### Ollama

Si vous avez déjà Ollama installé, déclarez-le :

```bash
apollia llm add ollama --url http://localhost:11434 --model llama3:8b
apollia llm set-default ollama
```

### Cloud (Anthropic, OpenAI)

Pour de la qualité ou des capacités multimodales (vision) :

```bash
apollia secrets set anthropic_api_key=sk-ant-...
apollia llm add anthropic --model claude-sonnet-4
apollia llm set-default anthropic
```

Le secret est chiffré localement (keyring système). Aucune valeur n'apparaît dans les logs ni dans les manifests.

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

- **`apollia: command not found`** : le binaire n'est pas dans le `PATH`. Vérifiez avec `which apollia` puis ajoutez le bon dossier à votre `PATH` (souvent `/usr/local/bin` ou `~/.cargo/bin`).
- **`ModuleNotFoundError: No module named 'apollia'`** : le SDK n'est pas installé dans l'environnement Python actif. Vérifiez avec `pip show apollia` et `python -c "import sys; print(sys.executable)"`.
- **`apollia inspect` lève `AgentConfigError`** : un argument du décorateur `@agent` ou `@skill` est invalide. Le message indique précisément la cause. Voir le [chapitre 6](../part-ii-the-decorators/06-agent-decorator.md) pour les contraintes.
- **Pas de réponse LLM** : vérifiez la configuration backend avec `apollia llm list`. Si vous utilisez un backend cloud, vérifiez le secret avec `apollia secrets list`.

Pour plus de détails sur la CLI, voir le [chapitre 33](../part-viii-runtime-rust/33-cli-complete.md). Pour la commande `apollia inspect` en détail, voir le [chapitre 27](../part-vii-tooling/27-apollia-inspect.md).
