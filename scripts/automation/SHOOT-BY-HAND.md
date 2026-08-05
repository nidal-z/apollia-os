# The thirteen to shoot by hand

Every other image in the documentation is taken by the automaton. These thirteen
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

## Session C. Installing a package, four images

Two demo packages exist for this, in `~/apollia-demo-packages/`. They carry
metadata only and are never executed. Their README says which is which.

- **`atlas-reporter`**: two agents, one pip dependency, no triggers.
- **`inbox-triage`**: one agent, two triggers, a webhook and a schedule.

| File | Gesture | What must be on screen | Crop |
|---|---|---|---|
| `agents-installer-un-agent-2.png` | Agents, new assistant, pick a package folder, point at **`atlas-reporter`**. | The Preview step: the Agents and Triggers sections, the green Valid badge. | dialog |
| `agents-installer-un-agent-2bis.png` | Same flow, point at **`inbox-triage`** instead. | The webhook trigger card with its Configure button. | dialog |
| `agents-installer-un-agent-3.png` | From the `inbox-triage` preview, click Configure. | The Configure step: the webhook card, the endpoint URL, the HMAC-SHA256 secret field. | dialog |
| `agents-installer-un-agent-4.png` | Click Install. | "Package installed!" with the agent and trigger counters, and the Close button. | dialog |

Rows 2bis and 3 need the package that carries a trigger, which is why the second
demo package exists at all.

---

## Session D. A seeded chat, one image, no model needed

| File | Gesture | What must be on screen | Crop |
|---|---|---|---|
| `chat-discuter-avec-votre-ia-3.png` | Chat, open the session **`Auditing the legacy documentation set`**, expand the reasoning card on the second message. | The reasoning strip expanded: 4 thinking captions interleaved with 4 tool cards, the second one in error, the table extractor died on a mismatched tag. | panel |

This one is fully deterministic: the session comes from the seed and needs no
model at all. It is manual only because conversation rows carry no `data-testid`
yet. Add one and this becomes the fourteenth automated capture.

---

## Session E. An MCP tool approval, one image

Needs `llama-server` on the `PATH` and the seed loaded: it connects two MCP
servers, `notes` and `filesystem`.

| File | Gesture | What must be on screen | Crop |
|---|---|---|---|
| `integration-comprendre-les-permissions-mcp-1.png` | In a chat, ask the model to use a tool from the `notes` MCP server, for instance `List the notes available on the notes server and read the most recent one.` | The approval popup: the tool title, the exposed parameters, Allow once / Deny, and the scope note. | panel |

The model does not always reach for the MCP tool on the first try. Rephrase
until it does, then shoot. That unpredictability is exactly why the automaton
cannot take this one.

---

## Session F. Google Workspace, three images

Needs a real Google account **and** your own OAuth client, since Apollia
deliberately embeds none.

| File | Gesture | What must be on screen | Crop |
|---|---|---|---|
| `integration-google-workspace-1.png` | Connections, the Google Workspace card. | The card selected showing Not connected, and the right panel with the connect action. | page |
| `integration-google-workspace-2.png` | Click Connect, follow through to Google. | Google's own consent screen listing the requested permissions. | dialog |
| `integration-google-workspace-3.png` | Come back to Apollia after consenting. | The Drive folder dialog, the `drive.file` scope explanation, the Folder path field. | dialog |

If you have neither account nor client today, leave these three stale and say so
in the release notes. They are the only three in the whole set where that is the
honest answer.

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

## Not in these thirteen: four pages with no image at all

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
