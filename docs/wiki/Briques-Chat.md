# Chat — Sous-systeme conversationnel

> Page source de verite pour le sous-systeme Chat introduit au Sprint 18.
> Derniere mise a jour : Sprint 25.

---

## Vue d'ensemble

Le sous-systeme Chat fournit un mode interactif pour les agents Apollia OS. Contrairement au `TaskRouter` qui traite des taches ponctuelles (fire-and-forget), le Chat maintient des sessions longues avec historique, streaming token-by-token et approbation d'outils inline.

Deux modes d'execution coexistent :

| Mode | Moteur | Streaming | Dependance Python |
|---|---|---|---|
| **Chat Libre** | `BuiltInChatAgent` (Rust pur, boucle ReAct) | Oui (token-by-token) | Non |
| **Chat Agent** | `AgentChatExecutor` (Python via `AIPBridge`) | Non (reponse bloc) | Oui |

Le Chat emprunte un chemin d'execution separe du `TaskRouter` (cf. ADR-034). Le `ChatSessionManager` est un acteur Tokio dedie, position 13 dans la sequence de demarrage du Supervisor.

La persistance repose sur `chat.db`, une base SQLite dediee contenant trois tables : sessions, messages et autorisations d'outils.

---

## Architecture

### ChatSessionManager (acteur Tokio)

Le `ChatSessionManager` suit le pattern acteur standard du projet : `mpsc::channel` + handle clonable.

```
ChatSessionManagerHandle (Clone + Send + Sync)
        |
        v  mpsc::Sender<ChatCommand>
ChatSessionManager (actor loop)
        |
        +-- HashMap<String, ChatSession>  (etat en memoire)
        +-- ChatRepository (SQLite chat.db)
        +-- PendingChatApprovals (oneshot channels)
        +-- EventBusSender (broadcast RuntimeEvent)
```

L'enum `ChatCommand` definit les 7 messages acceptes par l'acteur :

- `CreateSession` — cree une session (mode, agent optionnel, outils, prompt systeme)
- `SendMessage` — envoie un message utilisateur, declenche l'execution
- `ResolveTool` — resout une approbation d'outil en attente
- `ListSessions` — retourne la liste des sessions (filtre optionnel par statut)
- `GetSession` — retourne le detail complet d'une session (historique inclus)
- `CloseSession` — ferme une session (status → `Closed`)
- `Shutdown` — arret propre de l'acteur

Chaque commande porte un `oneshot::Sender` pour la reponse, sauf `Shutdown`.

Le `ChatSessionManager` occupe la position 13 dans la sequence de demarrage du Supervisor, apres le `NotificationEngine` (position 9) et le `PipelineEngine` (position 8).

### Chat Libre — BuiltInChatAgent

Le mode Libre utilise un agent Rust pur qui execute une boucle ReAct sans dependance Python.

Flux d'execution par echange :

1. Le message utilisateur est ajoute a l'historique de la session
2. L'historique complet est converti en `CompletionRequest`
3. `LlmRouter.stream()` produit un flux de tokens
4. Chaque token est emis comme `ChatToken` RuntimeEvent
5. La reponse complete est analysee :
   - Si `tool_call` detecte : verifier l'autorisation → executer ou HITL → reinjecter le resultat → retour a l'etape 2
   - Sinon : sauvegarder le message assistant → fin de l'echange
6. Le `StepBudget` est decremente a chaque iteration de la boucle

Le prompt systeme est configurable par session. La valeur par defaut fournit les instructions ReAct standard avec la liste des outils disponibles.

### Chat Agent — AgentChatExecutor

Le mode Agent delegue l'execution a un agent Python existant via le bridge PyO3.

Flux d'execution :

1. L'historique de la session est converti en `AIPTask` via `session_to_task()`
2. L'agent est charge via `AgentLoader` et valide
3. Un `RuntimeContext` est cree avec les composants necessaires (tools, memory, llm, budget)
4. `AIPBridge.call_run()` est appele directement — le `TaskRouter` n'est pas implique
5. Le resultat est ajoute comme message assistant
6. Si `AIPResult.input_required()` retourne `true`, le flux bascule vers `ChatApprovalRequired`

