# The fourteen to shoot by hand

Every other image in the documentation is taken by the automaton. These fourteen
are not, because each one needs something the runner cannot reach: a native OS
dialog, a real download, an OAuth round trip, or a model turn no prompt makes
repeatable.

This file is self-contained. Nothing here says "see note N4": if a value is
needed it is written on the line.

---

## Where every file goes, and under what name

```
docs/site/static/img/operator-help/<name>.png
```

One directory. No locale, no subfolder. `<name>` is the **File** column below,
verbatim, with no prefix and no suffix. Save or rename your capture to exactly
that, drop it in that directory, done.

**A misnamed file is invisible.** Nothing breaks, no build fails, the old image
simply stays and you believe you refreshed it. That is the only real trap in
this whole document.

Framing: shoot the useful region, not the whole window. The **Crop** column says
which. `dialog` means the modal alone, `panel` the working area without the
sidebar, `page` the full content area.

---

## Before you start

```sh
APOLLIA_SEED_PROJECT_ROOT="$HOME/Projects/atlas-migration" \
  bash scripts/automation/seed/load.sh
```

Read the line it prints: it names the overlay it used. Without the narrative
overlay you shoot the bare test fixture and the screens photograph thinner than
they should.

The seed does not write `onboarding.completed_at`, so the welcome modal opens on
every launch. For the four onboarding rows that modal **is** the subject. For
everything else, dismiss it first with Escape.

---

## Session A. Onboarding, three images

The modal opens once per profile reload, so do these three in one pass, in
order. If you miss one, reload the seed and start the pass again.

Fixed answers, use these and no others, the rest of the set was shot with them:

| Field | Value |
|---|---|
| First name | `Maya` |
| Role | `Head of operations` |
| Sector | `Consulting` |
| Main goal | `Migrate a client's documentation without rewriting every page by hand.` |
| Profile card | **Operator** |
| LLM model | **Qwen3 14B** (8.4 GB) |
| Speech model | **base** (142 MB) |

| File | Gesture | What must be on screen | Crop |
|---|---|---|---|
| `installation-configurer-votre-profil-4.png` | Select **Qwen3 14B** and **base**, click Download, count to ten. | One progress bar between 20 % and 60 % with a throughput figure, Whisper downloading beside it. **Capture around 40 %.** If you are past 60 %, cancel and start again. | dialog |
| `installation-configurer-votre-profil-5.png` | Let the downloads finish, Continue. The calibration step opens. | The 4 progress pips, the agent's first question, the answer field. Type `Maya` **but do not send**. | dialog |
| `installation-configurer-votre-profil-6.png` | Answer the four questions with the values above, reach the end of calibration. | The permission rule cards the agent proposes. | dialog |

The agent's wording changes on every run, so rows 5 and 6 will never be
byte-identical to the previous set. That is expected. Shoot twice and keep the
more legible one.

---

## Session B. Model download, one image

| File | Gesture | What must be on screen | Crop |
|---|---|---|---|
| `installation-telecharger-des-modeles-locaux-2.png` | Settings, Model Hub, click Download on the **Qwen3 14B** row. | That row with a progress bar and a Cancel button. **Capture around 40 %.** | panel |

Qwen3 14B is 8.4 GB, so the bar stays on screen long enough on any normal link.
Same rule as row 4: past 60 %, cancel and restart.

---

## Session C. Installing a package, five images

Two demo packages exist for this, in `~/apollia-demo-packages/`. They carry
metadata only and are never executed. Their README says which is which, and
records the rule that made the first version of them fail to install: `role`
accepts only `director`, `worker` or `assistant`, and nothing else.

- **`atlas-reporter`**: two agents, one pip dependency, no triggers.
- **`inbox-triage`**: one agent, two triggers, a webhook and a schedule.

| File | Gesture | What must be on screen | Crop |
|---|---|---|---|
| `agents-installer-un-agent-2.png` | Agents, new assistant, pick a package folder, point at **`atlas-reporter`**. | The Preview step: the Agents and Triggers sections, the green Valid badge. | dialog |
| `agents-installer-un-agent-2ter.png` | From that preview, click Install. | The dependency confirmation: the amber callout naming `atlas-reporter` and its one dependency, the list showing `markdown-it-py==3.0.0`, the venv note. | dialog |
| `agents-installer-un-agent-2bis.png` | Start over, point at **`inbox-triage`** instead. | The webhook trigger card with its Configure button. | dialog |
| `agents-installer-un-agent-3.png` | From the `inbox-triage` preview, click Configure. | The Configure step: the webhook card, the endpoint URL, the HMAC-SHA256 secret field. | dialog |
| `agents-installer-un-agent-4.png` | Click Install. | "Package installed!" with the agent and trigger counters, and the Close button. | dialog |

