# Backends locaux et cloud

Apollia OS supporte deux familles de backends LLM : les modèles locaux qui tournent entièrement sur votre machine, et les APIs cloud. Les deux s'utilisent de manière identique depuis votre code Python — seule la configuration change.

---

## Où est stockée la configuration

Depuis la version 0.2 (Sprint 28), la configuration des backends est **persistée dans SQLite** (`~/.apollia/system.db`), pas dans un fichier TOML. Vous la gérez via la CLI ou l'interface desktop — et elle persiste entre les redémarrages.

```bash
# Lister les backends configurés
$ apollia-os llm list
  NOM          PROVIDER     MODÈLE                    DÉFAUT  ACTIF
  local        llama-cpp    llama3.2-3B-q4_K_M.gguf   non     oui
  anthropic    anthropic    claude-haiku-4-5-20251001  oui     oui
  gpt-4o-mini  openai       gpt-4o-mini               non     oui

# Tester la disponibilité d'un backend
$ apollia-os llm ping anthropic
  ✔ anthropic répond en 243ms

# Vérifier l'état général
$ apollia-os status
  Runtime    ACTIVE
  LLM        anthropic (claude-haiku-4-5-20251001) — défaut
```

---

## Backend local — llama.cpp (feature `local`)

L'inférence locale tourne entièrement sur votre machine, sans appel réseau. Le modèle est un fichier `.gguf` dans `~/.apollia/models/`.

### Compiler avec le support local

```bash
# CPU (tout matériel)
cargo build --release --features local

# Apple Silicon — GPU Metal (recommandé sur M1/M2/M3)
cargo build --release --features local-metal,local-accelerate

# NVIDIA CUDA (non testé en CI)
cargo build --release --features local-cuda
```

### Ajouter un backend local

```bash
# Télécharger un modèle (exemple)
# Placez votre fichier .gguf dans ~/.apollia/models/

apollia-os llm add \
  --name local \
  --provider llama-cpp \
  --model ~/.apollia/models/llama3.2-3B-q4_K_M.gguf
```

Ou en JSON direct (pour les scripts) :
```bash
apollia-os llm add --json '{
  "name": "local",
  "provider": "llama-cpp",
  "model": "~/.apollia/models/llama3.2-3B-q4_K_M.gguf",
  "config": {"n_gpu_layers": 35}
}'
```

`n_gpu_layers` contrôle combien de couches du modèle sont chargées sur GPU (Metal/CUDA). Mettre `0` = CPU pur. Mettre une valeur élevée (35+) = GPU maximum.

### Fail-fast sur les mauvaises configurations

Si vous configurez `device = metal` mais que le binaire a été compilé sans `--features local-metal`, Apollia OS refuse de démarrer avec un message clair :

```
ERREUR: Backend 'local' — device 'metal' non disponible.
       Recompilez avec: cargo build --features local-metal
```

C'est le principe #4 : les erreurs détectables au démarrage sont détectées au démarrage.

---

## Backends cloud

### Anthropic (Claude)

```bash
export ANTHROPIC_API_KEY="sk-ant-..."

apollia-os llm add \
  --name anthropic \
  --provider anthropic \
  --model claude-haiku-4-5-20251001 \
  --api-key-env ANTHROPIC_API_KEY \
  --set-default
```

La clé API est lue depuis la variable d'environnement au démarrage — jamais stockée en clair dans `system.db`.

Modèles Anthropic courants :

| Modèle | Usage recommandé |
|---|---|
| `claude-haiku-4-5-20251001` | Tâches rapides, résumés, classification |
| `claude-sonnet-4-6` | Raisonnement complexe, agents autonomes |
| `claude-opus-4-6` | Analyse profonde, tâches critiques |

### OpenAI

```bash
export OPENAI_API_KEY="sk-..."

apollia-os llm add \
  --name openai \
  --provider openai \
  --model gpt-4o-mini \
  --api-key-env OPENAI_API_KEY
```

### Ollama (local via API)

Ollama fait tourner des modèles localement via une API HTTP compatible OpenAI — pas besoin de feature flags de compilation.

```bash
# Prérequis : Ollama installé et en cours d'exécution
# ollama serve && ollama pull llama3.2

apollia-os llm add \
  --name ollama-llama \
  --provider ollama \
  --model llama3.2 \
  --base-url http://localhost:11434
```

---

## Multi-backend — router par agent

Si vous avez plusieurs backends configurés, chaque agent peut choisir lequel utiliser via `llm_backend` dans son manifest :

```python
def manifest(self):
    return {
        "name": "file-assistant",
        "llm_backend": "local",   # utilise le backend nommé "local"
        # llm_backend: None → backend par défaut du runtime
        ...
    }
```

Si le backend nommé n'existe pas ou n'est pas disponible, le runtime émet un `WARN` et utilise le défaut — jamais d'erreur fatale pour l'agent.

Cas d'usage typiques :

| Agent | Backend | Raison |
|---|---|---|
| Agent de résumé haute fréquence | `local` | Zéro coût, latence maîtrisée |
| Agent d'analyse de contrats | `anthropic/claude-opus` | Raisonnement profond requis |
| Agent de classification | `ollama-llama` | Local, modèle léger |
| Agent généraliste | *(défaut)* | Équilibre coût/qualité |

---

## Suivi des coûts

Chaque appel LLM est persisté dans `~/.apollia/llm_calls.db`. Consultez les coûts cumulés :

```bash
$ apollia-os llm costs --since 2026-03-01
  BACKEND      MODÈLE                      APPELS   TOKENS    COÛT USD
  anthropic    claude-haiku-4-5-20251001   342      847 234   $1.27
  local        llama3.2-3B-q4_K_M.gguf    89       203 441   $0.00
  TOTAL                                   431      1 050 675  $1.27
```

Pour les backends locaux, `cost_usd` est toujours `$0.00`. Pour les backends cloud, le coût est calculé à partir de la table de prix compilée dans le binaire.

> Si `observability.debug_log_prompt = true` dans `apollia.toml`, les prompts complets sont enregistrés dans `llm_calls.db`. **Ne jamais activer en production** — les prompts peuvent contenir des données sensibles.