Le mode Agent ne supporte pas le streaming token-by-token : la reponse est retournee en bloc.

---

## Types principaux

### ChatSession

```rust
pub struct ChatSession {
    pub id: String,
    pub mode: ChatMode,
    pub agent_name: Option<String>,
    pub system_prompt: Option<String>,
    pub status: SessionStatus,
    pub history: Vec<ChatMessage>,
    pub authorized_tools: HashSet<String>,
    pub available_tools: Vec<String>,
    pub created_at: String,
    pub active_exchange: Option<ExchangeState>,
}
```

Le champ `authorized_tools` contient les outils approuves via `AlwaysAccept` pour cette session. Le champ `active_exchange` est `Some` uniquement pendant le traitement d'un message (statut `Processing`).

### ChatMessage

```rust
pub struct ChatMessage {
    pub id: String,
    pub role: ChatRole,
    pub content: String,
    pub tool_calls: Vec<ToolCallRecord>,
    pub tool_name: Option<String>,
    pub created_at: String,
    pub seq: i64,
}
```

Le champ `seq` est un entier croissant par session, utilise pour l'ordre d'affichage et la pagination. Le champ `tool_name` est renseigne uniquement pour les messages de role `Tool` (resultat d'execution d'un outil).

### Enums

| Enum | Variants | Description |
|---|---|---|
| `ChatMode` | `Libre`, `Agent` | Mode d'execution de la session |
| `SessionStatus` | `Active`, `Processing`, `Closed` | Etat du cycle de vie |
| `ChatRole` | `User`, `Assistant`, `System`, `Tool` | Role de l'emetteur du message |
| `ToolDecision` | `Accept`, `Refuse`, `AlwaysAccept` | Decision de l'operateur sur un appel d'outil |
| `ToolCallStatus` | `Pending`, `Authorized`, `Executed`, `Refused` | Etat d'un appel d'outil dans la boucle ReAct |

### ChatError

`ChatError` est defini via `thiserror` avec 7 variants :

| Variant | Description |
|---|---|
| `SessionNotFound` | Session inconnue |
| `SessionClosed` | Tentative d'operation sur une session fermee |
| `AlreadyProcessing` | Un echange est deja en cours sur cette session |
| `ToolNotAvailable` | Outil demande absent de la liste `available_tools` |
| `ToolApprovalTimeout` | Delai d'approbation depasse (5 minutes par defaut) |
| `LlmError` | Erreur du backend LLM (via `#[from]`) |
| `AgentError` | Erreur d'execution de l'agent Python |

---

## HITL inline (approbation d'outils)

En mode Chat, **tous les outils necessitent une approbation** avant execution. Cette politique est plus restrictive que les taches en arriere-plan, ou seuls les outils listes dans `tools_requiring_approval` du manifeste sont soumis a validation.

Trois decisions sont possibles :

| Decision | Effet |
|---|---|
| `Accept` | Execute l'outil une seule fois, approbation non memorisee |
| `Refuse` | Injecte un message de refus dans l'historique, la boucle ReAct continue sans executer |
| `AlwaysAccept` | Ajoute l'outil a `authorized_tools` de la session, execute, les appels suivants sont automatiquement autorises |

La decision `AlwaysAccept` est persistee dans la table `chat_tool_authorizations` de SQLite et restauree au redemarrage.

### Mecanisme interne

Le composant `PendingChatApprovals` gere les approbations en attente :

1. `register(session_id, tool_name)` cree un `oneshot::channel` et retourne le `Receiver<ToolDecision>`
2. `ChatApprovalRequired` RuntimeEvent est emis, declenche une notification desktop via `NotificationEngine` (canal `chat.approval_required`)
3. L'operateur repond via l'API REST ou l'IPC Tauri
4. `resolve(session_id, tool_name, decision)` envoie la decision via le `oneshot::Sender`
5. Si aucune reponse apres 5 minutes, `start_timeout()` envoie automatiquement `Refuse`

Le timeout de 5 minutes est configurable.

---

## Persistance SQLite

La base `chat.db` contient 3 tables. Elle est ouverte par le `ChatSessionManager` au demarrage et placee dans le repertoire standard `~/.apollia/`.

