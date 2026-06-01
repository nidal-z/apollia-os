# Systeme d'Onboarding - Spec technique

> *Reference technique du parcours d'onboarding multi-etapes d'Apollia OS Desktop (v2.2.0).*

---

## 1. Vue d'ensemble

Depuis la v2.2.0, l'onboarding est un **parcours multi-etapes** orchestré côté frontend, avec persistance backend de la phase courante. Quatre etapes consecutives :

| # | Etape (frontend) | Phase backend | Composant Svelte |
|---|---|---|---|
| 1 | Accueil | `welcome` | `OnboardingWelcome.svelte` |
| 2 | Profil (operator/builder) | `profile_choice` | `OnboardingProfileSelector.svelte` |
| 3 | Modèles (LLM + STT) | `ai_setup` | `OnboardingAiSetup.svelte` |
| 4 | Calibrage agent (chat) | `acquaintance` | `OnboardingChatStep.svelte` |

Le composant `OnboardingModal.svelte` joue le rôle d'orchestrateur : il maintient l'etat local `currentStep`, route les composants, et synchronise la phase backend en best-effort via `advance_onboarding_phase`.

```
App.svelte
  │
  ├── get_onboarding_state()   ← verif au demarrage
  │     └── si !completed && !skipped && started_at == null
  │           └── onboardingModalOpen.set(true)
  │
  ├── listen("runtime-event")  ← canal live
  │     ├── OnboardingRequired  → onboardingModalOpen.set(true)
  │     └── OnboardingCompleted → onboardingModalOpen.set(false)
  │
  ├── llmBackends.subscribe(...) ← rouvre le modal sur 0→1 backend
  │
  └── {#if $onboardingModalOpen}
        └── <OnboardingModal onclose={...} />
              │
              ├── restoreStepFromBackend()    ← snap à la phase persistée
              ├── currentStep ∈ {welcome, profile, ai-setup, chat}
              ├── advance_onboarding_phase(...) (best-effort, par etape)
              ├── listen("OnboardingCompleted") ← auto-close
              │
              └── {#switch currentStep}
                    ├── welcome   → <OnboardingWelcome onnext={...} />
                    ├── profile   → <OnboardingProfileSelector onnext={...} onback={...} />
                    ├── ai-setup  → <OnboardingAiSetup onnext / onback / onskip / onopencloud />
                    └── chat      → <OnboardingChatStep onback={...} />
```

---

## 2. Condition d'ouverture

```typescript
function shouldOpenOnboarding(state: OnboardingState): boolean {
  return !state.completed && !state.skipped && state.started_at === null;
}
```

Le modal s'affiche au tout premier lancement (`started_at == null`). Les relances ulterieures passent par :
- la commande palette ("Relancer l'onboarding"),
- **Paramètres → Zone de danger → Réinitialiser l'onboarding** (efface uniquement les marqueurs de progression),
- **Paramètres → Zone de danger → Réinitialisation d'usine** (efface tout `~/.apollia/`).

App.svelte rouvre aussi automatiquement le modal lorsque `$llmBackends` passe de 0 à ≥1, pour gérer le cas où l'utilisateur a quitté l'onboarding pour ajouter un backend cloud depuis `/llm`.

---

## 3. Reprise de parcours (resumable)

Au montage du modal, `restoreStepFromBackend()` lit `get_onboarding_state` et saute directement à l'etape correspondant à la phase backend persistée.

| Phase backend | Etape frontend rendue |
|---|---|
| `welcome` | Accueil |
| `profile_choice` | Profil |
| `ai_setup` | Modèles |
| `acquaintance` / `guided_tour` / `graduation` / `done` | Calibrage |

Conséquence : si l'utilisateur ferme le modal pour ajouter un backend cloud (en cliquant **Utiliser un fournisseur cloud** dans l'etape Modèles), au retour il reprend exactement où il etait.

---

## 4. Composant `OnboardingModal` - orchestrateur

Fichier : `ui/src/components/onboarding/OnboardingModal.svelte`

