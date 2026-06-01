# ADR-041 - Moteur STT embarqué : whisper-rs V1, trait SttBackend, roadmap candle-whisper/Voxtral

**Date :** 2026-03-25
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Sprint :** 24

---

## Contexte

Apollia OS doit transcrire la parole en texte localement pour permettre la dictée vocale dans
n'importe quelle application via hotkey globale. Le moteur STT doit respecter les mêmes
contraintes que le moteur LLM (ADR-020) :

1. **Principe #1 - Local-first** : aucun échantillon audio ne quitte la machine.
2. **Principe #2 - Zéro dépendance opérationnelle** : pas de service STT tiers (Whisper API,
   Google Speech, Deepgram) installé ou lancé sur la machine.
3. **Principe #4 - Fail fast** : modèle GGML absent → erreur au démarrage du `SttEngine`, pas
   à la première transcription.
4. **Principe #5 - Un acteur, une responsabilité** : STT et LLM sont des pipelines
   fondamentalement distincts (audio → spectrogramme → décodage séquentiel vs texte → texte).
   Les fusionner dans `apollia-llm` violerait ce principe.

Le modèle cible est `bofenghuang/whisper-large-v3-french` (fine-tuné 2200h FR, GGML Q5_0,
~900 Mo) pour un compromis FR+EN optimal. L'objectif de latence est < 2s sur M1 avec Metal
(RTF ~0.30x du modèle medium = 5s audio en ~1.5s).

Trois backends STT sont envisageables aujourd'hui. Le choix doit être fait en fonction de la
maturité, des performances Metal, et de l'absence de conflit de symboles avec le moteur LLM
existant (`mistral-rs-core` qui utilise candle, pas llama.cpp).

## Décision

Nous créons une crate dédiée `apollia-stt` avec :

