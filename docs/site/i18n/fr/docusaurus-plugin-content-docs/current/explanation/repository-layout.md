---
sidebar_position: 7
title: Structure du dépôt
---

# Structure du dépôt

Cette page explique le rôle de chaque fichier et répertoire de premier niveau
du dépôt Apollia OS, afin qu'un nouvel arrivant puisse s'orienter rapidement
et qu'un mainteneur n'ait jamais à deviner ce que fait un fichier racine
inconnu. Elle est descriptive : elle indique ce qu'est chaque entrée, pas
comment la modifier.

Au cœur d'Apollia OS se trouve un espace de travail Cargo de crates Rust,
entouré d'un SDK Python, de clients d'API générés, d'agents d'exemple, d'un
site de documentation, et de l'échafaudage habituel de build, de qualité et
de gouvernance. Les sections ci-dessous regroupent les entrées racine par
rôle.

## Répertoires source

| Chemin | Ce que c'est |
|---|---|
| `crates/` | L'espace de travail Rust. Chaque sous-répertoire est une crate avec une responsabilité unique. Les membres et leurs versions de dépendances partagées sont déclarés dans le `Cargo.toml` racine. |
| `sdk/` | Le SDK Python (paquet `apollia`, « AgentKit ») : les décorateurs `@agent` / `@skill`, le contrat de type `Ctx`, et l'interface minimale à duck-typing qu'un agent implémente. Empaqueté avec `pyproject.toml`. C'est le kit pour *écrire* des agents qui s'exécutent *à l'intérieur* du runtime. |
| `agents/` | Agents fournis avec le projet. `agents/system/` contient les agents livrés avec Apollia (l'agent d'onboarding, le guide intégré à l'application) ; `agents/examples/` contient un exemple minimal `hello`. À considérer comme des illustrations, pas des modèles canoniques. |
| `clients/` | Des clients d'API typés qu'une *application hôte* utilise pour *piloter* un runtime Apollia en cours d'exécution via son API HTTP (soumettre des tâches, streamer des résultats, lire le journal d'audit). Les clients TypeScript (`clients/ts`) et Python (`clients/python`) sont générés à partir de `clients/openapi.json` via `clients/regen.sh`, de sorte qu'ils ne peuvent pas diverger du contrat de communication. C'est la direction opposée de `sdk/`. |
| `tests/` | La crate de tests d'intégration et de bout en bout au niveau de l'espace de travail (`apollia-e2e-tests`), ainsi que la suite CLI (`tests/cli/`) qui exerce le runtime à travers les frontières des crates. Les tests unitaires par crate vivent quant à eux à l'intérieur de chaque crate. |
| `fuzz/` | La crate `cargo-fuzz` : les cibles libFuzzer (`fuzz/fuzz_targets/`) et leurs corpus (`fuzz/seeds/`). Compilée à part avec une chaîne d'outils nightly et exclue de l'espace de travail stable, de sorte que les contrôles habituels n'essaient jamais de la compiler. |
| `packaging/` | Scripts et manifestes qui assemblent un runtime Python autonome et déplaçable à embarquer avec l'application (récupérer un interpréteur autonome, construire le bundle universel, figer `requirements-bundled.txt`), plus les lanceurs par plateforme. Sert le principe de zéro dépendance externe. |
| `scripts/` | Scripts d'assistance pour le développement et la CI : wrappers de build (`build.sh`, `build.ps1`), utilitaires de réinitialisation des données/caches, le harnais d'évaluation de modèles (`model-eval/`), et l'automate de bout en bout du desktop (`automation/`). |
| `docs/` | Toute la documentation (détaillée ci-dessous). |

## Documentation (`docs/`)

| Chemin | Ce que c'est |
|---|---|
| `docs/site/` | Le site de documentation public (Docusaurus, anglais + français, structure Diataxis). Cette page se trouve ici. |
| `docs/agents/` | Le corpus de règles détaillé pour les contributeurs (humains et LLM) : patterns de code, nomenclature, tests, sécurité, et la liste des pratiques interdites. Anglais uniquement. |
| `docs/internal/` | Planification des releases et notes internes. Ignoré par git et jamais livré, donc tout chemin sous ce répertoire mentionné dans un enregistrement de décision n'est qu'une provenance, pas quelque chose que vous pouvez ouvrir. |

## L'espace de travail Rust (`crates/`)

L'espace de travail est défini dans le `Cargo.toml` racine. Le build par
défaut exclut `apollia-desktop` (une dépendance Tauri lourde, construite
explicitement) et deux crates construites isolément sous des flags spéciaux
(`apollia-loom-models` et la crate `fuzz` à la racine). Les crates
principales :

