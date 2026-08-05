# Screenshot shooting script

Eighty-five images, one set, shot in English, published into both locales.

Every row below is self-contained: the route, the gesture, the framing, the
state the seed puts on screen, and the exact values to type where a value is
needed. Read one row, shoot one image, move on. Nothing here should require a
decision.

If a row does need a decision, that is a defect in this file, not in your
judgement. Say so and it gets a value.

---

## Before you start

### 1. Load the seed

```sh
bash scripts/automation/seed/unload.sh 2>/dev/null   # only if one is loaded
APOLLIA_SEED_PROJECT_ROOT="$HOME/Projects/atlas-migration" \
  bash scripts/automation/seed/load.sh
```

`load.sh` moves `~/.apollia` aside rather than copying it, refuses to run if a
backup already exists, and picks up the narrative overlay from
`~/.apollia-seed-overlay` when there is one. It prints which overlay it used.
Read that line: without the overlay you get the test fixture, and half the
screens below show a thinner state than what they describe.

`APOLLIA_SEED_PROJECT_ROOT` is what the project pages, the permission cards and
the audit trail display. Left unset it is this repository checkout, which means
your own machine's path ends up on a published image. Point it at a directory
that exists (create an empty one if you have none) unless you have decided you
do not mind.

### 2. Check the seed before you shoot, not after

```sh
python3 ~/.apollia-seed-overlay/verify.py "$HOME"
```

It replays the SQL each screen issues, with the same time windows, and prints
what each one would show. The failure it exists to catch: a row that is present,
valid, and just outside the window the page filters on, so the page renders
empty and nothing anywhere reports an error. That is a whole shooting day.

The seed's timestamps are computed at build time. Coming back the next day,
reload it.

### 3. Set the application up

| Setting | Value | Why |
|---|---|---|
| Language | **English** | One image set, shared by both locales. See "One set, two locales" below. |
| Mode | **Operator**, unless a row says Builder | Operator is the default reading of every page. |
| Window | Maximised, then **do not resize** during the session | A crop that changes between two neighbouring images reads as two different products. |
| Appearance | **Light** | Whichever you pick, keep it for all 85. |

### 4. Dismiss the onboarding modal

The seed does not write `onboarding.completed_at`, so the first-launch modal
opens on every launch. It is the first thing you will see and it is not a bug.

- For rows 1 to 7, that modal **is** the subject: shoot those first, in order,
  then let the flow finish.
- For every other row, skip it (`Escape`, or the skip control at the bottom).

---

## One set, one directory, English

Every image lives in `docs/site/static/img/operator-help/`, with no locale
segment, and both locales reference the same file. The interface in the images
is English.

That is a deliberate narrowing, and it replaces the previous arrangement of
`en/` and `fr/` filled from a single capture set. The two directories stayed
byte identical until the day only one was refreshed. Then the English pages
served French captures for two weeks, and nothing caught it: a stale image is
not a broken link, the build says nothing, and the reader has no way to know.
One directory removes the failure mode instead of documenting it.

The cost is stated rather than hidden: a French reader sees an English
interface. It buys one capture set to maintain instead of two, one seed
narrative to keep coherent instead of two, and no possible drift between
halves of the site.

```sh
python3 scripts/automation/tools/publish_screenshots.py --apply
```

---

## Two ways to shoot

**72 of the 85 have an automaton label**, and 13 do not. The thirteen are not
an oversight, each is blocked by something the automaton cannot reach:

| What | How many | Why the automaton cannot take it |
|---|---|---|
| Installing an agent, steps 2 to 4 | 4 | A native OS file picker. The runner drives the webview by `data-testid` and has no handle on a system dialog. |
| Model download progress | 4 | A real multi-gigabyte download caught around 40 per cent. Neither fast nor repeatable, and the bar is where the value is. |
| Google Workspace, steps 1 to 3 | 3 | A real OAuth round trip against your own Google client, with a consent screen outside the application. |
| MCP permission prompt | 1 | Needs the model to call an MCP tool, and no prompt makes that deterministic. |
| Reasoning strip expanded | 1 | Needs one named seeded session opened by name; conversation rows carry no `data-testid` yet. Add one and this becomes automatable. |

The **How** column of every row says `auto` or `hand`, and it is kept in sync
with the scripts rather than by hand: `check_screenshot_script.py` compares the
rows against the pages, and the count above against the labels the two scripts
actually carry.