| Propriete | Valeur |
|---|---|
| Type | Overlay modal (`role="dialog"`, `aria-modal`) |
| Taille | `max-width: 720px`, `max-height: 90vh`, `min(86vh, 760px)` pour `ai-setup` et `chat()` |
| z-index | `80` |
| Capture Escape | uniquement à l'etape `chat()` (les autres laissent passer) |
| Rail de progression | Header avec 4 etapes nommées (Accueil · Profil · Modèles · Calibrage) |
| `data-testid` | `onboarding-modal` |

### State local (Svelte 5 runes)

```typescript
type Step = "welcome" | "profile" | "ai-setup" | "chat";
let currentStep = $state<Step>("welcome");
const stepIndex = $derived(STEPS.findIndex((s) => s.id === currentStep));
```

### Synchronisation backend

`syncBackendPhase(target)` est appelée à chaque transition d'etape. La phase machine valide les transitions sequentielles strictes - un echec est ignoré silencieusement (cas où la phase backend est déjà au-delà).

| Transition frontend | Phase cible | Commande IPC |
|---|---|---|
| → profile | `profile_choice` | `advance_onboarding_phase` |
| → ai-setup | `ai_setup` | `advance_onboarding_phase` |
| → chat | `acquaintance` | `advance_onboarding_phase` |

---

## 5. Etape 1 - `OnboardingWelcome`

Fichier : `ui/src/components/onboarding/OnboardingWelcome.svelte`

| Aspect | Détail |
|---|---|
| Style | Tailwind pur (composant `Button` partagé `variant="primary-gradient"`) |
| Contenu | Logo gradient, tagline, sous-titre, 3 cartes value-prop (Local-first / LLM au choix / Agents autonomes), CTA |
| Props | `onnext: => void` |
| `data-testid` | `onboarding-welcome`, `onboarding-welcome-cta` |

Aucune logique métier - sert uniquement à briser la friction du lancement à froid.

---

## 6. Etape 2 - `OnboardingProfileSelector`

Fichier : `ui/src/components/onboarding/OnboardingProfileSelector.svelte`

| Aspect | Détail |
|---|---|
| Choix | `"operator"` ou `"builder"` |
| IPC | `set_onboarding_profile(profile)` puis `onnext()` |
| Side-effect | Met à jour le store `uiMode` pour adapter l'UI immédiatement |
| Lien hors-piste | "Je suis les deux → mode Builder" (force `builder`) |
| Props | `onnext: => void`, `onback: => void` |
| `data-testid` | `onboarding-profile-selector`, `profile-card-operator`, `profile-card-builder`, `profile-back`, `profile-both` |

Carte Opérateur : 3 puces orientées simplicité + validation, exemple "PM, RSE, Support, Direction".
Carte Builder : 3 puces orientées observabilité + SDK, exemple "Dev, Data, Infra, IA".

---

## 7. Etape 3 - `OnboardingAiSetup`

Fichier : `ui/src/components/onboarding/OnboardingAiSetup.svelte` (~1100 lignes)

### Structure

| Section | Contenu |
|---|---|
| **Header** | Titre + sous-titre |
| **Bandeau système** | Chips RAM + OS/arch + GPU (issu de `get_ai_setup_info`) |
| **Section LLM** | Modèles GGUF locaux détectés OU liste curée + recherche HuggingFace + bouton cloud |
| **Section STT** | Toggle activation, modèles Whisper détectés OU liste curée |
| **Footer** | Boutons Retour / Configurer plus tard / Continuer (gradient primaire) |

### Catalogues curés

