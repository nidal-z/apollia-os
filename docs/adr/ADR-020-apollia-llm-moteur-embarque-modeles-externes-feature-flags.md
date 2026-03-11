# ADR-020 — apollia-llm : moteur d'inférence embarqué, modèles fichiers externes, feature flags

**Date :** 2026-03-08
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Sprint :** 8

---

## Contexte

Sprint 8 introduit la capacité LLM native dans Apollia OS : un agent Python doit pouvoir appeler
`ctx.llm.chat()` ou `ctx.llm.run_tools()` sans aucun service externe obligatoire.

Trois contraintes non-négociables encadrent la décision :

1. **Principe #1 — Local-first** : l'inférence doit fonctionner offline, sans API key, sans cloud.
2. **Principe #2 — Zéro dépendance opérationnelle** : `apollia-os start` ne peut pas supposer qu'un
   daemon tiers (ollama, llama.cpp-server, vLLM) est déjà lancé sur la machine.
3. **Principe #4 — Fail fast** : si le modèle est absent ou corrompu, l'erreur doit être signalée au
   démarrage, pas à la première inférence.

Simultaneously, certains utilisateurs préfèrent déléguer l'inférence à un backend cloud
(Anthropic, OpenAI). La solution doit couvrir les deux cas sans imposer la compilation du
moteur d'inférence à ceux qui n'en ont pas besoin (le moteur ajoute ~50–200 Mo au binaire).

## Décision

Nous introduisons le crate `apollia-llm` avec **deux feature flags Cargo exclusifs** :

- `cloud` (défaut) — compile les clients HTTP `AnthropicClient` et `OpenAICompatibleClient`
  via `async-openai` + `reqwest`. Aucun moteur d'inférence, binaire léger.
- `local` — compile en plus `EmbeddedBackend` via `mistral-rs-core` (inférence GGUF in-process).

Le modèle (`.gguf`) n'est **jamais embarqué dans le binaire**. Il réside dans
`~/.apollia/models/` comme un fichier de données externe, chargeable à chaud — exactement comme
une base SQLite (cf. ADR-002). Le moteur d'inférence (le code C++/Rust qui fait le calcul) est
lui compilé statiquement dans le binaire quand `feature = "local"`.

`LlmRouter` dispatche vers le bon backend au runtime selon la config `apollia.toml`. Si une clé
API cloud est absente, le backend est ignoré avec un `tracing::warn!` — pas de crash. Si aucun
backend n'est disponible, `ctx.llm` est `None` et l'agent passe en `DEGRADED`.

## Alternatives considérées

### Option A — Daemon externe géré par le Supervisor (rejetée)

Lancer un processus `llama.cpp-server` ou `ollama serve` comme enfant du Supervisor Apollia.
Le Supervisor surveille le PID, le relance si nécessaire, et le runtime communique en HTTP local.

**Pour :**
- Découplage total runtime / moteur d'inférence.
- Facilité de mise à jour du moteur sans recompiler `apollia-os`.
- `llama.cpp-server` est très mature.

**Contre :**
- Viole le Principe #2 : suppose que `llama.cpp` ou `ollama` est installé sur la machine.
- STORY-054 (`LlmBackendManager`) était l'implémentation prévue de cette option — sa suppression
  de la spec confirme le rejet.
- Latence HTTP interne pour chaque token (overhead pour les appels fréquents).
- Gestion PID complexe : race conditions au démarrage, zombie processes, port conflicts.
- Pas de déploiement "single binary" réel : l'utilisateur doit installer et gérer deux binaires.

### Option B — Modèle GGUF embarqué dans le binaire (rejetée)

Compiler le fichier `.gguf` directement dans le binaire via `include_bytes!()` ou un build
script, pour un binaire vraiment auto-suffisant.

**Pour :**
- Copier un seul fichier suffit pour distribuer runtime + modèle.
- Aucune gestion de `~/.apollia/models/`.

**Contre :**
- Un modèle quantifié 4-bit minimal (Llama 3.2 3B) pèse ~2 Go — binaire inutilisable.
- Impossible de changer de modèle sans recompiler l'intégralité du runtime.
- Viole Principe #2 dans l'esprit : distribution impossible sans connexion initiale pour
  télécharger le binaire (~2 Go).
- Pas de hot-reload de modèle à froid.

### Option retenue — Moteur in-process compilé, modèle fichier externe, feature flags