**72 of the 85 have an automaton label.** `scripts/automation/screenshots-en.json`
(61 labels) and `screenshots-en-llm.json` (5) drive the real application by
testid and capture under the right label:

```sh
lsof -ti :5173 :8899 | xargs kill -9 2>/dev/null
just desktop-screenshots scripts/automation/screenshots-en.json
```

Those runs frame the whole window, which is looser than the crops described
below. Use them as the baseline set, then re-shoot by hand any image whose
framing matters. The **How** column of each row says `auto` or `hand`.

The label count and the **How** column are not the same question, so they do not
match, and that is deliberate. A label says the runner can reach the screen; the
**How** column says whether the runner can reach it *in the state this row
wants*. Three rows (17, 82, 84) have a label and are still marked `hand`, because
the runner arrives at the right page and finds it empty: the state has to be
provoked first. One row (51) is marked `auto` and has no label, because the
runner captures it only as a side effect of a turn that produced a pending item.

**21 cannot be shot from the seed alone.** Twenty have a named reason in "The
twenty-one, and why" at the end; the twenty-first, row 25, is deterministic and
by hand only because its framing is tighter than the whole window. None of them
is a mystery to discover at the thirtieth image.

---

## Naming

Save each file under exactly the name in the **File** column. No prefix, no
suffix, no locale in the name.

A correctly named file replaces the old image with no edit anywhere. A misnamed
one is invisible: nothing breaks, the stale image simply stays. That silence is
why the names get checked at the end rather than trusted.

---

## Framing

Frame the useful region, not the whole window. What dates a screenshot is the
chrome around it. Keep one crop for a page's whole series so its images sit
together.

Three crops cover everything below, and each row names the one it wants:

- **panel**: the content area only, no sidebar, no title bar.
- **page**: the content area plus the left navigation, when the row is about
  where a thing lives.
- **dialog**: the modal or sheet plus a thin margin of the dimmed page behind.

---

## The predetermined values

Everything that has to be typed, chosen or asked, in one place. Do not
improvise: two neighbouring images built on two different answers tell two
different stories, and the reader notices.

| Where | Value |
|---|---|
| Onboarding, first name | `Maya` |
| Onboarding, role | `Head of operations` |
| Onboarding, sector | `Consulting` |
| Onboarding, main goal (free text) | `Migrate a client's documentation without rewriting every page by hand.` |
| Onboarding, profile card | **Operator** |
| Onboarding, LLM model | **Qwen3 14B** (8.4 GB) |
| Onboarding, speech model | **base** (142 MB) |
| Model Hub, model to download | **Qwen3 14B** (8.4 GB) |
| Model Hub, model to set as default | **Qwen3 14B** |
| Chat question, row 23 and 24 | `Summarise the three conversion failures from batch 3 and tell me which one blocks batch 4.` |
| Chat question, row 25 (reasoning) | `Read reports/atlas-audit.md and tell me how many pages need a manual pass.` |
| Chat question, row 60 (MCP approval) | `List the files under legacy/ using the filesystem connector.` |
| Approval rejection reason, row 50 | `That would remove batches 1 to 3 as well. Clear batch 4 by name instead.` |
| New project name, row 39 | `Client Digest 2027` |
| New project template, row 39 | **Developer project** |
| New automation sentence, rows 44 and 45 | `Every weekday at 8am, summarise what moved on Atlas Migration.` |
| New channel name, row 74 | `Ops desk` |
| Custom MCP name, rows 56 to 58 | `Notes server` |
| Custom MCP command (stdio), row 57 | `/usr/bin/python3` with argument `~/.apollia/mcp-stub-server.py` |
| Custom MCP URL (http), row 58 | `https://mcp.example.internal/v1` with header `Authorization: Bearer ****` |
| Global hotkey, rows 21 and 81 | `Cmd + Shift + D` |

**Anything not in this table is already on screen from the seed.** If a field is
empty and this table has no value for it, leave it empty: an empty field is part
of what the page documents.

---

## What the seed puts on screen

The whole set tells one week of one story, so that neighbouring pages agree.
This is what you should see; if you see something else, the seed did not load.

**Atlas Migration.** A two-person consultancy moving 340 pages of a client's
legacy documentation onto a new site. Week three: the inventory is done, batch 4
is converting, one batch failed on a malformed table and had to be redone.