### chat_sessions

```sql
CREATE TABLE chat_sessions (
    id TEXT PRIMARY KEY,
    mode TEXT NOT NULL,
    agent_name TEXT,
    system_prompt TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    available_tools TEXT,
    created_at TEXT NOT NULL,
    closed_at TEXT
);
```

Le champ `available_tools` est stocke comme JSON array serialise (`serde_json`). Le champ `closed_at` est renseigne lors du passage a `Closed`.

### chat_messages

```sql
CREATE TABLE chat_messages (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES chat_sessions(id),
    role TEXT NOT NULL,
    content TEXT NOT NULL DEFAULT '',
    tool_calls_json TEXT,
    tool_name TEXT,
    created_at TEXT NOT NULL,
    seq INTEGER NOT NULL
);
```

Le champ `tool_calls_json` contient la serialisation JSON du `Vec<ToolCallRecord>`. Il est `NULL` pour les messages sans appel d'outil.

### chat_tool_authorizations

```sql
CREATE TABLE chat_tool_authorizations (
    session_id TEXT NOT NULL REFERENCES chat_sessions(id),
    tool_name TEXT NOT NULL,
    authorized_at TEXT NOT NULL,
    PRIMARY KEY (session_id, tool_name)
);
```

Cette table persiste les decisions `AlwaysAccept`. Au demarrage, le `ChatSessionManager` restaure les autorisations pour toutes les sessions actives.

---

## RuntimeEvent (12 variants)

Le sous-systeme Chat emet 12 variants de `RuntimeEvent` via l'`EventBusSender` :

```rust
ChatSessionCreated { session_id: String, mode: ChatMode }
ChatSessionClosed { session_id: String }
ChatMessageSent { session_id: String, message_id: String }
ChatResponseStarted { session_id: String, message_id: String }
ChatToken { session_id: String, message_id: String, token: String }
ChatResponseCompleted { session_id: String, message_id: String, content: String }
ChatError { session_id: String, error: String }
ChatToolCallStarted { session_id: String, tool_name: String }
ChatToolCallCompleted { session_id: String, tool_name: String, success: bool }
ChatApprovalRequired { session_id: String, message_id: String, tool_name: String }
ChatApprovalResolved { session_id: String, tool_name: String, decision: ToolDecision }
ChatApprovalTimeout { session_id: String, tool_name: String }
```

`ChatToken` est emis a haute frequence (un par token genere). Les consommateurs SSE et Tauri traitent cet evenement sur un fast path dedie pour eviter la latence.

---

## API REST (7 endpoints)

Tous les endpoints sont montes sous le prefixe `/api/v1/sessions`.

| Methode | Route | Corps | Reponse | Description |
|---|---|---|---|---|
| `POST` | `/api/v1/sessions` | `CreateSessionRequest` | `201 ChatSessionResponse` | Cree une session |
| `GET` | `/api/v1/sessions` | — | `200 Vec<ChatSessionSummary>` | Liste les sessions |
| `GET` | `/api/v1/sessions/:id` | — | `200 ChatSessionDetail` | Detail complet (historique inclus) |
| `DELETE` | `/api/v1/sessions/:id` | — | `204` | Ferme une session |
| `POST` | `/api/v1/sessions/:id/messages` | `SendMessageRequest` | `202 { message_id }` | Envoie un message |
| `POST` | `/api/v1/sessions/:id/authorize` | `AuthorizeToolRequest` | `200` | Resout une approbation |
| `GET` | `/api/v1/sessions/:id/stream` | — | `200 text/event-stream` | Flux SSE de la session |

Le endpoint `POST /messages` retourne `202 Accepted` car le traitement est asynchrone : la reponse de l'agent arrive via le flux SSE.

Le flux SSE (`/stream`) emet les `RuntimeEvent` chat filtres par `session_id`. Les types d'evenements SSE nommes correspondent aux variants : `chat-token`, `chat-response-completed`, `chat-tool-call-started`, `chat-approval-required`, etc.

---

## Tauri IPC (6 commandes)