**Pour :**
- Principe #2 respecté : `apollia-os` (feature `cloud`) est un binaire léger sans moteur.
- Principe #1 respecté : feature `local` active l'inférence offline complète.
- Cohérence avec ADR-002 (SQLite) : le modèle est un fichier de données, pas du code.
- `mistral-rs-core` fournit l'inférence GGUF in-process sans subprocess, sans HTTP interne.
- Feature flags Cargo standard : pas de magie, le CI contrôle ce qui est compilé.
- Hot-reload possible : `apollia-os model load` peut charger un nouveau `.gguf` sans redémarrage.

**Compromis acceptés :**
- Le binaire `feature = "local"` est plus lourd (mistral-rs-core compile du CUDA/Metal/CPU
  optionnellement).
- `mistral-rs-core` est une dépendance nouvelle dans le workspace (version 0.7).
- L'utilisateur doit télécharger le `.gguf` séparément (`apollia-os model download` en roadmap).
- Le build `local-metal` nécessite soit Xcode complet, soit `MISTRALRS_METAL_PRECOMPILE=0`
  (shaders Metal compilés JIT au lieu d'être précompilés, sans impact sur les performances).

## Conséquences

**Positives :**
- `apollia-os start` avec un `.gguf` valide → inférence locale zéro-latence réseau.
- `apollia-os start` sans `.gguf` → warning + backend `local` ignoré, runtime continue.
- `ctx.llm` est disponible pour tous les agents sans modifier leur contrat (`manifest()` / `run()`).
- Observabilité native : `LlmCallCompleted` sur EventBus après chaque appel (tokens, latence, coût).
- La boucle ReAct (`run_tools()`) intègre `StepBudget` — garde-fou Principe #7 respecté.

**Négatives / Compromis :**
- Deux binaires de distribution à tester en CI (`cloud` + `local`).
- `mistral-rs-core` augmente le temps de compilation (dépendance C++ sous-jacente).
- L'utilisateur `feature = "local"` doit gérer ses `.gguf` dans `~/.apollia/models/`.

**Neutres / À surveiller :**
- Stabilité de l'API `mistral-rs-core 0.7` — surveiller les breaking changes.
- Performance de `EmbeddedBackend` sur macOS ARM vs Linux x86 (pas de CI Mac).
- Migration future vers `candle` (Hugging Face, pure Rust) si `mistral-rs-core` stagne.

**Mise à jour (2026-03-11) — Metal fonctionnel :**
- `objc2-metal 0.3.2` et `candle-metal-kernels 0.9.2` sont désormais publiés sur crates.io.
- `local-metal = ["local", "mistralrs/metal", "mistralrs-core/metal"]` est activé.
- `GgufModelBuilder::with_device(Device::new_metal(0))` est utilisé dans `EmbeddedBackend::load()`.
- Le blocker "objc2-metal absent" mentionné dans les sprints 8/9 est clos.
- Seule contrainte résiduelle : le build `local-metal` nécessite Xcode complet **ou** `MISTRALRS_METAL_PRECOMPILE=0` (Command Line Tools suffisent dans ce cas).

## Principes architecturaux impactés

- **Principe #1 — Local-first** : `feature = "local"` offre une inférence 100% offline,
  zéro donnée vers le cloud sans action explicite de l'utilisateur.
- **Principe #2 — Zéro dépendance opérationnelle** : aucun daemon externe requis. Le moteur
  est compilé dans le binaire.
- **Principe #4 — Fail fast** : modèle absent → `LlmError::ModelNotFound` au démarrage du
  `LlmRouter`, avant toute tâche agent.
- **Principe #5 — Un acteur, une responsabilité** : `LlmRouter` est un struct ordinaire
  (pas un acteur Tokio), car il n'a pas d'état mutable concurrent — cohérent avec le pattern.
- **Principe #7 — Garde-fous non-négociables** : `run_tools()` consulte `StepBudget.is_exhausted()`
  à chaque itération ReAct — le runtime contrôle la boucle, pas l'agent.

## Liens

- Stories associées : STORY-051 → STORY-064 (Sprint 8)
- ADR précédent lié : ADR-001 — Rust comme langage du runtime (mistral-rs-core est une crate Rust, cohérent)
- ADR précédent lié : ADR-010 — Pivot local-first (l'inférence embarquée concrétise le principe fondateur)
- ADR précédent lié : ADR-002 — SQLite comme fichier de données (même pattern : code compilé, données externes)