| Where | What is there |
|---|---|
| Projects | `Atlas Migration` (active minutes ago) and `Client Digest` |
| Chat | 5 conversations, the top one `Auditing the legacy documentation set`, which carries 5 tool cards: 3 executed, 1 failed, 1 refused |
| Plan | 6 steps: 3 done (one of them replanned), 1 running, 1 pending, 1 failed |
| Automations | 2 active, 2 paused; 8 history rows across fired / skipped / error |
| Timeline (1 h) | ~42 events, every filter chip populated |
| Audit trail | 10 invocations, 2 failures, each expandable |
| LLM costs (7 d) | 20 calls, 3 backends, 7 populated days |
| Memory | 5 namespaces, project one included, all three types present in each |
| Agents | 4 agents + 1 package, 7 tasks covering every status |

---

## Installation and setup

Shoot this section first, in order: rows 1 to 7 walk the onboarding flow and it
only opens once per profile reload.

| # | File | Route and gesture | What must be on screen | Values | Crop | How |
|---|---|---|---|---|---|---|
| 1 | `installation-configurer-votre-profil-1.png` | Launch the app. The welcome modal opens on step 1. | Apollia logo, the subtitle "The sovereign runtime to run your AI agents locally", three reassurance points | none | dialog | auto |
| 2 | `installation-configurer-votre-profil-2.png` | Step 1 → Continue. | Two profile cards, Operator (sparkles) and Builder (code), 3 bullets each | Hover **Operator**, do not click yet | dialog | auto |
| 3 | `installation-configurer-votre-profil-3.png` | Pick Operator → Continue. | RAM / macOS / GPU banner, the four curated Qwen3 models with a Recommended badge, the Whisper list below | none, nothing selected yet | dialog | auto |
| 4 | `installation-configurer-votre-profil-4.png` | Select the model, click Download, wait ~10 s. | One progress bar between 20 % and 60 % with a throughput figure, Whisper downloading beside it | **Qwen3 14B** and **base**; capture when the bar is around 40 % | dialog | hand, see N1 |
| 5 | `installation-configurer-votre-profil-5.png` | Downloads finish → Continue. The calibration step opens. | 4 progress pips, the agent's first question, the answer field | Type `Maya` but do not send | dialog | hand, see N2 |
| 6 | `installation-configurer-votre-profil-6.png` | Answer all four questions, reach the end of calibration. | The permission rule cards the agent proposes | Answer with the four onboarding values above | dialog | hand, see N2 |
| 7 | `installation-configurer-votre-profil-7.png` | Finish onboarding. Settings → Profile. | Identity (Maya, Head of operations, Consulting), Goals, Agent supervision | none, the values are the ones just entered | panel | auto |
| 8 | `installation-connecter-un-modele-distant-1.png` | Settings → LLM backends → **+ Add LLM backend**. | The dialog, empty, Name and Provider visible | leave every field empty | dialog | auto |
| 9 | `installation-telecharger-des-modeles-locaux-1.png` | Settings → Model Hub. | The available-models list: Name, Size, Type, Status | none | panel | auto |
| 10 | `installation-telecharger-des-modeles-locaux-1bis.png` | Same page, scroll to Installed models. | The installed list with the **In use** badge on the active model | none, the seed installs two | panel | auto |
| 11 | `installation-telecharger-des-modeles-locaux-2.png` | Click Download on a model row. | The row with a progress bar and a Cancel button | **Qwen3 14B**; capture around 40 % | panel | hand, see N1 |

## Cross-cutting

| # | File | Route and gesture | What must be on screen | Values | Crop | How |
|---|---|---|---|---|---|---|
| 12 | `transversal-activer-la-compagnonne-ia-1.png` | Dashboard. Do not press anything yet. | The dashboard as it is, before the Companion opens. There is no sidebar button for it: `Cmd+/` and the palette are the only ways in | none | page | auto |
| 13 | `transversal-activer-la-compagnonne-ia-2.png` | Press `Cmd+/`. | The Apollia Help panel, its welcome message, the input area | do not type | panel | auto |
| 14 | `transversal-naviguer-au-clavier-command-palette-1.png` | From any screen, `Cmd+K`. | The palette, search field at the top, grouped suggestions below | leave the field empty | dialog | auto |
| 15 | `transversal-naviguer-au-clavier-command-palette-2.png` | Settings → Shortcuts. | Search bar, shortcuts grouped by category | leave the search empty | panel | auto |
| 16 | `transversal-utiliser-l-inbox-1.png` | Inbox → **To do** tab. | The counter chips in the tab bar, the filter chips below | none | panel | auto |
| 17 | `transversal-utiliser-l-inbox-2.png` | Provoke an `ask_user`, then expand it. | The expanded form: context callout, then the questions | see N4 | panel | auto |
| 18 | `transversal-utiliser-l-inbox-3.png` | Inbox → **Activity** tab. | Four filter chips and the event cards | none | panel | auto |
| 19 | `transversal-utiliser-l-inbox-4.png` | Inbox → **Notifications sent** tab. | Channel filter and the four-column delivery table, 8 rows | none | panel | auto |