| Crate | Responsabilité |
|---|---|
| `apollia-core` | Types partagés et contrat public utilisé par toutes les autres crates. |
| `apollia-runtime` | Le bus d'événements, le superviseur d'acteurs Tokio, et l'API HTTP axum (socket Unix + TCP 7771). |
| `apollia-oria` | Le moteur d'exécution ORIA : classifier, planifier, contrôler, exécuter, vérifier, avec le `StepBudget` imposé par le runtime. |
| `apollia-aip` | Le pont PyO3 qui exécute les agents Python in-process (`Bound<'py, T>`, `pyo3-async-runtimes`). |
| `apollia-llm` | Routage des LLM locaux et cloud (le llama-server embarqué, basé sur llama.cpp amont, pour l'inférence GGUF locale ; les backends cloud compatibles OpenAI et Anthropic derrière un feature flag). |
| `apollia-runner` | Le sidecar runner hors-processus pour la reconnaissance vocale (whisper). L'inférence LLM locale ne passe plus par ici : elle transite désormais par le llama-server embarqué. |
| `apollia-stt` | Reconnaissance vocale locale (whisper). |
| `apollia-mcp` | Le client Model Context Protocol (transports stdio / HTTP / SSE, plafonds de taille sur les réponses non fiables). |
| `apollia-tools` | Implémentations des outils intégrés. |
| `apollia-memory` | Persistance de la mémoire des agents (SQLite + FTS5). |
| `apollia-permissions` | Les règles de permission persistées, le garde-fou de l'exécuteur de code, et le journal des décisions. |
| `apollia-auth` | Authentification et OAuth2 (PKCE), stockage des secrets via le trousseau du système d'exploitation. |
| `apollia-connectors` | Connecteurs vers des services externes. |
| `apollia-triggers` | Déclencheurs planifiés et événementiels (cron, surveillance du système de fichiers). |
| `apollia-notifications` | Livraison des notifications. |
| `apollia-workspace` | Collecte du contexte de l'espace de travail : état Git, règles `APOLLIA.md`, arborescence de fichiers, conventions, avec un cache à durée de vie limitée (TTL). Pas de sandboxing : le confinement de chemins vit dans `apollia-tools`. |
| `apollia-prompts` | Templates de prompts centralisés. |
| `apollia-eval` | Harnais d'évaluation des agents. |
| `apollia-cli` | Le binaire en ligne de commande `apollia` (clap, schéma nom-verbe, codes de sortie 0-5). La seule crate où `anyhow` et une sortie stdout destinée à l'utilisateur sont autorisés. |
| `apollia-desktop` | L'application desktop Tauri v2 + Svelte 5 (son interface vit dans `crates/apollia-desktop/ui/`). |

## Fichiers de gouvernance et communautaires

| Fichier | Ce que c'est |
|---|---|
| `README.md` | Vue d'ensemble du projet et point d'entrée. |
| `AGENTS.md` | Le point d'entrée standard pour les assistants de code LLM ; oriente vers le corpus de règles `docs/agents/`. |
| `CONTRIBUTING.md` | Comment contribuer (workflow, attentes). |
| `.github/CODE_OF_CONDUCT.md` | Normes de conduite de la communauté. GitHub le lit depuis `.github/` comme depuis la racine. |
| `GOVERNANCE.md` | Comment les décisions sont prises et qui maintient le projet. |
| `SECURITY.md` | Comment signaler une vulnérabilité, et la politique de versions supportées. |
| `SPONSORS.md` | Informations sur le financement et le sponsoring. |
| `CHANGELOG.md` | Registre lisible des changements notables par release. |

## Licences

Apollia OS est sous double licence **MIT ou Apache-2.0, au choix**. C'est le
standard de facto dans l'écosystème Rust (rustc, tokio, serde, axum, Tauri),
et cela maximise la compatibilité en aval : chaque utilisateur choisit la
licence qui convient à ses besoins.

| Fichier | Ce que c'est |
|---|---|
| `LICENSE` | Un court index qui énonce la double licence et pointe vers les deux textes complets. Il confirme aussi que les contributions sont soumises à la même double licence. Ce n'est pas lui-même un texte de licence. |
| `LICENSE-APACHE` | Le texte complet de l'Apache License 2.0. |
| `LICENSE-MIT` | Le texte complet de la MIT License. |

## Configuration de build et de qualité

Chaque fichier ci-dessous est lu par un outil spécifique. Les commentaires
décrivent ce qu'il configure réellement dans ce dépôt.

| Fichier | Ce qu'il configure |
|---|---|
| `Cargo.toml` | La racine de l'espace de travail : les crates membres, les versions de dépendances partagées (chaque crate utilise `{ workspace = true }` plutôt que des versions inline), les lints de l'espace de travail (`unsafe_code = "deny"`, `unwrap_used = "deny"`), et les profils release/dev. |
| `Cargo.lock` | Le graphe de dépendances résolu de manière exacte. Versionné pour que chaque build et chaque run de CI compile des versions identiques. |
| `rust-toolchain.toml` | Fige la chaîne d'outils de build sur Rust `1.95.0` avec `rustfmt`, `clippy`, `rust-src`, et `rust-analyzer`, de sorte que la sortie locale et celle de la CI correspondent octet pour octet. Le plancher MSRV déclaré (`rust-version = 1.89` dans `Cargo.toml`) est plus bas et distinct. |
| `clippy.toml` | Seuils Clippy : MSRV `1.89`, complexité cognitive `30`, complexité de type `250`, au plus `5` arguments par fonction, et une limite de `800` lignes par fonction. |
| `rustfmt.toml` | Formatage : édition 2021, `max_width = 100`, imports réordonnés, raccourcis field-init et try, fins de ligne Unix. |
| `deny.toml` | Politique `cargo-deny` : une liste blanche de licences acceptables, le refus des registres inconnus, un avertissement sur les versions de dépendances dupliquées, et une liste documentée, par release, des avis de sécurité ignorés (chacun avec sa condition de levée). |
| `Cross.toml` | Cross-compilation `cross-rs` : les étapes `pre-build` qui installent les bibliothèques système Linux (ALSA, PulseAudio, JACK, CMake, clang) dans le conteneur de build pour les cibles GNU/Linux x86_64 et aarch64. |
| `sonar-project.properties` | Configuration d'analyse SonarQube (build communautaire local, ne fait pas partie de la release publique) : racines des sources et des tests, exclusions, chemin du rapport Clippy importé, et exemptions documentées par règle. |
| `justfile` | Recettes de tâches `just` : les commandes canoniques pour construire, tester, linter, et exécuter l'automate desktop. |
| `.pre-commit-config.yaml` | La liste des hooks, et le fichier à lire pour la connaître plutôt qu'une copie de celle-ci : hygiène des fichiers et détection de secrets, `ruff` sur `sdk/`, `rustfmt` et `cargo check` sur l'espace de travail, neuf des gardes du dépôt sous `scripts/` (dont les règles de prose), et le build du site de documentation. Deux entrées tournent en dehors du commit lui-même : `clippy` au push, et la convention de message de commit à `commit-msg`. À ne jamais contourner. |
| `.editorconfig` | Règles d'espacement indépendantes de l'éditeur : fins de ligne LF, ligne finale vide, et tailles d'indentation par langage (4 pour Rust/Python/TOML, 2 pour le web et Markdown). |
| `.python-version` | Fige l'interpréteur Python local (`3.13.7`) pour des outils comme `pyenv`. |
| `.mailmap` | Canonicalise le nom et l'email des contributeurs à travers l'historique git. |
| `.cargo/` | Réglages par défaut de Cargo : `config.toml` fixe l'environnement de build (cible de déploiement macOS, un profil dev accéléré) et `audit.toml` contient les avis ignorés par `cargo-audit` (reflet de `deny.toml`). |
| `.github/` | Configuration GitHub : les workflows CI, CodeQL, nightly, et de release (`.github/workflows/`), plus `CODEOWNERS`, les templates d'issue et de PR, `dependabot.yml`, et `FUNDING.yml`. |
| `.gitignore` | Chemins que git ne suit pas (voir ci-dessous). |

## Entrées locales uniquement (ignorées par git, jamais livrées)

Ces entrées apparaissent dans une copie de travail mais ne sont pas suivies,
donc elles n'atteignent jamais le dépôt public ni un artefact de release.
Leur présence dépend de ce que vous avez exécuté localement.

| Chemin | Ce que c'est |
|---|---|
| `docs/internal/` | Planification des releases et notes internes. |
| `target/` | Le répertoire de sortie de build Cargo. |
| `.venv/`, `.venv-agents/` | Environnements virtuels Python locaux. |
| `.apollia-automation/`, `.apollia-seed-home/` | État jetable produit par l'automate de bout en bout du desktop, qui exécute l'application sur un `HOME` ensemencé et jetable (voir la note ci-dessous). |
| `.pytest_cache/`, `.ruff_cache/` | Caches d'outils. |
| `.DS_Store` | Métadonnées du Finder macOS. |

`AGENTS.local.md` et `AGENTS.override.md` sont eux aussi ignorés par git par
convention (surcharges de contributeur par machine et par session) ; ils
peuvent ne pas exister dans une copie de travail donnée.

**Ce que le `HOME` ensemencé couvre.** Les deux recettes qui le construisent,
`desktop-dev-automation-seeded` et `desktop-dev-automation-seeded-llama`,
remplacent `HOME` par la copie jetable avant de lancer l'application, de sorte
que l'application testée ne lit et n'écrit que cette copie, jamais le profil
`~/.apollia` réel. Les recettes atteignent quand même le répertoire personnel
réel en deux endroits. Toutes deux y résolvent `CARGO_HOME` et `RUSTUP_HOME`
(`~/.cargo`, `~/.rustup`) avant le remplacement, pour que la compilation
utilise la chaîne d'outils et le cache de crates déjà présents plutôt qu'un
cache vide sous la graine. Et la recette `-llama`, celle qui pilote un vrai
modèle, lit un fichier du profil réel, le GGUF du modèle sous
`~/.apollia/models/`, parce que la graine porte des GGUF de remplacement et non
un modèle exécutable. Ces deux points sont écrits dans les recettes elles-mêmes,
dans le `justfile`.