```typescript
const CURATED_LLM_MODELS: CuratedLlmModel[] = [
  { name: "Qwen3 4B",       size: "2.5 GB",  ram: 4 },
  { name: "Qwen3 8B",       size: "4.7 GB",  ram: 8 },
  { name: "Qwen3 14B",      size: "8.4 GB",  ram: 16 },
  { name: "Qwen3 30B-A3B",  size: "18.6 GB", ram: 24 },
];

const CURATED_STT_MODELS: CuratedSttModel[] = [
  { name: "Whisper Tiny",                    size: "75 MB",   ram: 1 },
  { name: "Whisper Base",                    size: "142 MB",  ram: 2 },
  { name: "Whisper Large-v3 Turbo Q5",       size: "547 MB",  ram: 4 }, // ⭐ recommandé
  { name: "Whisper Large-v3 Q5",             size: "1.1 GB",  ram: 8 },
  { name: "Whisper Large-v3 French",         size: "~1.1 GB", ram: 8 },
];
```

Filtrage dynamique par RAM disponible (`sysInfo.total_ram_gb`). Le badge "Recommandé" est calculé via `$derived` :
- LLM : plus gros modèle qui rentre.
- STT : `large-v3-turbo-q5` si la RAM le permet, sinon plus gros qui rentre.

### Commandes IPC consommées

| Commande | Usage |
|---|---|
| `get_ai_setup_info` | Bandeau système (RAM/OS/arch/GPU) |
| `scan_for_gguf_models` | Détection des `.gguf` dans `~/.apollia/models/` et `~/Downloads/` |
| `scan_for_whisper_models` | Détection des `ggml-*.bin` |
| `setup_local_llm({ ggufPath })` | Enregistre un backend `local` (TOML + DB) |
| `reload_llm` | Recharge le router LLM |
| `setup_whisper_model({ modelPath })` | Wire le modèle Whisper actif |
| `start_model_download({ url, filename, hf_token?, dest_dir?, repo_id? })` | Démarre un téléchargement, retourne `downloadId`. Quand `repo_id` est fourni (`org/name`), le downloader fetch ensuite `generation_config.json` depuis HF (avec retry sur `cardData.base_model` pour les quanteurs Bartowski/Unsloth/mradermacher) et persiste les sampling defaults officiels dans `~/.apollia/models/sampling-defaults.json`. Voir [LLM-Sampling-Defaults](./LLM-Sampling-Defaults). |
| `cancel_model_download({ downloadId })` | Annule un téléchargement actif |
| `search_hf_models({ query, limit })` | Recherche HuggingFace |
| `get_hf_model({ repoId })` | Métadonnées détaillées + liste des fichiers GGUF |

### Evénement Tauri consommé

`"model-download-progress"` - payload `DownloadProgress`. Distinguer `id === llmDownloadId` vs `sttDownloadId`. Statuts : `in_progress`, `completed`, `cancelled`, `failed`. À `completed`, déclenche un `loadData()` (LLM) ou `rescanStt()` (STT) pour refléter l'arrivée du nouveau fichier.

### Garde-fou Continuer

```typescript
const hasUsableLlm = $derived(llmSuccess || $llmBackends.length > 0);
```

Le bouton **Continuer** est désactivé tant qu'aucun LLM n'est utilisable - soit le modèle local vient d'être configuré dans cette session (`llmSuccess`), soit un backend cloud est déjà enregistré (via `$llmBackends`).

### Props

| Prop | Effet |
|---|---|
| `onnext` | Continuer vers chat (et lance `setup_whisper_model` si STT activé) |
| `onback` | Retour à profile |
| `onskip` | Aller au chat sans configurer STT (mais LLM reste obligatoire) |
| `onopencloud` | Fermer le modal pour permettre l'ajout d'un backend cloud depuis `/llm` |

### `data-testid`

`onboarding-ai-setup`, `system-info-bar`, `scan-loading`, `llm-section`, `stt-section`,
`curated-llm-list`, `curated-llm-row`, `curated-stt-list`, `curated-stt-row`,
`llm-empty-hint`, `stt-empty-hint`, `llm-download-progress`, `stt-download-progress`,
`llm-download-error`, `stt-download-error`, `search-results`,
`stt-toggle`, `whisper-model-list`, `whisper-model-row`, `llm-model-list`, `llm-model-row`,
`ai-setup-back`, `ai-setup-skip`, `ai-setup-continue`, `onboarding-open-cloud`.

