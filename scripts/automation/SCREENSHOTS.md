# Screenshot shooting script

One row per image the documentation needs: eighty-five, and the same eighty-five
twice, because each locale now has its own set. The English pages point at
`/img/operator-help/en/`, the French mirror at `/img/operator-help/fr/`.

## Before you start

```sh
bash scripts/automation/seed/load.sh     # moves ~/.apollia aside, installs the seed
# ... shoot ...
bash scripts/automation/seed/unload.sh   # puts your profile back
```

`load.sh` moves rather than copies, so nothing is duplicated, and it refuses to
run if a backup already exists rather than overwrite what may be the only copy
of a real profile. `unload.sh` refuses while the application is running, because
SQLite in WAL mode keeps files open next to each database.

The seed's timestamps are relative to the moment it is built, so the timeline and
the audit trail have entries inside their default windows. Coming back days
later, reload it.

## The two passes

Switch the language in **Settings > Appearance**, then do a whole pass before
switching. English into `en/`, French into `fr/`.

## Naming

Save each file under exactly the name in the **File** column, no prefix, no
suffix:

```
docs/site/static/img/operator-help/en/<file>
docs/site/static/img/operator-help/fr/<file>
```

That name is what the pages already reference, so a correctly named file replaces
the old image with no edit anywhere. A misnamed one is invisible: nothing breaks,
the stale image simply stays. That silence is the reason to check the names at
the end rather than trust them.

## Framing

Frame the useful region, not the whole window. What dates a screenshot is the
chrome around it. Keep one crop for a page's whole series so its images sit
together.

## Reading the Status column

- **seeded**: the seed already puts it on screen, shoot as is.
- **live**: the screen only exists while something runs, so provoke it. Send a
  message, let the agent ask for an approval, then shoot. No seed can produce
  these: the pending list lives in the runtime's memory, not in a database.
- **blocked**: needs something this machine does not have, named in the cell.


## Installation and setup

| # | File | What must be on screen | Where you are | Status |
|---|---|---|---|---|
| 1 | `installation-configurer-votre-profil-1.png` | Welcome window with the Apollia logo, subtitle "The sovereign runtime to run your AI agents locally", thr | Step 1 - Welcome | seeded |
| 2 | `installation-configurer-votre-profil-2.png` | Profile step with two cards Operator (sparkles icon) and Builder (code icon), each with 3 bullets and a s | Step 2 - Choose your profile | seeded |
| 3 | `installation-configurer-votre-profil-3.png` | Models step, RAM · macOS · GPU banner, LLM section with a curated list of Qwen3 models and a "Recommended | Step 3 - Configure the AI engine | seeded |
| 4 | `installation-configurer-votre-profil-4.png` | Qwen3 download in progress with a progress bar and throughput, plus a Whisper model downloaded in paralle | Cloud provider. The Use a cloud provider button closes the window and takes you to the LLM Back | **blocked**: a real model download in progress |
| 5 | `installation-configurer-votre-profil-5.png` | Calibration step with 4 progress pips at the top, the onboarding agent asking the first question and the  | Step 4 - Conversational calibration | **live** |
| 6 | `installation-configurer-votre-profil-6.png` | Permission rule cards suggested by the agent at the end of calibration: deny http_fetch on api.openai.com | Step 4 - Conversational calibration | **live** |
| 7 | `installation-configurer-votre-profil-7.png` | Settings → Profile page after onboarding, Identity (first name, role, sector), Goals, and Agent supervisi | Verification | seeded |
| 8 | `installation-connecter-un-modele-distant-1.png` | Add LLM backend dialog, empty, with the Name and Provider fields | Click + Add LLM backend at the top. A configuration window opens. | seeded |
| 9 | `installation-telecharger-des-modeles-locaux-1.png` | Model Hub page, list of available models with Name, Size, Type, Status columns | In the sidebar, click Settings, then the Model Hub section. | seeded |
| 10 | `installation-telecharger-des-modeles-locaux-1bis.png` | Model Hub: the Installed models section, with the active model marked by an In use badge | (Optional) Click Set as default to use this model automatically in new chats (GGUF) or for dict | seeded |
| 11 | `installation-telecharger-des-modeles-locaux-2.png` | model row "Llama 3.1 8B" with a progress bar at 42 % and a Cancel button | Click Download. A progress bar appears next to the model. | **blocked**: a real model download in progress |

## Cross-cutting