## Chat

| # | File | Route and gesture | What must be on screen | Values | Crop | How |
|---|---|---|---|---|---|---|
| 20 | `chat-activer-la-dictee-vocale-1.png` | Settings → Speech-to-Text. | The Whisper model status at the top of the section | none | panel | auto |
| 21 | `chat-activer-la-dictee-vocale-2.png` | Same page, click the Global hotkey field. | The capture window, "Press your hotkey combination", then the captured combination | press `Cmd + Shift + D` | dialog | auto |
| 22 | `chat-discuter-avec-votre-ia-1.png` | Chat, **New conversation**. | Conversation list on the left showing the 5 seeded threads, empty centre, input at the bottom | do not type | page | auto |
| 23 | `chat-discuter-avec-votre-ia-2.png` | Send the question, capture mid-stream. | A user message and an answer streaming in, markdown rendered | the row-23 question above | panel | auto |
| 24 | `chat-discuter-avec-votre-ia-2bis.png` | Same turn, a few seconds later. | The same answer further along | same question, do not resend | panel | auto |
| 25 | `chat-discuter-avec-votre-ia-3.png` | Open `Auditing the legacy documentation set`, expand the reasoning card on the second message. | The reasoning strip expanded: 4 thinking captions interleaved with 4 tool cards, the second in error (the table extractor died on a mismatched tag) | none, this one is **seeded** and needs no model | panel | hand |

Row 25 no longer needs a live turn: the seeded conversation carries the
reasoning trace and its tool calls, so it renders from the database. It is
marked `hand` only because the automaton frames the whole window.

## Agents

| # | File | Route and gesture | What must be on screen | Values | Crop | How |
|---|---|---|---|---|---|---|
| 26 | `agents-consulter-les-logs-d-un-agent-1.png` | My Assistants → select `apollia-guide` → **Logs**. | The Agent Logs panel: task counter, search, status filters, sorting. 7 tasks, every status | none | panel | auto |
| 27 | `agents-demarrer-un-agent-1.png` | My Assistants. | The left column with both sections, My assistants and My packages | none | page | auto |
| 28 | `agents-demarrer-un-agent-2.png` | Click the `seed-office-pack` row. | Package detail: Information, Agents with director/worker roles, Triggers | none | panel | auto |
| 29 | `agents-demarrer-un-agent-2bis.png` | Same panel, scrolled down. | The rest of the same panel | none | panel | auto |
| 30 | `agents-installer-un-agent-1.png` | My Assistants, one agent selected. | List left, detail right, **New assistant** top right | select `apollia-guide` | page | auto |
| 31 | `agents-installer-un-agent-2.png` | New assistant → pick a package folder. | Preview step: Agents and Triggers sections, green Valid badge | see N5 | dialog | hand, see N5 |
| 32 | `agents-installer-un-agent-2bis.png` | Same preview, a package with a webhook trigger. | The webhook trigger card with its Configure button | see N5 | dialog | hand, see N5 |
| 33 | `agents-installer-un-agent-3.png` | Preview → Configure. | Configure step: webhook card, endpoint URL, HMAC-SHA256 secret field | see N5 | dialog | hand, see N5 |
| 34 | `agents-installer-un-agent-4.png` | Configure → Install. | "Package installed!" with the agent and trigger counters, Close | see N5 | dialog | hand, see N5 |

## Projects