The 2ter row is why `atlas-reporter` declares a pip dependency: without one, the
installer skips that screen and the page documents a step nobody can photograph.
Confirming it builds a virtualenv and runs pip, so it needs the network and
takes a few seconds; `markdown-it-py` is small and pure Python, so the wait is
short. Shoot the screen **before** you confirm.

Rows 2bis and 3 need the package that carries a trigger, which is why the second
demo package exists at all.

---

## Session D. A seeded chat, one image, no model needed

| File | Gesture | What must be on screen | Crop |
|---|---|---|---|
| `chat-discuter-avec-votre-ia-3.png` | Chat, open the session **`Auditing the legacy documentation set`**, expand the reasoning card on the second message. | The reasoning strip expanded: 4 thinking captions interleaved with 4 tool cards, the second one in error, the table extractor died on a mismatched tag. | panel |

This one is fully deterministic: the session comes from the seed and needs no
model at all. It is manual only because conversation rows carry no `data-testid`
yet. Add one and this becomes the seventy-third automated capture.

---

## Session E. An MCP tool approval, one image

Needs `llama-server` on the `PATH` and the seed loaded: it connects two MCP
servers, `notes` and `filesystem`, eight tools between them.

| File | Gesture | What must be on screen | Crop |
|---|---|---|---|
| `integration-comprendre-les-permissions-mcp-1.png` | In a chat, ask: `List the seed notes using the notes server, then read the most recent one.` | The approval popup: the tool title, the exposed parameters, Allow once / Deny, and the scope note. | panel |

The seed leaves `mcp.tool_loading` at its default, `deferred`, which is the
setting a reader has. A model that answers it cannot call MCP tools used to be
the expected outcome there, and it was a real defect: deferred mode indexed the
tool names, registered an executor for each, accepted the call at the runtime
boundary, and never declared a single one to the model. It saw `tool_search`,
called it, got a name back, and had no declared tool to emit. Six tools sat
indexed and unreachable.

Deferred mode now advertises the whole index when it fits `mcp.tool_search_limit`
(20 by default), schemas included, and falls back to search-only above that.
Eight tools across two servers is well inside that bound, so the approval popup
is one prompt away. If the model still refuses, that is a finding, not a
shooting problem: say so rather than switching the seed to `eager`.

The stub gained `read_seed_note` for this row. It used to list three note ids
and expose no way to open one, so a model that did everything right still ended
the turn explaining it could not read them, and the second half of the prompt
was unreachable.

---

## Session F. Google Workspace, three images, deferred

**Skipped for this release.** The three images are placeholders that say so on
their face: a "screenshot pending" card naming the screen it will show and why
it is not there. They ship rather than leaving a broken image, and a reader who
lands on one learns something true instead of seeing nothing.

| File | What it will show, when it is shot |
|---|---|
| `integration-google-workspace-1.png` | The Connections page, Google Workspace card selected showing Not connected, connect action in the right panel. |
| `integration-google-workspace-2.png` | Google's own consent screen listing the requested permissions. |
| `integration-google-workspace-3.png` | Back in Apollia after consent: the Drive folder dialog, the `drive.file` scope explanation, the Folder path field. |

They need a real Google account **and** your own OAuth client, since Apollia
deliberately embeds none. Regenerate the placeholders, or replace them with real
captures, whenever that changes.

---

## When you are done

```sh
python3 scripts/check_screenshot_script.py
```

It compares the shooting rows against what the pages reference and names any
mismatch. Then, with the application closed:

```sh
bash scripts/automation/seed/unload.sh
```

`unload.sh` refuses while the app is running, because SQLite in WAL mode keeps
files open next to each database. Do not skip it: your real profile is sitting
aside until you do.

---

## Not in these fourteen: four pages with no image at all

Separate from the work above, and a decision rather than a task. Four operator
pages carry no illustration, and one of them matters more than the rest:

| Page | Why it deserves one |
|---|---|
| `integrations/connecter-microsoft-365.md` | The only connector that works with nothing to configure, and the contrast with Google is the point. Automatable. |
| `transversal/suivre-la-visite-guidee.md` | A whole subsystem, shipped and never shown. |
| `agents/choisir-un-palier-d-autonomie.md` | The choice happens on screen. Automatable. |
| `transversal/trouver-sa-version-et-ses-donnees.md` | Its five values just became copyable. |

These need a new row in `SCREENSHOTS.md` and a reference in the page, not just a
capture. Two of the four can then be shot by the automaton rather than by hand.
