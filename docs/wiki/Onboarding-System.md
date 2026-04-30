# Systeme d'Onboarding — Spec technique

> *Reference technique du systeme d'onboarding agent-driven d'Apollia OS Desktop (v2.1.0).*

---

## 1. Architecture generale

Depuis la v2.1.0, l'onboarding est entierement pilote par l'agent `onboarding-agent`. Le frontend ne gere plus de machine a etats ni d'ecrans sequentiels — il affiche un simple modal de chat et se ferme quand l'agent signale la completion.

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
  └── {#if $onboardingModalOpen}
        └── <OnboardingModal onclose={...} />
              │
              ├── trigger_onboarding()        ← demarre la session agent
              ├── send_chat_message("Bonjour !") ← kick initial
              ├── setInterval(pollSession, 3s)  ← comptage des tours
              └── listen("OnboardingCompleted") ← auto-close
```

---

## 2. Condition d'ouverture

```typescript
function shouldOpenOnboarding(state: OnboardingState): boolean {
  return !state.completed && !state.skipped && state.started_at === null;
}
```

Le modal ne s'affiche qu'au tout premier lancement (`started_at == null`). Les relances ulterieures passent par la commande palette ("Relancer l'onboarding").

---

## 3. Composant `OnboardingModal`

Fichier : `ui/src/components/onboarding/OnboardingModal.svelte`

| Propriete | Valeur |
|---|---|
| Type | Overlay modal (`role="dialog"`, `aria-modal`) |
| Taille | `max-width: 720px`, `height: min(80vh, 720px)` |
| z-index | `80` |
| Fermeture | `OnboardingCompleted` (auto) ou bouton "Configurer plus tard" (skip) |
| Indicateur progression | Barre de `TOTAL_TURNS = 4` cercles + coche finale |
| `data-testid` | `onboarding-modal` / `onboarding-skip` / `onboarding-bootstrap` |

### Cycle de vie

1. `onMount` → `trigger_onboarding({ topic: null, profile: null })` → `sessionId`
2. `send_chat_message(sessionId, "Bonjour !")` pour amorcer l'agent sans attendre l'utilisateur
3. `setInterval(pollSession, 3000)` → `get_chat_session(sessionId)` → comptage des messages `role: "user"` → `userTurns`
4. Completion : `userTurns >= TOTAL_TURNS` OR evenement `OnboardingCompleted` → `onclose()`

---

## 4. Store `onboardingModalOpen`

```typescript
// ui/src/lib/stores/onboarding.ts
export const onboardingModalOpen: Writable<boolean>;
```

Mis a `true` par :
- `App.svelte` au demarrage si `shouldOpenOnboarding(state)`
- L'ecouteur `runtime-event` quand `OnboardingRequired` est emis
- La commande palette (action "Relancer l'onboarding")

Mis a `false` par :
- Le modal lui-meme sur `onclose` (skip ou completion)
- L'ecouteur `runtime-event` quand `OnboardingCompleted` est emis

---

## 5. Commandes IPC utilisees par le frontend

| Commande | Parametres | Retour | Contexte |
|---|---|---|---|
| `get_onboarding_state` | — | `OnboardingState` | Verif au demarrage |
| `trigger_onboarding` | `topic: null, profile: null` | `TriggerResult` | Demarre la session chat |
| `send_chat_message` | `sessionId: String, content: String` | `String` | Kick initial |
| `get_chat_session` | `sessionId: String` | `ChatSessionDetail` | Comptage tours |
| `dismiss_onboarding` | — | `()` | Bouton "Configurer plus tard" |
| `reset_onboarding` | — | `()` | Reinitialise vers l'etat initial |

### Commandes utilitaires (commandes/onboarding.rs)

| Commande | Retour | Description |
|---|---|---|
| `check_onboarded` | `bool` | `true` si completed |
| `mark_onboarded` | `()` | Force l'etat complete |
| `check_python` | `bool` | Python disponible |
| `check_llm_configured` | `bool` | LLM configure |
| `check_hello_agent_exists` | `Option<String>` | Path de l'agent demo |

---

## 6. Types TypeScript

```typescript
interface OnboardingState {
  completed: boolean;
  skipped: boolean;
  started_at: string | null;   // ISO8601, null si jamais demarre
}

interface TriggerResult {
  session_id: string;
}
```

La progression est trackee cote client uniquement via le comptage de messages `role: "user"` dans `ChatSessionDetail.messages`.

---

## 7. RuntimeEvents

Tous emis sur le canal Tauri `"runtime-event"` avec `category: "onboarding-changed"`.

| `event_type` | Moment | Effet frontend |
|---|---|---|
| `OnboardingRequired` | Premier lancement, avant l'UI prete | `onboardingModalOpen = true` |
| `OnboardingCompleted` | Agent ecrit `onboarding.completed_at` | `onboardingModalOpen = false` (auto-close modal) |

---

## 8. Reinitialisation — `reset_onboarding`

La commande IPC `reset_onboarding` (appelee aussi par la commande palette) :

1. Purge `UserMemoryRepository` — efface les cles `onboarding_topic_*`
2. Purge la base semantique de l'agent (`~/.apollia/memory/onboarding-agent.db`) — efface `user.*` et `onboarding.*` (sauf `onboarding.active_profile`)
3. Remet `started_at = null` pour que `shouldOpenOnboarding` s'evalue a `true` au prochain demarrage

**Garanties :**
- Si la DB semantique est absente ou illisible, la purge est ignoree silencieusement
- `onboarding.active_profile` est preserve
- Les donnees des autres namespaces (episodique, procedural) ne sont pas touchees