| # | File | Route and gesture | What must be on screen | Values | Crop | How |
|---|---|---|---|---|---|---|
| 35 | `projets-activer-les-context-providers-1.png` | Projects → `Atlas Migration` → Context. | The provider list with its ON/OFF toggles. Three providers | none | panel | auto |
| 36 | `projets-activer-les-context-providers-2.png` | Same section, no click needed. | Git status ON (green), Directory tree OFF (grey) | none, the seed sets them that way | panel | auto |
| 37 | `projets-activer-les-context-providers-3.png` | Click **Preview context** (Workspace Snapshot). | The provider preview with real git diff and file tree content | none; needs `APOLLIA_SEED_PROJECT_ROOT` to be a real git checkout | panel | auto |
| 38 | `projets-creer-un-projet-1.png` | Projects. | The list, **+ New Project** highlighted top right | none | page | auto |
| 39 | `projets-creer-un-projet-2.png` | Click + New Project. | The modal: Name, Root folder, Template | name `Client Digest 2027`, template **Developer project**, leave the folder as offered | dialog | auto |
| 40 | `projets-creer-un-projet-3.png` | Cancel, then click the `Atlas Migration` card. | The detail sheet: Description, Agents (2), Context Providers (3), Documents (2), path | none | dialog | auto |
| 41 | `projets-lier-un-projet-a-un-chat-1.png` | Same sheet. | **+ New Chat** highlighted | none | dialog | auto |
| 42 | `projets-lier-un-projet-a-un-chat-2.png` | Chat → open any conversation → header menu (three dots). | The menu open on **Link to a project** | open `Weekly check-in` | panel | auto |
| 43 | `projets-lier-un-projet-a-un-chat-3.png` | Back to the project sheet, Chats section. | The linked chats with their titles and dates | none, the seed links two, `Weekly check-in` and `Auditing the legacy documentation set` | dialog | auto |

## Automations

| # | File | Route and gesture | What must be on screen | Values | Crop | How |
|---|---|---|---|---|---|---|
| 44 | `automatisations-programmer-un-trigger-1.png` | Automations → **Create an automation**. | The 4-step stepper on step 1 (Describe) | type the row-44 sentence, do not continue yet | dialog | auto |
| 45 | `automatisations-programmer-un-trigger-2.png` | Continue to the Schedule step. | The human-readable schedule box, the next-run line, the refinement fields | same sentence; the box should read "Every weekday at 08:00" | dialog | auto |
| 46 | `automatisations-suivre-l-historique-d-un-trigger-1.png` | Automations, hover the `seed-trigger-daily-digest` row, open ⋯. | The three-dot menu open on **View history** | none | panel | auto |
| 47 | `automatisations-suivre-l-historique-d-un-trigger-2.png` | Click View history. | The sliding panel: status filter chips, then the run cards. 4 runs, 2 fired and 2 skipped | none | panel | auto |

## Control

| # | File | Route and gesture | What must be on screen | Values | Crop | How |
|---|---|---|---|---|---|---|
| 48 | `controle-approuver-ou-refuser-une-action-1.png` | Provoke a tool approval in chat (see N4). | The inline approval card: orange shield, the command preview, the buttons | see N4 | panel | auto |
| 49 | `controle-approuver-ou-refuser-une-action-2.png` | Inbox → To do, expand the pending approval. | The expanded approval card with its risk badge | see N4 | panel | auto |
| 50 | `controle-approuver-ou-refuser-une-action-3.png` | Click Refuse. | The Reject action dialog: textarea, character counter, Cancel / Confirm | type the row-50 reason (98 characters, so the counter reads `98 / 500`) | dialog | auto |
| 51 | `controle-approuver-ou-refuser-une-action-4.png` | Inbox → To do, Recent history block. | 4 rows with different icons, the rejection showing its reason in red | none, the seed writes the history | panel | auto once a pending item exists, see N4 |
| 52 | `controle-configurer-les-permissions-de-fichiers-1.png` | Settings → Permissions. | The permission rule cards with their scope badges. 4 rules covering all three badges: Everywhere (2), This project, Chat / agent | none | panel | auto |
| 53 | `controle-configurer-les-permissions-de-fichiers-1bis.png` | Click **Revoke all**. | The dialog: scope selector, affected-rule count, Revoke | pick scope **project**; do **not** confirm | dialog | auto |
| 54 | `controle-configurer-les-permissions-de-fichiers-2.png` | Cancel, then hover a rule card and click Revoke. | The card mid-revoke and the confirmation toast | revoke the `bash_executor` rule, so the toast names `bash_executor` | panel | auto |
| 55 | `controle-configurer-les-permissions-de-fichiers-3.png` | Same page, Active sessions section. | The session entries with their orange Session badge and Revoke button. 5 authorizations across 3 sessions | none | panel | auto |

