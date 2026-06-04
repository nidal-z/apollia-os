# Speech-to-Text - Moteur STT embarque (apollia-stt)

> *Transcription vocale locale in-process via whisper.cpp - zero donnee audio ne quitte la machine, dictee universelle par hotkey global.*

---

## 1. Architecture

`apollia-stt` est la crate de transcription vocale d'Apollia OS. Elle expose un trait unifie `SttBackend` implemente par un backend concret, avec une roadmap de backends alternatifs :

| Backend | Feature flag | Moteur | Statut |
|---|---|---|---|
| `WhisperCppBackend` | `stt-whisper-cpp` (defaut) | whisper.cpp via `whisper-rs` 0.16 | Production |
| *(V2)* candle-whisper | *(futur)* | candle (Rust natif) | Roadmap |
| *(V3)* Voxtral | *(futur)* | Voxtral (Mistral) | Roadmap |

**Feature flags d'acceleration :**

| Feature | Activation | Prerequis |
|---|---|---|
| `stt-whisper-cpp` | Inclus par defaut | Aucun (CPU) |
| `stt-metal` | `--features stt-metal` | macOS Apple Silicon (M1+) |
| `stt-cuda` | `--features stt-cuda` | GPU NVIDIA + CUDA toolkit |

Les feature flags suivent le meme pattern que `apollia-llm` (ADR-008). Le modele `.bin` (format GGML) est un fichier de donnees dans `~/.apollia/models/` - jamais compile dans le binaire.

**Decision architecturale (ADR-009) :** whisper-rs (whisper.cpp FFI) est le backend V1. La roadmap prevoit candle-whisper (inference Rust native) en V2, et Voxtral (modele Mistral specialise audio) en V3. Le trait `SttBackend` permet cette evolution sans casser l'API.

---

## 2. Trait `SttBackend`

Toute la crate repose sur ce trait object-safe. Implementer ce trait suffit pour creer un backend custom ou un mock de test.

```rust
pub trait SttBackend: Send + Sync {
    /// Nom lisible du backend (ex: `"whisper-cpp"`, `"candle-whisper"`).
    fn name(&self) -> &str;

    /// Transcrit un buffer audio PCM f32 mono.
    fn transcribe(
        &self,
        audio: &[f32],
        sample_rate: u32,
        language_hint: Option<&str>,
    ) -> Result<TranscriptResult, SttError>;

    /// Detecte la langue dominante dans un buffer audio.
    /// Implementation par defaut : retourne `Ok(None)`.
    fn detect_language(&self, audio: &[f32]) -> Result<Option<String>, SttError> {
        let _ = audio;
        Ok(None)
    }

    /// Libere les ressources du backend (modele GPU, memoire, etc.).
    /// Implementation par defaut : no-op.
    fn unload(&self) {}
}
```

**Choix de design :** le trait est **synchrone** par conception. L'inference STT est CPU/GPU-bound, pas I/O-bound. L'appelant (`SttEngine`) wrappe les appels dans `tokio::task::spawn_blocking` pour ne pas bloquer le runtime Tokio.