1. **Un trait `SttBackend` object-safe** (`Send + Sync`) - contrat universel pour tout moteur
   STT. API synchrone (l'appelant wrappe dans `spawn_blocking`). Entrée : `&[f32]` PCM 16kHz
   mono. Sortie : `TranscriptResult` avec segments, timestamps, confiance, et langue détectée.

2. **Une implémentation V1 `WhisperCppBackend`** via `whisper-rs` 0.16 (bindings whisper.cpp) -
   compilée statiquement via CMake, feature-gated `stt-whisper-cpp` (défaut activé).

3. **Des feature flags Cargo** identiques au pattern ADR-020 :
   - `stt-whisper-cpp` (défaut) → compile `whisper-rs`
   - `stt-metal` → active `whisper-rs/metal`
   - `stt-cuda` → active `whisper-rs/cuda`
   - Les features workspace `metal` et `cuda` propagent vers `apollia-stt` en plus de `apollia-llm`

4. **Le modèle GGML comme fichier externe** dans `~/.apollia/models/`, cohérent avec ADR-002
   et ADR-020 (code compilé dans le binaire, données stockées comme fichiers).

## Alternatives considérées

### Option A - candle-whisper (pure Rust, Hugging Face) (rejetée pour V1)

**Pour :**
- Pure Rust, zéro dépendance CMake/C++.
- Même fondation que `mistral-rs-core` (candle) - cohérence écosystème.
- Pas de risque de conflit ggml futur.

**Contre :**
- Benchmarks Metal inférieurs à whisper.cpp en Q2 2026 (RTF ~0.50x vs ~0.30x).
- Maturité moindre : pas de release stable whisper-spécifique sur crates.io.
- Quantification GGML non supportée nativement (nécessite conversion → safetensors).

**Verdict :** Prévu pour V2 quand les benchmarks candle ≥ whisper.cpp sur Metal.

### Option B - Service STT cloud (Whisper API OpenAI, Google Speech, Deepgram) (rejetée)

**Pour :**
- Zéro build complexity, qualité state-of-the-art, multilingue natif.

**Contre :**
- Viole Principe #1 (local-first) : l'audio quitte la machine.
- Viole Principe #2 : nécessite une clé API et une connexion internet.
- Latence réseau + upload audio incompatible avec l'objectif < 2s.

**Verdict :** Incompatible avec les principes fondateurs d'Apollia OS.

### Option C - Voxtral (Mistral, modèle speech-to-text natif) (rejetée pour V1)

**Pour :**
- Modèle de la même famille que les LLM Mistral déjà supportés.
- Potentiellement intégrable via `mistral-rs-core`.

**Contre :**
- Pas encore disponible dans mistral-rs ni candle en Q2 2026.
- Format de modèle non standardisé à ce stade.

**Verdict :** Prévu pour V3 quand le support apparaît dans l'écosystème Rust.

### Option retenue - whisper-rs (whisper.cpp FFI) + trait SttBackend abstrait

**Pour :**
- Maturité production : v0.16.0, 28 releases, 184K téléchargements, MIT license.
- Compilation statique via CMake - zéro dépendance runtime (Principe #2).
- Support Metal natif avec RTF ~0.30x sur M1 (Principe #1).
- Pas de conflit de symboles ggml avec `mistral-rs-core` (qui utilise candle, pas llama.cpp).
- Le trait `SttBackend` isole le code appelant du backend : migration V2/V3 = implémenter le
  trait, changer un feature flag. Zéro refactoring du pipeline audio ou de l'intégration desktop.

**Compromis acceptés :**
- CMake requis au build-time (documenté dans INSTALL.md).
- ~5-15 Mo supplémentaires dans le binaire (code C++ whisper.cpp compilé statiquement).
- Le modèle GGML (~900 Mo) doit être téléchargé séparément (`apollia-os stt model download`).
- Conflit ggml potentiel **futur** si Apollia intègre un jour llama.cpp directement (pas via
  candle). Le trait `SttBackend` préserve la liberté de migrer vers candle-whisper.

## Conséquences

**Positives :**
- Pipeline STT 100% local : hotkey → capture audio → transcription → clipboard, < 2s sur M1 Metal.
- Trait `SttBackend` = liberté de migration vers candle-whisper (V2) ou Voxtral (V3) sans
  refactoring du code appelant (`SttEngine`, `SttFlow`, CLI).
- Feature flags cohérents avec `apollia-llm` (ADR-020) : les features workspace `metal`/`cuda`
  activent simultanément LLM et STT.
- `SttEngine` acteur Tokio (position 10 Supervisor) : démarrage conditionnel `stt.enabled = true`,
  shutdown graceful, `spawn_blocking` pour l'inférence synchrone.
- Observabilité native : `RuntimeEvent::SttTranscribed` sur EventBus + table `stt_transcriptions`
  SQLite.

**Négatives / Compromis :**
- CMake est une dépendance build-time supplémentaire (standard sur les env dev, mais à documenter).
- Deux moteurs d'inférence coexistent dans le binaire `--features metal` : `mistral-rs-core`
  (LLM) et `whisper-rs` (STT). Pas de conflit de symboles aujourd'hui, mais à surveiller.
- Le modèle GGML (~900 Mo) est un téléchargement initial non négligeable pour l'utilisateur.

**Neutres / À surveiller :**
- Conflit ggml si llama.cpp intégré directement (pas via candle) - le trait abstrait mitige.
- Benchmarks candle-whisper vs whisper.cpp sur Metal : point de décision V2.
- Disponibilité Voxtral dans l'écosystème Rust : point de décision V3.
- macOS permissions micro : `NSMicrophoneUsageDescription` requis dans `Info.plist` Tauri.

## Roadmap migration STT

| Phase | Backend | Quand | Déclencheur |
|---|---|---|---|
| **V1** (ce sprint) | `whisper-rs` (whisper.cpp FFI) | Q2 2026 | - |
| **V2** | `candle-whisper` (pure Rust) | Q3-Q4 2026 | Benchmarks candle ≥ whisper.cpp sur Metal |
| **V3** | Voxtral via candle ou mistral-rs | 2027 | Support Voxtral dans mistral-rs ou candle |

Chaque migration = implémenter `SttBackend` pour le nouveau moteur + changer le feature flag.
Le pipeline audio (capture, resample, silence trim), le `SttEngine`, le `SttFlow` desktop, et
la CLI restent inchangés.

## Principes architecturaux impactés

- **Principe #1 - Local-first** : inférence STT 100% locale, aucun échantillon audio ne sort
  de la machine. Renforcé.
- **Principe #2 - Zéro dépendance opérationnelle** : whisper.cpp compilé statiquement dans le
  binaire. Le modèle GGML est un fichier de données externe (`~/.apollia/models/`), même pattern
  qu'ADR-002 et ADR-020. CMake requis au build-time uniquement. Respecté.
- **Principe #4 - Fail fast** : modèle GGML absent/corrompu → `SttError::ModelLoadFailed` au
  démarrage du `SttEngine`, avant la première transcription. Respecté.
- **Principe #5 - Un acteur, une responsabilité** : crate `apollia-stt` dédiée, séparée de
  `apollia-llm`. `SttEngine` acteur distinct en position 10 Supervisor. Respecté.
- **Principe #7 - Garde-fous non-négociables** : `max_recording_sec` dans `SttConfig` empêche
  un enregistrement infini. Respecté.
- **Principe #8 - CLI humaine, API machine** : `apollia-os stt status/transcribe/models list`
  avec `--json`. Config dans `[stt]` de `apollia.toml`. Respecté.

## Liens

- Stories associées : STORY-290 → STORY-306 (Sprint 24)
- ADR précédent lié : ADR-020 - apollia-llm moteur embarqué, modèles externes, feature flags
  (même pattern : code compilé + données fichier + feature flags)
- ADR précédent lié : ADR-002 - SQLite comme fichier de données (modèle GGML = fichier externe,
  même que les `.db`)
- ADR précédent lié : ADR-027 - apollia-desktop processus unique Tauri (hotkey + clipboard +
  permissions micro intégrés dans le même processus)