Row 54 mutates the seed. Shoot it after 52, 53 and 55, or reload the seed.

## Integrations

| # | File | Route and gesture | What must be on screen | Values | Crop | How |
|---|---|---|---|---|---|---|
| 56 | `integration-cabler-son-propre-serveur-mcp-1.png` | Connections → **+ Add custom** → Custom tab. | The blank form | leave it empty | dialog | auto |
| 57 | `integration-cabler-son-propre-serveur-mcp-2.png` | Same form, transport **stdio**. | Command and arguments filled in | name `Notes server`, command and argument from the values table | dialog | auto |
| 58 | `integration-cabler-son-propre-serveur-mcp-3.png` | Same form, transport **streamable-http**. | URL and authentication headers | URL and header from the values table | dialog | auto |
| 59 | `integration-comprendre-la-portee-d-une-integration-1.png` | My Assistants → `apollia-guide` → Tools tab. | Required and optional tools with their approval badges | none | panel | auto |
| 60 | `integration-comprendre-les-permissions-mcp-1.png` | Ask the row-60 question in a chat with the MCP server connected. | The approval popup: tool title, exposed parameters, Allow once / Deny, the scope note | the row-60 question | panel | hand, see N3 |
| 61 | `integration-comprendre-les-permissions-mcp-2.png` | Settings → Permissions. | The rules stacked, a Revoke on each row | none | panel | auto |
| 62 | `integration-google-workspace-1.png` | Connections → Google Workspace card. | The card selected (Not connected), the right panel with the connect action | see N6 | page | hand, see N6 |
| 63 | `integration-google-workspace-2.png` | Click Connect, follow to the Google consent screen. | Google's consent screen listing the requested permissions | see N6 | dialog | hand, see N6 |
| 64 | `integration-google-workspace-3.png` | Return to Apollia after consent. | The Drive folder dialog, the drive.file scope explanation, the Folder path field | see N6 | dialog | hand, see N6 |
| 65 | `integration-connecter-un-serveur-mcp-1.png` | Connections → **+ Discover**. | The catalogue on its Discover tab, grid of entries | none; needs network, the grid is a live fetch | dialog | auto |
| 66 | `integration-tester-une-connexion-mcp-1.png` | Connections, select `filesystem` in the sidebar. | The server selected, its detail page on the right | none | page | auto |
| 67 | `integration-tester-une-connexion-mcp-2.png` | Same page, actions area. | The Test button next to the server name | do not click | panel | auto |
| 68 | `integration-overview-1.png` | Connections. | The sidebar: native connectors (Google Workspace, Microsoft 365) then the MCP servers | none | page | auto |

## Memory

| # | File | Route and gesture | What must be on screen | Values | Crop | How |
|---|---|---|---|---|---|---|
| 69 | `memoire-consulter-et-nettoyer-la-memoire-1.png` | Memory. | Namespace sidebar left, type filters and search centre, entries right. 5 namespaces, the **project** chip populated | select `default · seed-project-alpha` | page | auto |
| 70 | `memoire-consulter-et-nettoyer-la-memoire-2.png` | Click the `atlas.pattern.nested_tables` entry. | The detail panel: full value, metadata, Copy and Delete | that entry, it is the longest and shows the JSON formatting | panel | auto |
| 71 | `memoire-gerer-mon-profil-1.png` | Settings → Profile. | The stacked sections from Identity down to the danger zone | none | panel | auto |
| 72 | `memoire-gerer-mon-profil-1bis.png` | Danger zone → **Reset profile**. | The confirmation modal in front of the danger zone | do **not** confirm | dialog | auto |

## Notifications

| # | File | Route and gesture | What must be on screen | Values | Crop | How |
|---|---|---|---|---|---|---|
| 73 | `notifications-choisir-les-evenements-notifies-1.png` | Notifications, Global events section. | The grid of 7 checkboxes with label, description, technical identifier | none, the seed ticks 3 | panel | auto |
| 74 | `notifications-configurer-un-canal-1.png` | Same page, top. | Global events, the channel list, **+ New channel** top right | none | panel | auto |
| 75 | `notifications-configurer-un-canal-2.png` | Hover the `Desktop notifications` card. | One channel card: accent bar, icon, name and identifier, badges | none | panel | auto |