L'implementation par defaut `WhisperCppBackend` charge un modele GGML depuis le disque via `WhisperCppBackend::load(model_path)`. Le chargement verifie l'existence du fichier avant de tenter le load (Principe #4 - Fail fast).

```rust
pub struct WhisperCppBackend { /* ... */ }

impl WhisperCppBackend {
    /// Charge un modele GGML whisper depuis le chemin specifie.
    /// Retourne `SttError::ModelNotFound` si le fichier n'existe pas.
    /// Retourne `SttError::ModelLoadFailed` si le chargement echoue.
    pub fn load(model_path: &str) -> Result<Self, SttError>;

    /// Retourne le chemin du modele GGML charge.
    pub fn model_path(&self) -> &str;
}
```

---

## 3. Types fondamentaux

```rust
/// Resultat complet d'une transcription audio.
pub struct TranscriptResult {
    /// Texte complet transcrit (concatenation de tous les segments).
    pub full_text: String,
    /// Segments temporels individuels avec timestamps et confiance.
    pub segments: Vec<TranscriptSegment>,
    /// Langue detectee ou utilisee pour la transcription (code ISO 639-1).
    pub language: Option<String>,
    /// Duree de l'audio source en millisecondes.
    pub audio_duration_ms: u64,
    /// Temps de traitement de la transcription en millisecondes.
    pub processing_time_ms: u64,
}

/// Segment individuel d'une transcription avec timestamps.
pub struct TranscriptSegment {
    /// Texte transcrit pour ce segment.
    pub text: String,
    /// Debut du segment en millisecondes depuis le debut de l'audio.
    pub start_ms: u64,
    /// Fin du segment en millisecondes depuis le debut de l'audio.
    pub end_ms: u64,
    /// Score de confiance du modele pour ce segment (0.0 - 1.0).
    pub confidence: f32,
}

/// Ligne persistee dans `stt_transcriptions`.
pub struct TranscriptRow {
    pub id: String,              // hex-encoded random bytes
    pub full_text: String,
    pub language: Option<String>,
    pub source: String,          // "hotkey" | "file" | "api"
    pub audio_duration_ms: i64,
    pub processing_time_ms: i64,
    pub model_name: Option<String>,
    pub created_at: String,      // ISO 8601
}

/// Erreurs du moteur STT (8 variants, thiserror).
pub enum SttError {
    ModelNotFound { path: String },
    ModelLoadFailed { reason: String },
    TranscriptionFailed { reason: String },
    InvalidAudio { reason: String },
    BackendUnavailable { backend: String },
    Timeout { timeout_ms: u64 },
    Internal(String),
    Repository { reason: String },
}
```

`TranscriptResult` et `TranscriptSegment` implementent `Serialize + Deserialize` pour le transport JSON via l'API REST et les commandes Tauri IPC.

---

## 4. Pipeline audio

Le pipeline audio comporte trois etapes executees sequentiellement avant de soumettre les echantillons au `SttBackend` :

### 4.1. `AudioCapture` (cpal)

Capture audio depuis le peripherique d'entree systeme par defaut via la crate `cpal` 0.15.

```rust
pub struct AudioCapture { /* device, config, sample_format */ }

impl AudioCapture {
    /// Ouvre le peripherique d'entree par defaut.
    pub fn default_input() -> Result<Self, SttError>;

    /// Demarre la capture. Retourne le Stream (a garder vivant) et le buffer partage.
    pub fn start(&self) -> Result<(Stream, CaptureBuffer), SttError>;
}

pub struct CaptureBuffer { /* Arc<Mutex<Vec<f32>>>, sample_rate, channels */ }

impl CaptureBuffer {
    /// Draine et retourne tous les echantillons accumules.
    pub fn drain(&self) -> Vec<f32>;
    pub fn sample_rate(&self) -> u32;
    pub fn channels(&self) -> u16;
}
```

Formats supportes : `F32` et `I16` (normalise en f32 via division par `i16::MAX`). Le callback `cpal` pousse les echantillons dans un `Arc<Mutex<Vec<f32>>>` que le consommateur draine a son rythme.

### 4.2. `to_whisper_format` (rubato)

Conversion vers le format attendu par Whisper : **mono 16 kHz f32**.

```rust
/// Convertit des echantillons PCM entrelaces vers mono 16 kHz f32.
/// Passthrough si deja mono 16 kHz. Sinon : mix mono + resample sinc.
pub fn to_whisper_format(
    samples: &[f32],
    from_sample_rate: u32,
    channels: u16,
) -> Result<Vec<f32>, SttError>;
```

Le resampling utilise `rubato::SincFixedIn` avec interpolation lineaire (sinc_len=256, f_cutoff=0.95, fenetre Blackman-Harris). Le mix mono fait la moyenne des canaux par frame.

### 4.3. `trim_silence`

Detection et suppression du silence en tete et fin de l'audio.

```rust
/// Supprime le silence en tete et fin d'un buffer 16 kHz mono.
/// Detection par energie RMS sur des fenetres de 160 echantillons (10 ms a 16 kHz).
/// Si tout le buffer est sous le seuil, le slice original est retourne tel quel.
pub fn trim_silence(audio: &[f32], threshold_db: f32) -> &[f32];
```

Le seuil est configurable via `silence_threshold_db` (defaut : -40.0 dB). Le seuil lineaire est calcule par `10^(threshold_db / 20)`.

---

## 5. `SttRepository` SQLite

Persistance des transcriptions dans `~/.apollia/data/stt_transcriptions.db` (SQLite, mode WAL).

### Schema (version 1)

```sql
CREATE TABLE IF NOT EXISTS stt_transcriptions (
    id                  TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    full_text           TEXT NOT NULL,
    language            TEXT,
    source              TEXT NOT NULL DEFAULT 'hotkey',
    audio_duration_ms   INTEGER NOT NULL DEFAULT 0,
    processing_time_ms  INTEGER NOT NULL DEFAULT 0,
    model_name          TEXT,
    created_at          TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    CHECK (source IN ('hotkey', 'file', 'api'))
);

CREATE INDEX IF NOT EXISTS idx_stt_transcriptions_created
    ON stt_transcriptions(created_at DESC);
```

### CRUD

```rust
pub struct SttRepository { /* rusqlite::Connection */ }

impl SttRepository {
    /// Ouvre (ou cree) la base. Active PRAGMA journal_mode=WAL.
    /// Applique les migrations si necessaire.
    pub fn open(path: &Path) -> Result<Self, SttError>;

    /// Persiste une transcription et retourne son ID genere.
    pub fn insert(
        &self,
        source: &str,
        result: &TranscriptResult,
        model_name: Option<&str>,
    ) -> Result<String, SttError>;

    /// Liste les transcriptions par `created_at DESC` avec pagination.
    pub fn list(&self, limit: u32, offset: u32) -> Result<Vec<TranscriptRow>, SttError>;

    /// Retourne une transcription par ID, ou `None` si absente.
    pub fn get(&self, id: &str) -> Result<Option<TranscriptRow>, SttError>;

    /// Supprime une transcription par ID. No-op si l'ID n'existe pas.
    pub fn delete(&self, id: &str) -> Result<(), SttError>;
}
```

**Timestamps :** ISO 8601 avec fractions de secondes (`strftime('%Y-%m-%dT%H:%M:%fZ', 'now')`). **IDs :** hex-encoded random bytes generes par SQLite (`lower(hex(randomblob(16)))`).

---

## 6. `SttEngine` acteur Tokio

L'integration runtime vit dans `apollia-runtime::stt`. Le pattern acteur standard Tokio est applique : `mpsc::channel` bounded + handle clonable.

### Handle public

```rust
/// Handle clonable, Send + Sync, pour interagir avec l'acteur SttEngine.
#[derive(Clone)]
pub struct SttEngineHandle {
    tx: mpsc::Sender<SttCommand>,
}

impl SttEngineHandle {
    /// Spawn l'acteur et retourne le handle.
    /// Emet `RuntimeEvent::SttModelLoaded` sur l'EventBus.
    pub fn start(
        backend: Box<dyn SttBackend>,
        repository: SttRepository,
        config: SttConfig,
        event_bus: EventBusSender,
    ) -> Self;

    /// Transcription asynchrone - l'inference tourne dans `spawn_blocking`.
    pub async fn transcribe(
        &self,
        audio: Vec<f32>,
        sample_rate: u32,
        source: TranscriptSource,
    ) -> Result<TranscriptResult, SttEngineError>;

    /// Status courant du moteur.
    pub async fn status(&self) -> Option<SttStatus>;

    /// Arret gracieux - appelle `backend.unload()` puis quitte.
    pub async fn shutdown(&self);
}
```

### Commandes internes

```rust
enum SttCommand {
    Transcribe { audio: Vec<f32>, sample_rate: u32, source: TranscriptSource, reply: oneshot::Sender<_> },
    GetStatus { reply: oneshot::Sender<SttStatus> },
    Shutdown,
}
```

### `SttStatus`

```rust
pub struct SttStatus {
    pub enabled: bool,
    pub model_loaded: bool,
    pub model_path: String,
    pub model_name: String,
    pub backend_name: String,
    pub metal_enabled: bool,    // cfg!(feature = "stt-metal")
    pub cuda_enabled: bool,     // cfg!(feature = "stt-cuda")
}
```

### Integration Supervisor (Phase 15)

Le `SttEngine` est demarre en **Phase 15** du Supervisor - conditionnellement :

1. Si `stt_config` est `None` ou `stt.enabled = false` → Phase 15 skippee, `stt_engine = None`.
2. Si le fichier modele est absent → log error, `stt_engine = None`, runtime continue.
3. Si `SttRepository::open` echoue → log error, `stt_engine = None`.
4. Si `try_load_backend` echoue (chargement du modele GGML) → log error, `stt_engine = None`.
5. Succes → `SttEngineHandle::start`, emission `RuntimeEvent::SttModelLoaded`.

**Degradation gracieuse :** aucun de ces cas d'erreur ne fait paniquer le runtime. Les routes API retournent 503 quand `stt_engine = None`.

### Evenements EventBus

```rust
RuntimeEvent::SttModelLoaded { backend: String, model_path: String, model_name: String }
RuntimeEvent::SttTranscribed { text: String, language: Option<String>, source: String, duration_ms: u64, processing_time_ms: u64 }
RuntimeEvent::SttTranscriptionFailed { reason: String }
RuntimeEvent::SttRecordingStarted
RuntimeEvent::SttRecordingStopped { audio_duration_ms: u64 }
```

---

## 7. API REST

5 endpoints sous `/api/v1/stt/` (module `apollia-runtime::api::routes_stt`).

| Methode | Route | Description | Reponse |
|---|---|---|---|
| `GET` | `/api/v1/stt/status` | Status du moteur STT | `200` SttStatusResponse / `503` |
| `POST` | `/api/v1/stt/transcribe` | Transcrire un fichier audio (multipart) | `200` TranscriptRow / `400` / `503` |
| `GET` | `/api/v1/stt/transcriptions` | Historique des transcriptions | `200` TranscriptionsListResponse / `503` |
| `DELETE` | `/api/v1/stt/transcriptions/:id` | Supprimer une transcription | `204` / `503` |
| `GET` | `/api/v1/stt/models` | Lister les fichiers modeles `.bin` | `200` ModelsListResponse |

**POST /transcribe** accepte `multipart/form-data` avec :
- `audio` (requis) : fichier WAV
- `language` (optionnel) : hint langue ISO 639-1

**GET /transcriptions** supporte la pagination via `?limit=N&offset=N` (defaut : limit=50, offset=0).

**Regle commune :** tous les endpoints (sauf `/models`) retournent **503 Service Unavailable** quand `stt_engine = None` ou `stt_repository = None` dans l'`AppState`.

---

## 8. Integration Desktop (Tauri v2)

Le module `apollia-desktop::stt` fournit quatre composants pour l'experience de dictee desktop :

### 8.1. `HotkeyListener`

Ecoute globale du raccourci clavier via `tauri-plugin-global-shortcut`. Fonctionne meme quand l'application Apollia n'a pas le focus.

Deux modes de declenchement :
- **Toggle** : premiere pression = debut, deuxieme pression = fin.
- **Push-to-talk** : touche maintenue = enregistrement, relachement = fin.

```rust
pub struct HotkeyListener { /* recording: Arc<AtomicBool>, hotkey, trigger_mode */ }

impl HotkeyListener {
    pub fn new(hotkey: String, trigger_mode: TriggerMode) -> Self;
    pub fn recording_flag(&self) -> Arc<AtomicBool>;
    pub fn register<F1, F2>(self, app: &tauri::AppHandle, on_start: F1, on_stop: F2) -> Result<(), String>;
}
```

### 8.2. `ClipboardManager`

Injection de texte via le presse-papier systeme + simulation de raccourci Coller.

- `arboard` pour la lecture/ecriture du presse-papier.
- `enigo` pour la simulation clavier (`Cmd+V` macOS, `Ctrl+V` Linux).
- Option `clipboard_restore` : sauvegarde le contenu precedent du presse-papier et le restaure apres un delai de 100 ms.

```rust
/// Injecte du texte a la position du curseur via clipboard + paste simule.
/// Fonction bloquante - appeler depuis `spawn_blocking`.
pub fn inject(text: &str, restore: bool) -> Result<(), ClipboardError>;
```

### 8.3. `SttFlow`

Orchestrateur end-to-end : hotkey → capture → resample → trim → transcribe → clipboard/notification.

```rust
pub struct SttFlow { /* config, stt_engine, event_bus, app, recording, active_buffer, stop_tx */ }

impl SttFlow {
    pub fn new(config: SttConfig, stt_engine: SttEngineHandle, event_bus: EventBusSender, app: tauri::AppHandle) -> Self;
    pub fn recording_flag(&self) -> Arc<AtomicBool>;
    pub fn start_recording(&self);
    pub async fn stop_and_transcribe(&self);
}
```

Le flux audio tourne sur un thread OS dedie (affinite Core Audio sur macOS - `cpal::Stream` n'est pas `Send`). Le `CaptureBuffer` partage est draine depuis le contexte async.

Les enregistrements de moins de 100 ms (1600 echantillons a 16 kHz) sont ignores. Les enregistrements depassant `max_recording_sec` sont tronques.

Le mode de dispatch depend de `clipboard_mode` :
- `"paste"` : injection clipboard + paste simule.
- `"memo"` : notification desktop uniquement.
- `"both"` : les deux.

### 8.4. `RecordingOverlay`

Fenetre Tauri secondaire toujours au premier plan, non focusable, sans decorations. Affiche un indicateur visuel d'enregistrement en cours.

```rust
pub struct RecordingOverlay { /* app, hotkey */ }

impl RecordingOverlay {
    pub fn create(app: &tauri::AppHandle, hotkey: String) -> Result<Self, OverlayError>;
    pub fn show(&self) -> Result<(), OverlayError>;
    pub fn hide(&self) -> Result<(), OverlayError>;
}
```

Positionnement : centre-bas de l'ecran principal (350x80 px logiques, marge de 40 px du bas). La visibilite est pilotee par un listener EventBus qui reagit a `SttRecordingStarted` / `SttRecordingStopped`.

### 8.5. Commandes Tauri IPC

5 commandes exposees au frontend Svelte :

| Commande | Signature | Description |
|---|---|---|
| `get_stt_status` | ` -> Result<Value, String>` | Status du moteur STT |
| `list_transcriptions` | `(limit: Option<u32>) -> Result<Vec<TranscriptRow>, String>` | Historique des transcriptions |
| `delete_transcription` | `(id: String) -> Result<, String>` | Supprimer une transcription |
| `transcribe_file` | `(file_path: String) -> Result<TranscriptRow, String>` | Transcrire un fichier WAV local |
| `list_stt_models` | ` -> Result<Vec<SttModelInfo>, String>` | Lister les modeles disponibles |

### 8.6. Evenements Tauri (event bridge)

- `stt-transcribed` - fast path vers le frontend (contient le texte transcrit).
- `stt-recording-started` / `stt-recording-stopped` - pilotent l'overlay et les stores Svelte.
- `stt-overlay-config` - envoie la configuration hotkey a la fenetre overlay.

---

## 9. Frontend Svelte

### Route

`/transcriptions` dans la categorie "Donnees" du sidebar.

### Stores

- `sttStatus` - etat courant du moteur STT (polling ou event-driven).
- `transcriptions` - liste des transcriptions recentes.
- `isRecording` - flag booleen pilote par les evenements `stt-recording-started/stopped`.

### Composants

| Composant | Role |
|---|---|
| `TranscriptCard` | Affiche une transcription (texte, langue, source, duree, date). |
| `TranscribeFileDialog` | Dialogue de selection de fichier audio + soumission au backend STT. |
| `RecordingOverlay.svelte` | Vue de la fenetre overlay (indicateur d'enregistrement, hotkey affiche). |

### Settings

Section STT dans la page Parametres - affichage read-only de la configuration courante (conforme ADR-020 : la configuration structurelle est dans `apollia.toml`, pas editable depuis l'UI).

---

## 10. CLI

Sous-commande `apollia-os stt` (module `apollia-cli::commands::stt`).

```
apollia-os stt status                      # Status du moteur STT
apollia-os stt transcribe <file>           # Transcrire un fichier WAV
apollia-os stt transcribe <file> --output out.json  # Sauvegarder le resultat en JSON
apollia-os stt transcriptions list         # Lister l'historique
apollia-os stt transcriptions list --limit 10       # Avec pagination
apollia-os stt model list                  # Lister les modeles .bin disponibles
apollia-os stt model download <name>       # Telecharger depuis HuggingFace
apollia-os stt model download default      # Alias pour whisper-large-v3-fr-q5_0
```

**Pre-check (Principe #4) :** `stt transcribe` verifie que `stt.enabled = true` dans la configuration du runtime avant de soumettre la transcription. Si STT est desactive, un message clair est affiche sans appeler l'API.

**Telechargement de modeles :** utilise `hf-hub` (crate HuggingFace) en mode synchrone. Le repository par defaut est `bofenghuang/whisper-large-v3-french`. Le modele est d'abord telecharge dans le cache HuggingFace, puis copie dans `~/.apollia/models/`. Un spinner `indicatif` affiche la progression en mode TTY.

**Resolution de noms :**
- `default` ou `whisper-large-v3-fr-q5_0` → `bofenghuang/whisper-large-v3-french/whisper-large-v3-fr-q5_0.bin`
- `owner/repo/file.bin` → repo `owner/repo`, fichier `file.bin`
- `nom-simple` → fichier `nom-simple.bin` dans le repo par defaut

Tous les sous-commandes supportent le flag global `--json` pour une sortie machine-readable.

---

## 11. Configuration `[stt]`

Section `[stt]` dans `apollia.toml` (separation structurelle TOML / operationnelle SQLite conforme ADR-014).

```toml
[stt]
enabled = true
model_path = "~/.apollia/models/whisper-large-v3-fr-q5_0.bin"
hotkey = "ctrl+shift+space"
clipboard_mode = "paste"       # "paste" | "memo" | "both"
clipboard_restore = true
silence_threshold_db = -40.0
max_recording_sec = 60
language = "fr"
trigger_mode = "toggle"        # "toggle" | "push_to_talk"
```

| Champ | Type | Defaut | Description |
|---|---|---|---|
| `enabled` | `bool` | `false` | Active/desactive le moteur STT |
| `model_path` | `PathBuf` | `~/.apollia/models/whisper-large-v3-fr-q5_0.bin` | Chemin du modele GGML |
| `hotkey` | `String` | `ctrl+shift+space` | Raccourci global de declenchement |
| `clipboard_mode` | `String` | `paste` | Mode de dispatch : paste / memo / both |
| `clipboard_restore` | `bool` | `true` | Restaurer le presse-papier apres injection |
| `silence_threshold_db` | `f32` | `-40.0` | Seuil RMS pour la detection de silence |
| `max_recording_sec` | `u32` | `60` | Duree maximale d'enregistrement |
| `language` | `Option<String>` | `Some("fr")` | Hint langue ISO 639-1 pour le modele |
| `trigger_mode` | `String` | `toggle` | Mode de declenchement : toggle / push_to_talk |

---

## 12. Decisions architecturales cles

| Decision | Justification |
|---|---|
| Trait synchrone `SttBackend` | L'inference STT est CPU/GPU-bound, pas I/O-bound. L'appelant wrappe dans `spawn_blocking`. Simplifie les implementations de backends. |
| Feature flags pour backends | Meme pattern que `apollia-llm` (ADR-008). Permet de compiler sans whisper.cpp pour les environnements sans C++ toolchain. |
| whisper-rs V1, candle V2, Voxtral V3 (ADR-009) | whisper.cpp est le moteur le plus mature et performant. candle-whisper eliminera la dependance C++ FFI. Voxtral explorera les modeles audio next-gen. |
| Thread OS dedie pour `cpal::Stream` | Core Audio sur macOS impose une affinite de thread - `cpal::Stream` n'est pas `Send`. Le buffer partage `Arc<Mutex<Vec<f32>>>` permet la communication cross-thread. |
| Resample rubato sinc | Qualite superieure aux algorithmes lineaires pour la conversion 48 kHz → 16 kHz. Le surcout CPU est negligeable face a l'inference. |
| `SttRepository` separe dans l'`AppState` | SQLite WAL supporte les lecteurs concurrents. Le Supervisor ouvre deux connexions : une pour l'acteur (ecriture), une pour les routes API (lecture). |
| Degradation gracieuse dans le Supervisor | Modele absent ou chargement echoue → `stt_engine = None`, runtime continue. Aucun panic, routes API retournent 503. Conforme Principe #4 (Fail fast au demarrage pour les erreurs detectables). |
| Settings read-only dans l'UI (ADR-020) | La configuration STT est structurelle (`apollia.toml`), pas operationnelle. L'UI affiche mais ne modifie pas. |

---

## 13. Diagrammes de reference

- [Architecture Vue d'Ensemble](./Architecture-Vue-Ensemble) - positionnement de `apollia-stt` dans le workspace
- [Briques Runtime Core](./Briques-Runtime-Core) - Supervisor Phase 15, integration acteur
- [Briques Desktop](./Briques-Desktop) - integration Tauri, hotkey, overlay
- [Config apollia.toml](./Config-apollia-toml) - section `[stt]`
- [Briques CLI](./Briques-CLI) - sous-commande `apollia-os stt`
- [API-HTTP-Observability](./API-HTTP-Observability#stt-speech-to-text---28) - routes `/api/v1/stt/*`

---

## Voir aussi

- [Briques LLM Backend](./Briques-LLM-Backend) - pattern similaire (trait + feature flags + Supervisor integration)
- [Config apollia.toml](./Config-apollia-toml) - section `[stt]` complete
- [Ops Exploitation et Debug](./Ops-Exploitation-et-Debug) - `apollia-os stt status/transcribe`
- [Architecture Principes](./Architecture-Principes) - Principes #1 (Local-first), #4 (Fail fast), #5 (Un acteur, une responsabilite)
