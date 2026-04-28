# Chat — Sous-systeme conversationnel

> Page source de verite pour le sous-systeme Chat introduit.
> Derniere mise a jour :.

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
3. `LlmRouter.stream` produit un flux de tokens
4. Chaque token est emis comme `ChatToken` RuntimeEvent
5. La reponse complete est analysee :
   - Si `tool_call` detecte : verifier l'autorisation → executer ou HITL → reinjecter le resultat → retour a l'etape 2
   - Sinon : sauvegarder le message assistant → fin de l'echange
6. Le `StepBudget` est decremente a chaque iteration de la boucle

Le prompt systeme est configurable par session. La valeur par defaut fournit les instructions ReAct standard avec la liste des outils disponibles.

### CompositeToolInvoker — routing A2A depuis le chat libre

Le `CompositeToolInvoker` est introduit par pour permettre au Chat Libre d'invoquer des Worker Agents via A2A sans que l'utilisateur ne sache que la délégation a eu lieu.

**Principe** : chaque skill des agents actifs avec `supports_a2a: true` est exposé comme un outil virtuel préfixé `a2a:` dans la liste des outils disponibles du `BuiltInChatAgent`. Le LLM voit `[file_read, bash_executor,..., a2a:read-pdf, a2a:extract-tables,...]` et choisit naturellement l'outil A2A quand le domaine correspond.

**Architecture** :

```
BuiltInChatAgent
  └── tool_invoker: Arc<dyn ToolInvoker>
        ├── NativeChatToolInvoker  (outils natifs — comportement inchangé)
        └── CompositeToolInvoker   (si agents A2A actifs)
              ├── native: NativeChatToolInvoker
              └── a2a: Arc<A2AInvoker>
```

Le `ChatSessionManager` instancie un `CompositeToolInvoker` si `a2a_invoker` est disponible, sinon revient au `NativeChatToolInvoker` seul (backward-compatible).

**Comportement de routage** :

1. `tool_name.strip_prefix("a2a:")` → extrait le `skill_id`
2. `arguments["text"]` → extrait la requête textuelle
3. `A2AInvoker.invoke(skill_id,...)` → délègue à l'agent Worker
4. Si `status == "completed"` → `Ok("[{skill_id} via {agent_name}]\n{output_text}")`
5. Si `status == "failed"` → `Err("Agent {name} a échoué : {message}")` propagé au chat
6. Si pas de préfixe `a2a:` → `NativeChatToolInvoker.invoke(...)` (fallback)

**Génération des ToolSpec virtuels** :

La fonction `generate_a2a_tool_specs(a2a_invoker)` (fichier `chat/a2a_tools.rs`) itère les agents actifs et crée une `ToolSpec` par skill :
- Nom : `"a2a:{skill_id}"`
- Description : `"{skill.description} (via {agent_name})"`
- Input schema : `{"type": "object", "properties": {"text": {"type": "string"}}, "required": ["text"]}`

Si aucun agent n'a `supports_a2a: true`, la liste des outils virtuels est vide — pas d'outils `a2a:` exposés.

**Approbation HITL** : les outils `a2a:*` sont auto-approuvés dans la session chat. L'agent Worker cible gère ses propres gardes-fous internes.

### Chat Agent — AgentChatExecutor

Le mode Agent delegue l'execution a un agent Python existant via le bridge PyO3.

Flux d'execution :

1. L'historique de la session est converti en `AIPTask` via `session_to_task`
2. L'agent est charge via `AgentLoader` et valide
3. Un `RuntimeContext` est cree avec les composants necessaires (tools, memory, llm, budget)
4. `AIPBridge.call_run` est appele directement — le `TaskRouter` n'est pas implique
5. Le resultat est ajoute comme message assistant
6. Si `AIPResult.input_required` retourne `true`, le flux bascule vers `ChatApprovalRequired`

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
5. Si aucune reponse apres 5 minutes, `start_timeout` envoie automatiquement `Refuse`

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

## Tauri IPC (10 commandes)

