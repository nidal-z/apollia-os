---
sidebar_position: 7
title: Installer et lancer le runtime
description: "Compiler Apollia OS depuis les sources et lancer le runtime : prérequis, compilation, premier démarrage, et comment vérifier qu'il sert vraiment."
---

# Installer et lancer le runtime

Apollia ne publie encore aucun paquet sur crates.io ni sur PyPI, donc vous le
construisez depuis un clone. Ce guide s'adresse aux développeurs. Il vous mène
d'un clone à un daemon fonctionnel capable d'exécuter un agent, puis à
l'application desktop Tauri en mode développement, sur macOS, Linux ou
Windows. Prévoyez une première compilation plus longue (Rust compile
l'ensemble du workspace une fois), les reconstructions suivantes sont
ensuite incrémentales.

Si vous voulez seulement utiliser l'application desktop terminée, téléchargez
plutôt un installeur préconstruit : [Installer l'application
desktop](/how-to/install-the-desktop-app).

Chaque commande s'exécute depuis la racine du dépôt, sauf mention contraire.

## Prérequis

Le runtime a besoin d'une chaîne d'outils Rust, de Python et de Git sur
toutes les plateformes. L'application desktop ajoute Node.js, le CLI `cargo
tauri`, et une chaîne d'outils webview propre à chaque OS. Installez d'abord
les outils communs, puis la section spécifique à votre plateforme.

### Commun (toutes plateformes)

- **Chaîne d'outils Rust (stable).** Installez-la depuis
  [rustup.rs](https://rustup.rs) si vous n'avez pas `cargo`. Le dépôt fixe une
  version exacte du compilateur dans `rust-toolchain.toml` ; `rustup` lit ce
  fichier et installe automatiquement la chaîne d'outils correspondante à la
  première compilation dans le clone, vous n'avez donc pas à choisir de
  version manuellement.
- **Python 3.13**, disponible en tant que `python3`. Le runtime embarque
  Python pour charger les agents, et le SDK s'y installe. Le clone déclare la
  version exacte dans `.python-version`.
- **Git.** Nécessaire pour cloner le dépôt et, plus tard, pour installer des
  agents depuis une URL Git.
- **Pour l'application desktop uniquement : Node.js 20 ou plus récent** (le
  projet construit l'interface avec Node 22) avec `npm`, ainsi que le CLI
  `cargo tauri` :

  ```sh
  cargo install tauri-cli --version "^2"
  ```

- **Pour l'inférence locale :** le daemon sert les modèles GGUF locaux via un
  `llama-server` embarqué. Une build packagée l'inclut déjà ; sur une build
  depuis les sources, il vous faut `llama-server` sur votre `PATH` (voir la
  dernière section), sans compilateur requis. Construire depuis les sources
  l'exécuteur optionnel de reconnaissance vocale nécessite en plus CMake et un
  compilateur C/C++.

### macOS

- **Xcode Command Line Tools**, pour la chaîne d'outils C/C++ et le webview
  WebKit dans lequel s'affiche l'application desktop :

  ```sh
  xcode-select --install
  ```

- **PyO3 doit trouver le bon interpréteur au moment de la compilation.** Si le
  `python3` par défaut n'est pas celui que vous voulez, indiquez-le
  explicitement avant de compiler. Avec Python de Homebrew :

  ```sh
  export PYO3_PYTHON=$(brew --prefix python@3.13)/bin/python3.13
  ```

- Construire depuis les sources l'exécuteur optionnel de reconnaissance
  vocale nécessite CMake (`brew install cmake`) ; le compilateur vient des
  Command Line Tools ci-dessus.
- L'application desktop requiert macOS 13 (Ventura) ou plus récent.

### Linux (Debian / Ubuntu)

Installez la chaîne d'outils de compilation ainsi que le webview Tauri v2 et
les bibliothèques système. Sur Debian et Ubuntu :

```sh
sudo apt-get update
sudo apt-get install -y \
  build-essential pkg-config libssl-dev \
  libgtk-3-dev libwebkit2gtk-4.1-dev \
  libayatana-appindicator3-dev librsvg2-dev \
  libsoup-3.0-dev libjavascriptcoregtk-4.1-dev \
  libasound2-dev libpulse-dev libjack-jackd2-dev \
  python3-dev clang cmake file
```

À quoi sert chaque groupe :

- `build-essential pkg-config libssl-dev` : chaîne d'outils C/C++ et édition
  de liens.
- `libgtk-3-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev
  librsvg2-dev libsoup-3.0-dev libjavascriptcoregtk-4.1-dev` : les
  dépendances du webview et de la zone de notification de Tauri v2.
  Nécessaire uniquement pour l'application desktop.
- `libasound2-dev libpulse-dev libjack-jackd2-dev` : en-têtes audio pour la
  capture de reconnaissance vocale de l'application desktop. Nécessaire
  uniquement pour l'application desktop.
- `python3-dev` : en-têtes pour le Python embarqué.
- `clang cmake` : nécessaires seulement si vous construisez l'exécuteur de
  reconnaissance vocale depuis les sources.

Si vous construisez uniquement le runtime en ligne de commande, sans
l'application desktop, vous pouvez sauter les groupes webview et audio et
installer seulement `build-essential pkg-config libssl-dev python3-dev`
(plus `clang cmake` pour l'exécuteur de reconnaissance vocale).

Sur d'autres distributions, installez les équivalents de ces mêmes
bibliothèques. Les noms de paquets diffèrent (par exemple WebKitGTK 4.1, GTK
3, libayatana-appindicator, et les paquets de développement librsvg) ;
vérifiez les noms exacts pour votre distribution dans les [prérequis Tauri
v2](https://v2.tauri.app/start/prerequisites/).

### Windows

- **Microsoft C++ Build Tools** (le workload "Desktop development with C++"
  de l'installeur Visual Studio Build Tools), pour le compilateur et
  l'éditeur de liens MSVC.
- **Le runtime Microsoft Edge WebView2.** Il est préinstallé sur les Windows
  11 récents et les Windows 10 à jour. S'il manque, installez
  l'"Evergreen Bootstrapper" depuis la page de téléchargement du runtime
  WebView2 de Microsoft. Nécessaire uniquement pour l'application desktop.
- **CMake**, seulement si vous construisez l'exécuteur de reconnaissance
  vocale depuis les sources.
- **LLVM** (fournit `libclang.dll` pour bindgen), seulement si vous
  construisez l'exécuteur de reconnaissance vocale depuis les sources.
  Installez-le avec `winget install LLVM.LLVM`, puis pointez bindgen dessus
  avant de compiler :

  ```powershell
  $env:LIBCLANG_PATH = "C:\Program Files\LLVM\bin"
  $env:CMAKE_MSVC_RUNTIME_LIBRARY = "MultiThreaded"
  ```

  Sans `LIBCLANG_PATH`, la compilation de `whisper-rs-sys` échoue avec
  `Unable to find libclang`.
- Exécutez les commandes ci-dessous depuis un shell où `cargo`, `python`,
  `git`, et (pour l'application desktop) `npm` sont sur le `PATH`. Les
  primitives du runtime sont testées sur macOS et Linux ; sur Windows,
  vérifiez les commandes du daemon sur votre machine et préférez un
  PowerShell développeur avec l'environnement MSVC chargé.

## Étape 1 : cloner et compiler le daemon

```sh
git clone https://github.com/Apollia-OS/apollia-os.git
cd apollia-os
cargo build -p apollia-cli
```

Compilez spécifiquement la crate `apollia-cli`. Une compilation de tout le
workspace (`cargo build --workspace`) embarque aussi la crate desktop, qui
nécessite la chaîne d'outils webview complète décrite dans les sections par
plateforme ci-dessus ; limitez la compilation à `apollia-cli` pour ne
construire que le runtime.

La crate s'appelle `apollia-cli` mais le binaire qu'elle produit s'appelle
`apollia-os`, à l'emplacement `target/debug/apollia-os`. Ce choix de nom est
délibéré ; ne cherchez pas un fichier nommé `apollia-cli`. Mettez le binaire
sur votre `PATH` pour que la suite de ce guide puisse l'appeler par son nom :

```sh
export PATH="$PWD/target/debug:$PATH"
```

Sur Windows, le binaire est `target\debug\apollia-os.exe` ; ajoutez
`target\debug` à votre `PATH` de façon équivalente.

Cette compilation dialogue avec les backends cloud Anthropic, compatibles
OpenAI, ou Vertex, et sert les modèles GGUF locaux via le `llama-server`
embarqué. Sur une build depuis les sources, ce moteur doit se trouver sur
votre `PATH`, ce qui est couvert dans la dernière section.

## Étape 2 : installer le SDK

Les agents sont écrits en Python. Installez le paquet `apollia` en mode
éditable dans le même interpréteur que celui utilisé par le runtime.

Créez d'abord un environnement virtuel. Homebrew, Debian et Fedora
distribuent Python comme un environnement géré de manière externe (PEP 668) :
installer directement dedans s'arrête avec
`error: externally-managed-environment`, et le runtime charge les agents
depuis l'interpréteur qu'il trouve sur le `PATH`, donc l'environnement que
vous activez ici est celui qu'il utilisera.

```sh
python3 -m venv .venv
source .venv/bin/activate
pip install -e ./sdk
```

Sur Windows, activez-le avec `.venv\Scripts\activate` à la place.

Gardez cet environnement activé dans chaque terminal où vous lancez
`apollia-os`. Si vous préférez une installation système et en acceptez les
conséquences, `pip install --break-system-packages -e ./sdk` est l'échappatoire,
pas le chemin recommandé.

## Étape 3 : configurer un backend de modèle {#step-3-configure-a-model-backend}

Un agent qui génère du texte a besoin d'un backend. Choisissez un chemin.

`llm setup --local` écrit directement dans la base de données locale et
fonctionne hors ligne. Toute autre sous-commande `llm`, y compris `backends
create`, `reload` et `status`, dialogue avec le daemon. Si vous prenez le
chemin cloud, ou si vous voulez vérifier avec `llm status`, démarrez d'abord
le daemon à l'étape 4 puis revenez ici.

<!-- claim:cloud-llm-auth-is-api-key-only -->
Cloud : enregistrez un backend avec une clé d'API. C'est la seule façon
dont un fournisseur cloud s'authentifie ; il n'existe aucun flux OAuth pour
cela.

```sh
apollia-os llm backends create prod --provider anthropic \
  --model claude-sonnet-4-20250514 --api-key "$ANTHROPIC_API_KEY" --default
```

`--api-key` accepte aussi une forme `${VAR}`, résolue depuis l'environnement
au démarrage, de sorte que la clé n'a pas besoin de figurer dans
`apollia.toml`.

Utilisez l'identifiant de modèle actuel de votre fournisseur pour `--model` ;
la valeur ci-dessus n'est qu'un exemple.

Local : pointez le runtime vers un fichier `.gguf` sur votre machine. Cela
enregistre le backend ; le daemon le sert via le `llama-server` embarqué
(voir la dernière section pour le prérequis `llama-server`).

```sh
apollia-os llm setup --local --model /path/to/model.gguf
```

Dans les deux cas, rechargez le registre des backends et vérifiez que le
backend est visible. Un backend fraîchement configuré n'apparaît pas dans
`llm status` tant que vous ne l'avez pas rechargé :

```sh
apollia-os llm reload
apollia-os llm status
```

## Étape 4 : démarrer le daemon

```sh
apollia-os start --port 7771
```

<!-- claim:daemon-binds-tcp-by-default -->
À son premier lancement, le runtime crée son répertoire de données dans
`~/.apollia`. Il écoute sur un socket Unix (`~/.apollia/runtime.sock` par
défaut, passé en `0600` après le bind) et sur `127.0.0.1:7771`. `apollia-os start` lie toujours le TCP ; `--port`
choisit le numéro, et l'omettre prend la valeur par défaut 7771 plutôt que de
laisser le port fermé. Au premier démarrage il écrit un
jeton d'API dans `~/.apollia/api-token` ; les appelants en TCP doivent le
présenter comme identifiant porteur, tandis que le socket Unix repose sur une
confiance locale et n'en a besoin d'aucun. Laissez ce terminal en cours
d'exécution et ouvrez-en un second pour les étapes suivantes.

## Étape 5 : exécuter un agent

Le dépôt fournit un agent `echo` sans LLM qui fonctionne sur n'importe quelle
machine. Installez-le, activez-le pour que le runtime le charge, puis
envoyez-lui une tâche :

```sh
apollia-os agent install clients/examples/echo_agent.py --skip-tests
apollia-os agent enable echo
apollia-os run echo "hello from Apollia"
```

Sans l'étape `enable`, `run` échoue avec `agent not found: echo` et une
indication listant la séquence installation, activation et chargement.

Vous devriez voir le résultat renvoyé en écho. Pour écrire votre propre agent
depuis zéro, suivez [Votre premier
agent](/tutorials/your-first-agent).

## Étape 6 : arrêter le daemon

```sh
apollia-os stop
```

## Lancer l'application desktop en mode développement

L'application desktop Tauri est l'interface graphique du même runtime. Pour
la faire tourner depuis les sources, installez d'abord une fois les
dépendances de l'interface, puis démarrez la build de développement. Cela
nécessite Node.js, le CLI `cargo tauri`, et les prérequis webview par OS
listés dans les sections ci-dessus.

Installez les dépendances de l'interface Svelte (une recette `just` encapsule
`npm ci`) :

```sh
just desktop-ui-install
# équivalent à : cd crates/apollia-desktop/ui && npm ci
```

Démarrez l'application en mode développement. Cela lance le serveur de
développement Vite pour l'interface et le shell Tauri avec rechargement à
chaud :

```sh
just desktop-dev
# équivalent à : cd crates/apollia-desktop && cargo tauri dev
```

Le premier `cargo tauri dev` compile la crate desktop et peut prendre du
temps ; les lancements suivants sont incrémentaux. La fenêtre utilise le
webview du système (WebKit sur macOS et Linux, WebView2 sur Windows), assurez-vous
donc que les prérequis webview de votre plateforme sont installés.

Pour l'inférence locale dans l'application en mode développement, assurez-vous
que `llama-server` se trouve sur votre `PATH` (section suivante) ; le daemon
embarqué par l'application le sert pour les modèles GGUF locaux. Voir [Tirer
le meilleur parti de l'inférence
locale](/how-to/accelerate-local-inference).

## Construire un bundle desktop de release

Les recettes `just` ci-dessous produisent un installeur desktop distribuable
(`.dmg` sur macOS, `.deb`/`.AppImage` sur Linux, `.msi`/`.exe` sur Windows).
Elles exécutent `bundle-cli.sh`, qui prépare le runtime Python, le CLI
`apollia-os`, les exécuteurs de reconnaissance vocale, et un binaire
`llama-server` figé.

Chaque recette accepte deux arguments optionnels :

| Argument | Rôle | Valeur par défaut (macOS / Linux / Windows) |
|---|---|---|
| `target` | Triplet Rust passé à `cargo tauri build` | `aarch64-apple-darwin` / `x86_64-unknown-linux-gnu` / `x86_64-pc-windows-msvc` |
| `runners` | Liste, séparée par des espaces, des backends d'exécuteurs à compiler et inclure dans le bundle | `cpu metal` / `cpu` / `cpu` |

La valeur `runners` contrôle deux choses :

1. Quels sidecars `apollia-runner-{backend}` sont compilés et copiés dans le
   bundle.
2. Quel artefact `llama-server` préconstruit est téléchargé. Le script
   choisit le premier backend GPU présent dans la liste (`metal`, `cuda`,
   `rocm`, ou `vulkan`) ; si aucun n'est présent, il retombe sur le CPU.

`cpu` est toujours inclus comme repli universel. Ajoutez un backend GPU
correspondant à votre matériel :

| Matériel | Valeur `runners` typique | Remarques |
|---|---|---|
| Apple Silicon | `cpu metal` | Préréglage par défaut pour macOS |
| NVIDIA (CUDA 12+) | `cpu cuda` | Sur Windows, LLM et STT utilisent tous deux CUDA. Sur Linux, la STT utilise CUDA tandis que le moteur LLM embarqué reste sur CPU : la release amont épinglée ne publie pas de `llama-server` CUDA Linux (compilez-en un et passez `LLAMA_SERVER_DIR` pour l'embarquer) |
| AMD Radeon / Intel Arc | `cpu vulkan` | LLM sur GPU ; STT reste sur CPU (`whisper-rs` n'a pas de backend Vulkan) |
| AMD Pro / Instinct + HIP SDK | `cpu rocm` | LLM et STT sur ROCm là où c'est pris en charge |

Préréglages par plateforme :

```sh
# macOS Apple Silicon, Metal + repli CPU (par défaut)
just release-macos

# Linux x86_64, CPU uniquement (par défaut)
just release-linux

# Windows x86_64, CPU uniquement (par défaut)
just release-windows
```

Remplacez la cible et/ou les exécuteurs sur n'importe quel préréglage :

```sh
# Windows avec Vulkan pour le LLM (AMD / Intel / repli NVIDIA)
just release-windows runners="cpu vulkan"

# Windows avec CUDA (NVIDIA)
just release-windows runners="cpu cuda"

# Linux avec Vulkan
just release-linux runners="cpu vulkan"

# macOS avec un jeu d'exécuteurs personnalisé
just release-macos runners="cpu metal"
```

Pour un triplet et un jeu d'exécuteurs non couverts par un préréglage,
utilisez la recette générique :

```sh
just release-desktop x86_64-pc-windows-msvc "cpu vulkan"
just release-desktop x86_64-unknown-linux-gnu "cpu rocm"
just release-desktop aarch64-apple-darwin "cpu metal"
```

Le moteur `llama-server` embarqué est récupéré depuis la release llama.cpp
amont épinglée, qui publie des builds pour macOS (arm64 et x86-64), Linux
x86-64 (CPU, Vulkan, ROCm), Linux arm64 (CPU) et Windows x86-64 (CPU, CUDA,
Vulkan). Pour tout autre couple, compilez llama.cpp vous-même et passez
`LLAMA_SERVER_DIR=<bin dir>` ; la recette embarque alors votre build.

Sur Windows, exportez `LIBCLANG_PATH` et `CMAKE_MSVC_RUNTIME_LIBRARY` dans le
même shell avant de lancer l'une de ces recettes (voir les prérequis Windows
ci-dessus). Sur Linux, l'exécuteur de reconnaissance vocale a en plus besoin
de `clang` et `cmake` dans votre gestionnaire de paquets.

Le bundle se retrouve sous `target/<triple>/release/bundle/` (par exemple
`target/x86_64-pc-windows-msvc/release/bundle/msi/` sur Windows).

## Inférence GGUF locale {#local-gguf-inference}

Les modèles locaux tournent via un `llama-server` embarqué (le projet
llama.cpp en amont) que le daemon lance et supervise à travers son API HTTP
compatible OpenAI, avec appel d'outils natif (`--jinja`) et batching continu.
Le nom du fournisseur reste `llama-cpp`.

Une build desktop packagée inclut automatiquement `llama-server`, aux côtés
des exécuteurs de reconnaissance vocale, donc rien n'est à faire de ce côté.
Sur une build depuis les sources, le daemon cherche `llama-server` sur votre
`PATH`. Fournissez l'une des options suivantes :

une installation en amont qui met `llama-server` sur votre `PATH`, par
exemple `brew install llama.cpp` sur macOS ou une compilation de llama.cpp
sur Linux.

Le dépôt propose aussi une recette `just llama-server`. Elle ne satisfait
**pas** ce prérequis : elle suppose que `llama-server` est déjà sur le `PATH`,
et démarre un serveur séparé sur le port 8899 auquel le daemon ne parle pas.
C'est un banc d'essai pour développeurs, pour comparer avec un serveur réglé
à la main, pas une étape d'installation.

Si un backend local est configuré mais qu'aucun `llama-server` n'est
joignable, les appels au LLM échouent avec un `503 Service Unavailable` et
une raison `BackendUnavailable` ; placez le moteur sur votre `PATH` pour
résoudre le problème.

Il n'existe aucune commande de téléchargement de modèles. Procurez-vous un
fichier `.gguf` vous-même (par exemple depuis un hub de modèles) et
placez-le dans `~/.apollia/models/`, puis pointez un backend local dessus
avec `apollia-os llm setup --local --model <path.gguf>`. Les sous-commandes
`model` (`list`, `search`, `show`, `hardware`, `delete`) inspectent et gèrent
les modèles déjà présents ; voir la [référence CLI](/reference/cli).

La reconnaissance vocale est un composant séparé, optionnel. Le sidecar
`apollia-runner`, construit avec une fonctionnalité `local-*`
(`local-metal`, `local-cpu`, `local-cuda`, `local-rocm`, `local-vulkan`),
exécute whisper hors processus ; il ne sert plus l'inférence LLM. Une build
packagée l'inclut, et depuis les sources vous ne le construisez que si vous
voulez la dictée locale.

## Étapes suivantes

- Écrire et exécuter votre propre agent : [Votre premier
  agent](/tutorials/your-first-agent).
- Faire tourner Apollia comme service géré : [Déployer en
  production](/how-to/deploy-in-production).
- Chaque option de chaque commande figure dans la [référence
  CLI](/reference/cli).