| # | File | What must be on screen | Where you are | Status |
|---|---|---|---|---|
| 12 | `transversal-activer-la-compagnonne-ia-1.png` | The dashboard, with the sidebar and the entry point to Apollia Help | In the sidebar, find the Apollia Help button (Apollia logo, at the bottom of the sidebar). | seeded |
| 13 | `transversal-activer-la-compagnonne-ia-2.png` | Apollia Help panel open, with its welcome message and the input area | Ask a quick question. Apollia Help answers without interrupting your work on the main page. | seeded |
| 14 | `transversal-naviguer-au-clavier-command-palette-1.png` | command palette open in the middle of the screen, search field at the top, grouped suggestions below | From any screen, press Cmd+K (macOS) or Ctrl+K (Windows and Linux). The palette opens in the mi | seeded |
| 15 | `transversal-naviguer-au-clavier-command-palette-2.png` | Settings then Shortcuts page, with the search bar at the top and the shortcuts grouped by category | Full list of shortcuts | seeded |
| 16 | `transversal-utiliser-l-inbox-1.png` | Inbox on the To do tab, with the counter chips in the tab bar and the filter chips below | The "To do" tab | seeded |
| 17 | `transversal-utiliser-l-inbox-2.png` | An expanded ask_user form, with its context callout at the top followed by the questions to answer | Answer an agent question (ask_user) | **live** |
| 18 | `transversal-utiliser-l-inbox-3.png` | Inbox on the Activity tab, with its four filter chips and the list of event cards | The "Activity" tab | seeded |
| 19 | `transversal-utiliser-l-inbox-4.png` | Inbox on the Notifications sent tab, with the channel filter and the four-column delivery table | The "Notifications sent" tab | seeded |

## Chat