| Commande | Parametres | Retour | Description |
|---|---|---|---|
| `create_chat_session` | `mode`, `agent_name?`, `tools?`, `system_prompt?` | `ChatSessionResponse` | Cree une session |
| `list_chat_sessions` | `status?` | `Vec<ChatSessionSummary>` | Liste les sessions |
| `get_chat_session` | `session_id` | `ChatSessionDetail` | Detail complet (messages, autorisations) |
| `close_chat_session` | `session_id` | `` | Ferme une session (conserve le transcript) |
| `delete_chat_session` | `session_id` | `` | Supprime definitivement une session |
| `rename_chat_session` | `session_id`, `title` | `` | Renomme (title max 100 caracteres) |
| `update_chat_session` | `session_id`, `request` | `` | Met a jour les metadonnees (mode, agent, outils) |
| `generate_chat_session_name` | `session_id`, `first_message` | `String` (titre) | Genere automatiquement un titre court via LLM a partir du premier message utilisateur (max 60 caracteres, compatible reasoning models) |
| `send_chat_message` | `session_id`, `content` | `{ message_id }` | Envoie un message |
| `authorize_chat_tool` | `session_id`, `tool_name`, `decision` | `` | Resout une approbation |

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

## Mode-aware tool card rendering

Le introduit un rendu conditionnel des appels d'outils dans le frontend Svelte, pilote par le store `uiMode`. Selon la valeur de ce store, chaque appel d'outil est affiche sous une forme adaptee a l'audience : phrase lisible pour l'operateur final, details techniques pour le developpeur.

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
- Integre une icone Lucide fournie par `resolveToolDisplay`.
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

## Injection memoire utilisateur

Le mode Chat Libre enrichit automatiquement le prompt systeme avec le profil memorise de l'utilisateur a chaque nouvelle session. Ce mecanisme est implementé dans `build_system_prompt` (`chat/builtin_agent.rs`).

### Fonctionnement

Quand `BuiltInChatAgent` est cree avec un `UserMemoryRepository` (parametre optionnel), il appelle `repo.recall_persona_brief(30)` avant chaque echange. Cette methode :

1. Appelle `SemanticMemory::recall_all("__user__")` — lit toutes les entrees stockees sous le namespace reserve `__user__`
2. Filtre les entrees avec `confidence < 0.3` (entrées de faible confiance ignorées)
3. Retourne au maximum 30 entrees formatees sous forme de bloc narratif

Si le bloc retourne est non-vide, il est injecte dans le prompt systeme sous la section `## User Persona` :

```
## User Persona
Follow the adaptation instructions below to personalize every response.
Do not repeat this information back to the user unless asked.

<bloc narrative des entrees __user__>
```

Le prompt systeme inclut egalement une section `## System Environment` (OS, architecture, home dir, working dir) injectee inconditionnellement.

### Principe #6 respecte

Conformement au Principe #6 (Mémoire à initiative de l'agent), les entrées `__user__` ne sont **jamais** injectées automatiquement dans le contexte des taches en arriere-plan ni dans le mode Chat Agent. L'injection est limitée au prompt systeme du mode Chat Libre (`BuiltInChatAgent`). L'agent reste maitre de l'utilisation de cette information.

### Source de verite

- Injection : `crates/apollia-runtime/src/chat/builtin_agent.rs` — `BuiltInChatAgent::build_system_prompt`
- Stockage et format : `crates/apollia-memory/src/user_memory.rs` — `UserMemoryRepository::recall_persona_brief`
- Namespace reserve : `const USER_NAMESPACE: &str = "__user__"`

---

## Resume de conversation

Quand une session Chat Libre depasse le seuil de la fenetre de contexte, le sous-systeme declenche automatiquement une summarisation de la partie ancienne de l'historique.

### Seuil de declenchement

Le seuil est defini par `DEFAULT_CONTEXT_WINDOW_SIZE = 20` (messages). Apres chaque echange, la condition suivante est evaluee :

```
history.len() > DEFAULT_CONTEXT_WINDOW_SIZE && stored_summary.is_none()
```

Si vraie, les `history.len - 20` messages les plus anciens (ceux hors de la fenetre glissante) sont soumis a la summarisation. La summarisation n'est declenchee qu'une seule fois par session : si un resume est deja stocke (`stored_summary.is_some`), il est reutilise tel quel.

### ConversationSummarizer

La summarisation est realisee par une fonction async `summarize(messages, llm)` dans `chat/summarizer.rs`. Elle construit un `CompletionRequest` avec :

- Prompt systeme : instruction de produire un resume en 2-3 paragraphes, focusse sur les decisions cles, le contexte etabli, et les questions non resolues
- Contenu : la transcription des messages anciens (`role: contenu\n`)
- `max_tokens: Some(500)` — cap absolu a **500 tokens** pour eviter de surcharger la fenetre de contexte lors de la reinsertion