---

## 8. Etape 4 - `OnboardingChatStep`

Fichier : `ui/src/components/onboarding/OnboardingChatStep.svelte`

### Cycle de vie

1. À l'arrivée, vérifie `$llmBackends.length > 0` (filet de sécurité).
   - Si non : affiche un état "Aucun moteur LLM disponible" + bouton **← Étape précédente**.
2. `trigger_onboarding({ topic: null, profile: null })` → `sessionId`.
3. `send_chat_message(sessionId, "Bonjour !")` pour amorcer l'agent (failures tolérées).
4. `setInterval(pollSession, 3000)` → `get_chat_session(sessionId)` → comptage des messages `role: "user"` → `userTurns`.
5. `TOTAL_TURNS = 4`. Au 4e tour ou sur `OnboardingCompleted`, le parent ferme le modal.

### Indicateur de progression

Quatre pips horizontaux + une coche finale. Les pips actifs sont remplis en gradient `primary → secondary`.

### Props

| Prop | Effet |
|---|---|
| `onback` | Retour à `ai-setup` |
| `onclose` | Réservé - l'orchestrateur ferme le modal sur `OnboardingCompleted` |

### `data-testid`

`onboarding-chat-step`, `onboarding-chat-no-llm`, `onboarding-bootstrap`.

---

## 9. Store `onboardingModalOpen`

```typescript
// ui/src/lib/stores/onboarding.ts
export const onboardingModalOpen: Writable<boolean>;
```

Mis à `true` par :
- `App.svelte` au demarrage si `shouldOpenOnboarding(state)`,
- l'ecouteur `runtime-event` quand `OnboardingRequired` est emis,
- la commande palette ("Relancer l'onboarding"),
- `App.svelte` lorsque `$llmBackends` passe de 0 à ≥1 et que l'onboarding doit encore se faire.

Mis à `false` par :
- le modal lui-meme sur `onclose` (skip ou completion),
- l'ecouteur `runtime-event` quand `OnboardingCompleted` est emis,
- l'utilisateur cliquant sur **Utiliser un fournisseur cloud** (l'etape `ai-setup` appelle `onopencloud()` qui ferme le modal et navigue vers `/llm`).

---

## 10. Commandes IPC utilisees par le frontend

| Commande | Parametres | Retour | Etape |
|---|---|---|---|
| `get_onboarding_state` | - | `OnboardingState` | Verif au demarrage + `restoreStepFromBackend` |
| `advance_onboarding_phase` | `targetPhase: OnboardingPhase` | `OnboardingState` | Sync à chaque transition d'etape |
| `set_onboarding_profile` | `profile: "operator" \| "builder"` | `OnboardingState` | Etape 2 (profile) |
| `get_ai_setup_info` | - | `SystemInfo` | Etape 3 (ai-setup) header |
| `scan_for_gguf_models` | - | `Vec<GgufModelInfo>` | Etape 3 (ai-setup) LLM |
| `scan_for_whisper_models` | - | `Vec<WhisperModelInfo>` | Etape 3 (ai-setup) STT |
| `setup_local_llm` | `ggufPath: String` | `()` | Etape 3 (ai-setup) LLM |
| `reload_llm` | - | `()` | Etape 3 (ai-setup) LLM |
| `setup_whisper_model` | `modelPath: String` | `()` | Etape 3 (ai-setup) STT |
| `start_model_download` | `request: { url, filename, hf_token?, dest_dir?, repo_id? }` | `String` (downloadId) | Etape 3 (ai-setup) télé. `repo_id` déclenche l'auto-persistance des sampling defaults officiels HF. |
| `cancel_model_download` | `downloadId: String` | `()` | Etape 3 (ai-setup) télé. |
| `search_hf_models` | `params: { query, limit }` | `{ models, next_cursor }` | Etape 3 (ai-setup) HF |
| `get_hf_model` | `repoId: String` | `HfModelCard` | Etape 3 (ai-setup) HF |
| `trigger_onboarding` | `topic: null, profile: null` | `TriggerResult` | Etape 4 (chat) demarrage |
| `send_chat_message` | `sessionId: String, content: String` | `String` | Etape 4 (chat) kick initial |
| `get_chat_session` | `sessionId: String` | `ChatSessionDetail` | Etape 4 (chat) polling |
| `dismiss_onboarding` | - | `()` | Bouton "Configurer plus tard" |
| `reset_onboarding` | - | `()` | Settings → Zone de danger |