| # | File | What must be on screen | Where you are | Status |
|---|---|---|---|---|
| 20 | `chat-activer-la-dictee-vocale-1.png` | Settings page, Speech-to-Text section, Whisper model status shown at the top | In the sidebar, click Settings, then the Speech-to-Text section. | seeded |
| 21 | `chat-activer-la-dictee-vocale-2.png` | HotkeyCapture window with the message "Press your hotkey combination" and the captured combination | Click the Global hotkey field. A window prompts you to press the key combination you want (for  | seeded |
| 22 | `chat-discuter-avec-votre-ia-1.png` | Chat page, conversation sidebar on the left, empty area in the middle with the input field at the bottom | In the sidebar, click Chat. The list of your conversations shows on the left, the input area in | seeded |
| 23 | `chat-discuter-avec-votre-ia-2.png` | conversation with a user message and an AI answer streaming in, markdown formatting rendered | Press Enter or click Send. The answer streams in, word by word. | **live** |
| 24 | `chat-discuter-avec-votre-ia-2bis.png` | conversation with a user message and an AI answer streaming in, markdown formatting rendered (continued) | Press Enter or click Send. The answer streams in, word by word. | **live** |
| 25 | `chat-discuter-avec-votre-ia-3.png` | answer bubble with an expanded reasoning card showing the agent's steps | If you are talking to an Assistant, the reasoning steps show up inline in the message bubbles a | **live** |

## Agents

| # | File | What must be on screen | Where you are | Status |
|---|---|---|---|---|
| 26 | `agents-consulter-les-logs-d-un-agent-1.png` | Logs panel open with counter, search bar, status filters and sorting | Click Logs on its card. A panel opens on the right, titled Agent Logs, with a task counter at t | seeded |
| 27 | `agents-demarrer-un-agent-1.png` | My Assistants page - left column with the two sections "My assistants" and "My packages" visible | In the sidebar, open My Assistants. The left column lists your assistants under My assistants · | seeded |
| 28 | `agents-demarrer-un-agent-2.png` | package detail panel - Information, Agents (with director/worker roles) and Triggers sections | Click the row to open the package detail: there you see the list of the agents it contains, the | seeded |
| 29 | `agents-demarrer-un-agent-2bis.png` | package detail panel - Information, Agents (with director/worker roles) and Triggers sections (continued) | Click the row to open the package detail: there you see the list of the agents it contains, the | seeded |
| 30 | `agents-installer-un-agent-1.png` | My Assistants page: list on the left, detail of the selected agent on the right, "New assistant" button a | In the sidebar, open My Assistants. The page lists your existing assistants, and the New assist | seeded |
| 31 | `agents-installer-un-agent-2.png` | Installation dialog, preview step: Agents and Triggers sections, green Valid badge | Package preview. Apollia shows a summary: name, version, author, the list of the agents in the  | **blocked**: the native folder picker |
| 32 | `agents-installer-un-agent-2bis.png` | Preview with a webhook trigger requiring configuration, Configure → button | Package preview. Apollia shows a summary: name, version, author, the list of the agents in the  | **blocked**: the native folder picker |
| 33 | `agents-installer-un-agent-3.png` | Installation dialog, configure step: webhook trigger card with endpoint URL and HMAC-SHA256 secret field | (Optional) Webhook configuration. If you are asked for it, each webhook requires a secret (at l | **blocked**: the native folder picker |
| 34 | `agents-installer-un-agent-4.png` | Package installed! confirmation screen with agents and triggers counters, Close button | Click Install. Apollia copies the package, registers the agents and activates their triggers. A | **blocked**: a real package install |

## Projects

| # | File | What must be on screen | Where you are | Status |
|---|---|---|---|---|
| 35 | `projets-activer-les-context-providers-1.png` | Context Providers section in the project panel, provider list with ON/OFF toggle | Scroll down to the Context Providers section. Three provider types are available. | seeded |
| 36 | `projets-activer-les-context-providers-2.png` | Git Status provider enabled (green toggle), Directory Tree provider disabled (grey toggle) | Switch each provider to ON or OFF depending on your needs. | seeded |
| 37 | `projets-activer-les-context-providers-3.png` | detailed preview of a context provider with git diff / file tree content | To see exactly what will be handed to the AI, click Preview context (Workspace Snapshot). A col | seeded |
| 38 | `projets-creer-un-projet-1.png` | Projects page, + New Project button highlighted at the top right | Click + New Project at the top right. | seeded |
| 39 | `projets-creer-un-projet-2.png` | New Project modal with Name, Root folder and Template fields | (Optional) Pick a project template in the drop-down list. The template pre-enables the matching | seeded |
| 40 | `projets-creer-un-projet-3.png` | Project detail panel opened as a side sheet, with its Description, Agents, Context Providers, Documents a | Click the project card to open its detail panel (side Sheet). It shows the path, the linked age | seeded |
| 41 | `projets-lier-un-projet-a-un-chat-1.png` | project detail page, + New Chat button highlighted | From the Projects page: click Projects in the sidebar, open the project, then click + New Chat  | seeded |
| 42 | `projets-lier-un-projet-a-un-chat-2.png` | chat header, drop-down menu with the Link to a project option | From an existing chat: open the chat, click the menu at the top (three dots), then click Link t | seeded |
| 43 | `projets-lier-un-projet-a-un-chat-3.png` | project page with the list of linked chats, each with its title and date | You can create several chats linked to the same project. Each keeps its own history but shares  | seeded |

## Automations

| # | File | What must be on screen | Where you are | Status |
|---|---|---|---|---|
| 44 | `automatisations-programmer-un-trigger-1.png` | Automations page, with the Create an automation button at the top right and the four-step stepper | Click the Create an automation button at the top right. A 4-step wizard opens (Describe → Sched | seeded |
| 45 | `automatisations-programmer-un-trigger-2.png` | Schedule step, with the human-readable schedule box, the next-run line and the refinement fields | Schedule step - Apollia shows how it read your sentence in a box (for example "Every day at 08: | seeded |
| 46 | `automatisations-suivre-l-historique-d-un-trigger-1.png` | Automation row on hover, with its three-dot menu open on View history | Click the ⋯ icon (three dots) on the right of the row → View history. A sliding panel opens fro | seeded |
| 47 | `automatisations-suivre-l-historique-d-un-trigger-2.png` | Trigger run history panel, with the status filter chips at the top and the stacked run cards below | Each row of the list already carries the essentials - no need to click to open a detail view: | seeded |

## Control

| # | File | What must be on screen | Where you are | Status |
|---|---|---|---|---|
| 48 | `controle-approuver-ou-refuser-une-action-1.png` | Inline approval card in the chat, with an orange shield icon, a preview of the command to authorise, and  | Where the approval request appears | **live** |
| 49 | `controle-approuver-ou-refuser-une-action-2.png` | Inbox page with the filter chips at the top and an expanded approval card showing its risk badge | Where the approval request appears | **live** |
| 50 | `controle-approuver-ou-refuser-une-action-3.png` | Reject action dialog with textarea, "12 / 500" counter, Cancel / Confirm rejection buttons at the bottom | Refuse - a Reject action dialog opens. Enter an explanation of 5 to 500 characters (counter at  | **live** |
| 51 | `controle-approuver-ou-refuser-une-action-4.png` | Recent history section - four rows with different icons, one rejection with its reason shown in red | Review the decision history | **live** |
| 52 | `controle-configurer-les-permissions-de-fichiers-1.png` | Settings > Permissions page, list of permission cards (PermissionRuleCard) with scope badges | In the left menu, select Permissions. | seeded |
| 53 | `controle-configurer-les-permissions-de-fichiers-1bis.png` | Revoke all dialog: the scope selector and the revoke button | Check the number of affected rules shown in the dialog, then click Revoke. | seeded |
| 54 | `controle-configurer-les-permissions-de-fichiers-2.png` | permission card with the Revoke button visible, confirmation toast "Rule bash revoked" | A confirmation message appears briefly. The card disappears immediately. | seeded |
| 55 | `controle-configurer-les-permissions-de-fichiers-3.png` | Active sessions section, list of entries with an orange Session badge and a Revoke button | Active sessions | seeded |

## Integrations

| # | File | What must be on screen | Where you are | Status |
|---|---|---|---|---|
| 56 | `integration-cabler-son-propre-serveur-mcp-1.png` | Custom tab of the catalogue: the blank form | In the Connections sidebar, click + Add custom at the top. The panel opens on the Custom tab. | seeded |
| 57 | `integration-cabler-son-propre-serveur-mcp-2.png` | Custom form on stdio transport, with the command and the arguments filled in | stdio case (local command) | seeded |
| 58 | `integration-cabler-son-propre-serveur-mcp-3.png` | Custom form on streamable-http transport, with the URL and the authentication headers | streamable-http case (remote server) | seeded |
| 59 | `integration-comprendre-la-portee-d-une-integration-1.png` | Agent detail page, Tools tab: the list of required and optional tools with their approval badges | On the agent side | seeded |
| 60 | `integration-comprendre-les-permissions-mcp-1.png` | Approval popup in the chat: the tool title, the exposed parameters, the Allow once and Deny buttons, and  | Why this tool asks for an approval | **live** |
| 61 | `integration-comprendre-les-permissions-mcp-2.png` | Settings, Permissions page: the permission rules stacked with a Revoke button on each row | Viewing and changing the rules | seeded |
| 62 | `integration-google-workspace-1.png` | Connections page, Google Workspace card selected in the sidebar (Not connected state), right-hand panel w | In the sidebar, open Connections, then select the Google Workspace card in the list of native c | **blocked**: a real Google account, OAuth consent |
| 63 | `integration-google-workspace-2.png` | Google consent screen, Apollia asks for access to the account, list of permissions (app Drive files, Cale | Pick the Google account to use, then accept the permissions offered (Mail, Calendar, Drive Work | **blocked**: the Google consent screen, outside the app |
| 64 | `integration-google-workspace-3.png` | Google Drive folder dialog in Apollia, explanation of the drive.file scope, Folder path field with the va | Back in Apollia, the window detects the return automatically. A second step offers you the agen | **blocked**: a real Google account, post-consent dialog |
| 65 | `integration-connecter-un-serveur-mcp-1.png` | Connections page: the catalogue open on the Discover tab, with its grid of entries | In the sidebar, open Connections, then click + Discover at the top. The catalogue opens in a de | seeded |
| 66 | `integration-tester-une-connexion-mcp-1.png` | Connections page: an MCP server selected in the sidebar, its detail page on the right | In the Connections sidebar, select the MCP server to test. | seeded |
| 67 | `integration-tester-une-connexion-mcp-2.png` | Page of an installed MCP server, with the Test button in the actions area | In the detail panel, click the plug icon next to the server name, or Test connection in the act | seeded |
| 68 | `integration-overview-1.png` | Connections page, left sidebar listing the native connectors (Google Workspace, Microsoft 365) and the MC | MCP servers | seeded |

## Memory

| # | File | What must be on screen | Where you are | Status |
|---|---|---|---|---|
| 69 | `memoire-consulter-et-nettoyer-la-memoire-1.png` | Memory page: the namespace list on the left, the type filters and the search in the middle, and the entri | In the sidebar, click Memory. The page shows a two-column layout: the namespace sidebar on the  | seeded |
| 70 | `memoire-consulter-et-nettoyer-la-memoire-2.png` | Detail panel of a memory entry, with its value, its metadata and the Copy and Delete actions | Click an entry to open the detail panel on the right. It shows the full value (with automatic J | seeded |
| 71 | `memoire-gerer-mon-profil-1.png` | Settings then Profile page, showing its stacked sections from Identity down to the danger zone | Where to edit it | seeded |
| 72 | `memoire-gerer-mon-profil-1bis.png` | The profile danger zone with the Reset profile confirmation modal in the foreground | A - Erase only the profile and ask the questions again | seeded |

## Notifications

| # | File | What must be on screen | Where you are | Status |
|---|---|---|---|---|
| 73 | `notifications-choisir-les-evenements-notifies-1.png` | Global events section, grid of 7 checkboxes with label, description and technical identifier | Spot the Global events section at the top of the page: a grid of checkboxes, one per type, with | seeded |
| 74 | `notifications-configurer-un-canal-1.png` | Notifications page - Global events section, channel list, "New channel" button at the top right | Click + New channel at the top right. The Create channel dialog opens. | seeded |
| 75 | `notifications-configurer-un-canal-2.png` | A notification channel card, with its accent bar, channel icon, name and identifier, and its badges | Anatomy of a channel card | seeded |

## Observability

| # | File | What must be on screen | Where you are | Status |
|---|---|---|---|---|
| 76 | `observabilite-consulter-l-audit-trail-1.png` | Audit Trail tab - purpose banner at the top, 4 KPIs, filters, then the table | Just below, four key indicators (KPI) update according to the filters: Entries shown, Distinct  | seeded |
| 77 | `observabilite-consulter-l-audit-trail-2.png` | expanded row showing the Arguments / stdout / stderr sections | Click a row to expand its detail. Depending on what was captured, three sections can appear: | seeded |
| 78 | `observabilite-consulter-l-historique-des-taches-1.png` | Timeline tab: the KPI strip, the filter bar, then the events grouped by day | Choose the time window: 30 min / 1 h / 6 h / 24 h / 7 d. Default: 1 h. Events reload automatica | seeded |
| 79 | `observabilite-lire-le-digest-quotidien-1.png` | dashboard in operator mode, three cards in a grid, Decisions waiting on the left spanning two columns | The dashboard, for the present moment | seeded |
| 80 | `observabilite-surveiller-les-couts-llm-1.png` | LLM Costs tab, with the period selector, the four KPIs, the stacked bar chart and the backend legend | At the top, four key indicators (KPI) summarise the selected window: | seeded |

## Troubleshooting

| # | File | What must be on screen | Where you are | Status |
|---|---|---|---|---|
| 81 | `troubleshooting-la-dictee-vocale-ne-transcrit-rien-1.png` | Keyboard shortcut capture dialog, waiting for a key combination | Click the combination: a full-screen capture dialog opens. Press the new combination you want,  | seeded |
| 82 | `troubleshooting-le-fournisseur-d-ia-ne-repond-pas-1.png` | LLM backends page: a backend card in error, with its red icon and the Error label | Find the backend marked ✗ error in the list. Hover the status label: a native tooltip shows the | seeded |
| 83 | `troubleshooting-reinitialiser-apollia-factory-reset-1.png` | Settings Danger Zone page, red "Factory Reset" box with a clearly isolated button | In the sidebar, click Settings, then the Danger Zone section. | seeded |
| 84 | `troubleshooting-un-agent-est-bloque-1.png` | Inbox on the To do tab, with one approval card expanded to show what the agent is waiting for | The To do tab is selected by default. Filter on the Approvals chip to see only pending approval | **live** |
| 85 | `troubleshooting-une-action-est-refusee-1.png` | Inbox on the To do tab, with the recent history at the bottom showing a rejected line and its reason | Find the line with the ❌ Rejected icon matching the action. The reason entered at the time of t | **live** |

## Totals

| | Count |
|---|---|
| seeded, shoot as is | 63 |
| live, provoke first | 13 |
| blocked | 9 |
| **per locale** | **85** |
| **both locales** | **170** |

## Checking your work

```sh
# names: what is referenced, what is missing, what is unused
python3 scripts/automation/tools/publish_screenshots.py --locale en --from <dir>
python3 scripts/automation/tools/publish_screenshots.py --locale fr --from <dir>

# the site must still build, both locales
cd docs/site && npm run build      # expect two SUCCESS lines
```

`publish_screenshots.py` answers the question the naming makes silent: which
files a pass produced that no page uses, and which the pages want that the pass
did not produce. Run it before the build, because it names the file, while the
build only tells you a page is broken.