Le LLM appele est le router par defaut de la session (`LlmRouter`). Le resume est retourne comme `String` brute.

### Persistance et indexation FTS5

Apres la generation, le resume est envoye via la commande interne `PersistSummary` au `ChatSessionManager`. L'acteur le stocke dans la colonne `summary` de la table `chat_sessions` (migration v3).

Simultanement, la table virtuelle FTS5 `chat_sessions_fts` (migration v4) est mise a jour :

```sql
CREATE VIRTUAL TABLE IF NOT EXISTS chat_sessions_fts USING fts5(
    session_id UNINDEXED,
    created_at UNINDEXED,
    summary
);
```

Seule la colonne `summary` est indexee en full-text. Les colonnes `session_id` et `created_at` sont `UNINDEXED` (stockees mais non recherchables).

### Source de verite

- Seuil et logique de declenchement : `crates/apollia-runtime/src/chat/manager.rs`, fonction `handle_send_message`
- Summariseur : `crates/apollia-runtime/src/chat/summarizer.rs`
- Migration FTS5 : `crates/apollia-runtime/src/chat/repository.rs`, migration v4

---

## Cross-session recall

Lors de la **premiere** requete d'une nouvelle session Chat Libre, le `ChatSessionManager` peut injecter des resumes de sessions passees pertinentes dans le prompt systeme.

### Declenchement

La logique de recall est activee uniquement quand `history.len == 1` (premiere reponse de la session). Elle est ignoree si le premier message est trop court :

```rust
const MIN_MESSAGE_LENGTH_FOR_RECALL: usize = 20;
```

Les messages courts (salutations comme "bonjour", "hello") ne declenchent pas de recall — evite d'injecter un contexte hors-sujet.

### Recherche FTS5 (BM25)

La methode `repository.find_relevant_sessions(query, limit)` execute une requete FTS5 sur la table `chat_sessions_fts` :

```sql
SELECT session_id, created_at, summary
FROM chat_sessions_fts
WHERE summary MATCH ?1
ORDER BY rank
LIMIT ?2
```

Le classement `ORDER BY rank` utilise le scoring **BM25** natif de FTS5. La requete est sanitisee avant envoi (`sanitize_fts_query`) pour echapper les caracteres speciaux FTS.

Le nombre maximum de sessions retournees est defini par :

```rust
const MAX_PAST_SESSIONS: usize = 3;
```

Seules les sessions **fermees** (`status = 'closed'`) avec un resume non-null sont indexees dans `chat_sessions_fts`.

### Injection dans le prompt systeme

Les sessions trouvees sont formatees et prepend au prompt systeme de la session courante :

```
## Previous conversations (for reference)
- [2026-03-15T14:22:00Z] The conversation covered the migration of the billing module...
- [2026-03-08T09:11:00Z] The user set up the CI/CD pipeline with GitHub Actions...
```

Ce bloc est injecte **une seule fois**, au premier echange. Les echanges suivants de la meme session utilisent le prompt enrichi tel quel, sans nouvelle recherche FTS.

### Type PastSessionSummary

```rust
pub struct PastSessionSummary {
    pub session_id: String,
    pub created_at: String,
    pub summary: String,
}
```

### Source de verite

- Constantes et logique : `crates/apollia-runtime/src/chat/manager.rs` — `build_cross_session_context` et constantes `MAX_PAST_SESSIONS`, `MIN_MESSAGE_LENGTH_FOR_RECALL`
- Requete FTS5 : `crates/apollia-runtime/src/chat/repository.rs` — `find_relevant_sessions`
- Type : `crates/apollia-runtime/src/chat/types.rs` — `PastSessionSummary`

---

## Voir aussi

- [ADR-034 — Chat hybride](../adr/ADR-034-chat-hybride-sessions-streaming-hitl-inline.md) — decision architecturale
- [Runtime Core](./Briques-Runtime-Core.md) — Supervisor et acteurs Tokio
- [Memory Engine](./Briques-Memory-Engine.md) — UserMemoryRepository et namespace `__user__`
- [Notifications](./Briques-Notifications.md) — evenement `chat.approval_required`
- [Desktop](./Briques-Desktop.md) — commandes Tauri IPC
- [API-HTTP-Agents](./API-HTTP-Agents#chat-sprint-18) — reference complete des endpoints Chat