## Observability

| # | File | Route and gesture | What must be on screen | Values | Crop | How |
|---|---|---|---|---|---|---|
| 76 | `observabilite-consulter-l-audit-trail-1.png` | Observability → **Audit Trail**. | Purpose banner, 4 KPIs, filters, then the table. 10 rows, 2 agents, 2 failures | none | panel | auto |
| 77 | `observabilite-consulter-l-audit-trail-2.png` | Expand the failed `bash_executor` row. | Arguments, stdout and stderr sections, all three populated | expand the row whose stderr mentions page 187 | panel | auto |
| 78 | `observabilite-consulter-l-historique-des-taches-1.png` | Observability → **Timeline**. | KPI strip, filter bar, events grouped by day. ~42 events in the default 1 h window, every chip populated | keep the default **1 h** window | panel | auto |
| 79 | `observabilite-lire-le-digest-quotidien-1.png` | Dashboard, operator mode. | Three cards in a grid, Decisions waiting spanning two columns on the left. Both projects active | none | page | auto |
| 80 | `observabilite-surveiller-les-couts-llm-1.png` | Observability → **LLM Costs**. | Period selector, 4 KPIs, the stacked bars, the backend legend. 3 backends, 7 populated days | keep the default **7 d** period | panel | auto |

## Troubleshooting

| # | File | Route and gesture | What must be on screen | Values | Crop | How |
|---|---|---|---|---|---|---|
| 81 | `troubleshooting-la-dictee-vocale-ne-transcrit-rien-1.png` | Settings → Speech-to-Text → click the shortcut. | The full-screen capture dialog waiting for a combination | do not press anything, shoot the waiting state | dialog | auto |
| 82 | `troubleshooting-le-fournisseur-d-ia-ne-repond-pas-1.png` | Settings → LLM backends. | A backend card in error, red icon, Error label; hover it for the native tooltip | the seed marks one backend disabled; see N7 | panel | auto |
| 83 | `troubleshooting-reinitialiser-apollia-factory-reset-1.png` | Settings → Danger Zone. | The red Factory Reset box with its isolated button | do **not** click | panel | auto |
| 84 | `troubleshooting-un-agent-est-bloque-1.png` | Inbox → To do, filter on **Approvals**, expand one. | The approval card expanded showing what the agent waits for | see N4 | panel | auto |
| 85 | `troubleshooting-une-action-est-refusee-1.png` | Inbox → To do, Recent history at the bottom. | The ❌ Rejected line with its reason | none, the seed writes it | panel | auto once a pending item exists, see N4 |

---

## The twenty-one, and why

Twenty of these cannot be made deterministic, and the reason is named. The
twenty-first, row 25, is fully deterministic and listed here only so the count
in "Two ways to shoot" reconciles: it is shot by hand because its crop is
tighter than the whole window, not because anything about it varies.

**N1. A real download in progress** (rows 4, 11)
The useful instant lasts a few seconds and depends on your connection. Mitigated
as far as it can be: **Qwen3 14B** is 8.4 GB, so the bar stays visible for a
while on any normal link. Start the download, count to ten, shoot. If the bar is
past 60 %, cancel and start again.

**N2. A live model turn during onboarding** (rows 5, 6)
Calibration is a conversation with the onboarding agent, and its wording changes
every run. The four answers are fixed above so the *questions* are the same; the
agent's phrasing will not be. Accept it, or shoot the same row twice and keep
the more legible one.

**N3. A live model turn in chat** (rows 23, 24, 60)
Same reason. The questions are fixed above, the generated text is not. Row 25,
which used to be here, is now seeded and no longer needs a model at all.

**N4. The inbox pending list lives in memory** (rows 17, 48, 49, 50, 51, 84, 85)
`list_pending_approvals` reads an in-memory set, not a database, so no seed can
reach it. You have to provoke an approval during the session:

1. Open the `Auditing the legacy documentation set` conversation.
2. Send: `Clear the out/ directory before the next batch.`
3. The agent proposes `bash_executor` and the approval appears inline (row 48)
   and in the Inbox (rows 17, 49, 84).