| Commande | Parametres | Retour | Description |
|---|---|---|---|
| `create_chat_session` | `mode`, `agent_name?`, `tools?`, `system_prompt?` | `ChatSessionResponse` | Cree une session |
| `list_chat_sessions` | `status?` | `Vec<ChatSessionSummary>` | Liste les sessions |
| `get_chat_session` | `session_id` | `ChatSessionDetail` | Detail complet (messages, autorisations) |
| `close_chat_session` | `session_id` | `()` | Ferme une session |
| `send_chat_message` | `session_id`, `content` | `{ message_id }` | Envoie un message |
| `authorize_chat_tool` | `session_id`, `tool_name`, `decision` | `()` | Resout une approbation |

### Event bridge Tauri

Les evenements chat transitent du `RuntimeEvent` vers le frontend Svelte via le bridge Tauri :

- La majorite des evenements chat emettent `"chat-changed"`, ce qui declenche un refresh de la liste des sessions et de la session courante cote frontend.
- Exception : `ChatToken` emet `"chat-token"` avec le payload `{ session_id, message_id, token }`. Ce fast path evite un appel IPC complet pour chaque token et permet l'affichage streaming cote Svelte.

---

## Frontend Svelte (8 composants)

### Composants

| Composant | Fichier | Role |
|---|---|---|
| `Chat.svelte` | `src/routes/chat/Chat.svelte` | Route principale `/chat`, layout split (liste + conversation) |
| `NewChatDialog.svelte` | `src/lib/components/chat/NewChatDialog.svelte` | Dialogue de creation (selection mode, agent, outils, prompt systeme) |
| `ChatSessionCard.svelte` | `src/lib/components/chat/ChatSessionCard.svelte` | Carte resume dans la sidebar (badge mode, dernier message, timestamp) |
| `ChatConversation.svelte` | `src/lib/components/chat/ChatConversation.svelte` | Vue conversation (header session, historique scrollable, input) |
| `ChatMessageBubble.svelte` | `src/lib/components/chat/ChatMessageBubble.svelte` | Bulle message (User aligne a droite, Assistant a gauche) |
| `ChatInput.svelte` | `src/lib/components/chat/ChatInput.svelte` | Textarea auto-resize (Enter = envoyer, Shift+Enter = saut de ligne) |
| `StreamingText.svelte` | `src/lib/components/chat/StreamingText.svelte` | Affichage progressif token-by-token (mode Libre uniquement) |
| `ToolCallCard.svelte` | `src/lib/components/chat/ToolCallCard.svelte` | Affichage d'un appel d'outil (input JSON, output, badge status) |
| `ApprovalCard.svelte` | `src/lib/components/chat/ApprovalCard.svelte` | Carte approbation HITL (3 boutons : Accept, Refuse, Always Accept) |

### Stores Svelte

| Store | Type | Description |
|---|---|---|
| `chatSessions` | `writable<ChatSessionSummary[]>` | Liste des sessions, rafraichie sur `"chat-changed"` |
| `currentSession` | `writable<ChatSessionDetail \| null>` | Session courante avec historique complet |
| `chatTokenBuffer` | `writable<string>` | Accumulation des tokens recus via `"chat-token"`, vide a la reception de `ChatResponseCompleted` |

Le store `chatTokenBuffer` est consomme par `StreamingText.svelte` pour l'affichage progressif. A la reception de `ChatResponseCompleted`, le buffer est vide et le message complet est ajoute a l'historique de `currentSession`.

---

## Mode-aware tool card rendering *(Sprint 25)*

Le Sprint 25 introduit un rendu conditionnel des appels d'outils dans le frontend Svelte, pilote par le store `uiMode`. Selon la valeur de ce store, chaque appel d'outil est affiche sous une forme adaptee a l'audience : phrase lisible pour l'operateur final, details techniques pour le developpeur.

### Deux modes de rendu

Le store `uiMode` accepte deux valeurs :

| Valeur | Audience | Style d'affichage |
|---|---|---|
| `"operator"` | Utilisateur final | Phrase humaine, icone, indicateur de statut — sans JSON expose |
| `"builder"` | Developpeur / debug | Nom technique, JSON entree/sortie, previews specialisees |