### Commandes utilitaires (`commands/onboarding.rs`)

| Commande | Retour | Description |
|---|---|---|
| `check_onboarded` | `bool` | `true` si `~/.apollia/.onboarded` existe |
| `mark_onboarded` | `()` | Crée le flag `.onboarded` |
| `check_python` | `bool` | Python 3 disponible |
| `check_llm_configured` | `bool` | Au moins un backend LLM enregistré |
| `check_hello_agent_exists` | `Option<String>` | Path de l'agent demo |

---

## 11. Types TypeScript

```typescript
type OnboardingPhase =
  | "welcome"
  | "profile_choice"
  | "ai_setup"
  | "acquaintance"
  | "guided_tour"
  | "graduation"
  | "done";

interface OnboardingState {
  phase: OnboardingPhase;
  profile: string | null;
  llm_configured: boolean;
  stt_configured: boolean;
  topics_covered: string[];
  mandatory_complete: boolean;
  // ... + tour fields, stats, started_at, completed_at, skipped, completed
}

interface TriggerResult {
  session_id: string;
}
```

---

## 12. RuntimeEvents

Tous emis sur le canal Tauri `"runtime-event"` avec `category: "onboarding-changed"`.

| `event_type` | Moment | Effet frontend |
|---|---|---|
| `OnboardingRequired` | Premier lancement, avant l'UI prete | `onboardingModalOpen = true` |
| `OnboardingCompleted` | Agent ecrit `onboarding.completed_at` | `onboardingModalOpen = false` (auto-close modal) |

Canal Tauri additionnel : `"model-download-progress"` (consommé par `OnboardingAiSetup`).

---

## 13. Reinitialisation

### `reset_onboarding` (soft)

Efface uniquement les marqueurs de progression et de profil. Backends LLM, modèles téléchargés et autres données restent intacts.

1. Supprime `~/.apollia/.onboarded`.
2. Purge les clés `onboarding_*` dans `UserMemoryRepository` (Context).
3. Purge la base sémantique de l'agent (`~/.apollia/memory/onboarding-agent.db`) - efface `user.*` et `onboarding.*` (sauf `onboarding.active_profile`).

Effet : `started_at = null` au prochain `get_onboarding_state`, donc `shouldOpenOnboarding == true` au prochain demarrage. Le parcours redémarre depuis l'Accueil.

### `factory_reset` (hard)

Supprime **tout** `~/.apollia/`. Action destructive irréversible.

1. `tokio::fs::remove_dir_all(~/.apollia/)`.
2. `app.restart()` (peut échouer en mode dev - bandeau orange en fallback).
3. Au redémarrage, runtime fresh → `OnboardingRequired` émis → parcours en 4 etapes ouvert depuis l'Accueil.

---

## 14. Tests

| Niveau | Couverture |
|---|---|
| Unit (Rust) | Phase machine, transitions, persistence dans UserMemory - voir `crates/apollia-desktop/src/commands/onboarding.rs::tests` |
| Component (Svelte/Vitest) | À ajouter - actuellement non couvert |
| E2E (Playwright) | À ajouter - `crates/apollia-desktop/ui/tests/` ne couvre pas encore le parcours v2.2.0 |

> **Voir aussi :** [Briques-Desktop](https://github.com/Apollia-OS/apollia-os/wiki/Briques-Desktop) pour le contexte global Tauri/Svelte, [Briques-LLM-Backend](https://github.com/Apollia-OS/apollia-os/wiki/Briques-LLM-Backend) pour le détail des providers.