4. Refuse it with the row-50 reason (rows 50, 85).
5. Rows 51 and 85 read the persisted history, which the seed already fills, but
   the block only renders while a pending item exists. Shoot them before you
   resolve the last approval.

**N4b. Session authorizations are not in the same family** (row 55)
The Active sessions block reads the running `ChatSessionManager`, which is what
makes the Inbox unseedable, so it looks like the same trap. It is not.
`restore_sessions` hydrates each active session's authorized tools from
`chat_tool_authorizations` at boot (`chat/manager/user_input.rs`, called from
`chat/manager/handle.rs`), so the seeded rows are on screen from the first
launch. Row 55 is `auto` and needs no gesture. Verify it before shooting: the
`Settings > Permissions, Active sessions` block of `verify.py` prints the exact
rows the panel will list.

**N5. The native folder picker** (rows 31, 32, 33, 34)
Installing a package opens the macOS folder dialog, which lives outside the web
view and cannot be driven. Do it by hand, once, and shoot the four steps of the
flow it opens. Use `scripts/automation/seed/files/agents/packages/seed-office-pack`
as the package: it is a valid bundle and it is the one the rest of the set shows.
Row 32 needs a package with a webhook trigger, which `seed-office-pack` does not
have. That one image cannot be shot from the seed at all: either add a webhook
trigger to a copy of the package, or leave the stale image in place.

**N6. A real Google account** (rows 62, 63, 64)
Row 63 is Google's own consent screen, outside the application entirely. All
three need a real account and a real OAuth client. If you have neither, leave
the three stale images in place: they are the only rows in the set where that is
the honest answer.

**N7. A backend in error needs the backend to fail** (row 82)
The seed disables `openai-gpt4o-mini` but disabled is not error: the card shows
grey, not red. To get the red state, configure a backend pointing at an
unreachable host and let the health check fail. Name it `Remote GPU` and point
it at `http://127.0.0.1:9/v1`, which refuses instantly.

---

## Totals

Counted from the **How** column, not from the label files:

| | Count | Rows |
|---|---|---|
| `auto`, the runner reaches the state on its own | 62 | everything not listed below |
| `auto` once a pending item exists | 2 | 51, 85 |
| `hand`, deterministic, shot by hand for the crop | 1 | 25 |
| `hand`, non-deterministic (N1 to N7) | 20 | 4, 5, 6, 11, 17, 23, 24, 31, 32, 33, 34, 48, 49, 50, 60, 62, 63, 64, 82, 84 |
| **Images** | **85** | |
| **Published directories** | **2, from one set** | |

Separately, 66 rows carry an automaton capture label (61 in
`screenshots-en.json`, 5 in `screenshots-en-llm.json`). That number answers a
different question, as "Two ways to shoot" explains, and is not expected to
equal 64.

---

## Checking your work

```sh
# names: what is referenced, what is missing, what is unused
python3 scripts/automation/tools/publish_screenshots.py --locale both --from <dir>

# then, once the report is clean
python3 scripts/automation/tools/publish_screenshots.py --from <dir> --apply

# the site must still build, both locales
cd docs/site && npm run build      # expect two SUCCESS lines
```

`publish_screenshots.py` answers the question the naming makes silent: which
files a pass produced that no page uses, and which the pages want that the pass
did not produce. Run it before the build, because it names the file, while the
build only tells you a page is broken.

When you are done:

```sh
bash scripts/automation/seed/unload.sh
```

It refuses while the application is running, because SQLite in WAL mode keeps
files open next to each database. Quit the app first.

---

## Keeping this file true

Each row is independent on purpose. A page added by another change gets one new
row; a screen that is restyled gets its "What must be on screen" cell edited and
nothing else. Nothing below the tables needs rewriting for either.

One thing is checked for you. The File column and the image names the pages
reference must be the same set, and neither side reports a mismatch on its own:

```sh
python3 scripts/check_screenshot_script.py
```

CI runs it, along with `--self-test`, which replays each mismatch the check
claims to catch so a check that has gone blind fails instead of passing.

Two things do go stale silently and are worth a glance before a shooting day:

- the row counts in "What the seed puts on screen", if the overlay changes.
  `verify.py` prints the real ones, and it is the only honest source: the counts
  written here were wrong on five rows the first time they were transcribed by
  hand.
- the automated / by hand split, if a script gains or loses a capture label.
  `python3 -c` over `screenshots-en.json` counts them; today it is 61 + 5.