`ChatMessageBubble.svelte` lit `$uiMode` a chaque rendu et instancie le composant correspondant en lieu et place de l'ancien `ToolCallCard.svelte`.

### OperatorToolCard.svelte

Composant cible quand `$uiMode === "operator"`.

- Affiche une phrase humaine construite a partir des cles i18n : par exemple "Lecture de rapport.pdf" pour un appel `read_file`.
- Integre une icone Lucide fournie par `resolveToolDisplay()`.
- Affiche un indicateur de statut anime :
  - Spinner (roue) pendant `Pending` / `Authorized`
  - Icone check verte pour `Executed`
  - Icone X rouge pour `Refused`
- Affiche un resume de la sortie (`outputSummaryKey` + `outputParams`) une fois l'outil execute.
- N'expose aucun JSON brut a l'utilisateur final.

### BuilderToolCard.svelte

Composant cible quand `$uiMode === "builder"` (valeur par defaut).

- Affiche le nom technique de l'outil.
- Section "Entree" : JSON brut de l'appel, repliable via `<details>`.
- Section "Sortie" : JSON brut du resultat, repliable, visible uniquement apres execution.
- Previews specialisees selon le type d'outil :
  - **bash** : bloc code avec syntaxe shell.
  - **http** : methode + URL en evidence, corps repliable.
- Equivalent enrichi de l'ancien `ToolCallCard.svelte`, conserve pour les sessions de debug.

### OperatorApprovalCard.svelte

Carte d'approbation HITL adaptee au mode operateur. Elle remplace `ApprovalCard.svelte` quand `$uiMode === "operator"`.

- Description humaine de l'outil en attente (via i18n), sans JSON.
- Deux actions uniquement : **Approuver** et **Refuser** — le bouton "Toujours accepter" (`AlwaysAccept`) est retire de la vue operateur.
- `ApprovalCard.svelte` (existant) reste utilise en mode `"builder"` et conserve les trois decisions (`Accept`, `Refuse`, `AlwaysAccept`).

### Module tool-display.ts

Fichier : `src/lib/chat/tool-display.ts`

Ce module centralise la logique de presentation des outils natifs.

```typescript
interface ToolDisplayInfo {
  icon: LucideIcon;          // composant Lucide a afficher
  labelKey: string;          // cle i18n du nom court de l'outil
  descriptionKey: string;    // cle i18n de la phrase humaine (supporte templateParams)
  templateParams?: string[]; // noms des champs de l'input a interpoler dans la description
  outputSummaryKey?: string; // cle i18n du resume de sortie
  outputParams?: string[];   // noms des champs de l'output a interpoler dans le resume
}

function resolveToolDisplay(toolCall: ToolCallView): ToolDisplayInfo
```

`resolveToolDisplay` couvre les 10 outils natifs du Tool Registry. Pour tout outil inconnu, il retourne une entree generique avec l'icone `Wrench` et les cles `tools.unknown.*`.

### Support i18n

Les cles de traduction sont definies dans les deux catalogues (EN et FR) sous l'espace de noms `tools.*`.

Structure de cle :

```
tools.<tool_name>.label          — nom court (ex. "Lecture de fichier")
tools.<tool_name>.description    — phrase humaine avec placeholders {param} (ex. "Lecture de {path}")
tools.<tool_name>.output_summary — resume de sortie avec placeholders (ex. "{lines} lignes lues")
```

Les 10 outils natifs documentes : `read_file`, `write_file`, `list_dir`, `bash`, `http_request`, `search_memory`, `store_memory`, `web_search`, `read_url`, `send_notification`.

---

## Voir aussi

- [ADR-034 — Chat hybride](../adr/ADR-034-chat-hybride-sessions-streaming-hitl-inline.md) — decision architecturale
- [Runtime Core](./Briques-Runtime-Core.md) — Supervisor et acteurs Tokio
- [Notifications](./Briques-Notifications.md) — evenement `chat.approval_required`
- [Desktop](./Briques-Desktop.md) — commandes Tauri IPC
- [API HTTP Reference](./API-HTTP-Reference.md) — reference complete des endpoints
