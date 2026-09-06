# Native-tool coverage instrument

Eighteen native tools exist in this tree (`NATIVE_TOOL_NAMES`,
`crates/apollia-tools/src/tool_registry.rs:29`). This directory drives
seventeen of them, one task each, against a throwaway workspace, and reads the
result back with an oracle that does not ask a model anything.

| File | What it is |
|---|---|
| `native-tools-offline.toml` | 15 tasks, no network. The gate. |
| `native-tools-network.toml` | 2 tasks that reach a third-party service. Opt-in. |
| `agent/` | The `eval-tools-probe` fixture agent the tasks are addressed to. |
| `seed_workspace.py` | Builds the throwaway workspace and proves it usable. |
| `check_suite.py` | Static guard: the suites against the tree they claim to measure. |
| `verify_effects.py` | Post-run oracle: what the five state-changing tools actually did. |
| `simulate_run.py` | Applies a perfect or a near-miss run by hand, to prove the oracle. |

Every script answers with the same three codes: **0** measured and clean,
**1** a defect, **2** nothing measured. A 2 is never a pass, and none of the
scripts can print a green summary without having read something.

## Running it

```sh
python3 evals/tools/seed_workspace.py          # exits 0, builds /tmp/apollia-eval-tools
python3 evals/tools/check_suite.py             # exits 0, no daemon needed

HOME=/tmp/apollia-eval-tools/home ./target/debug/apollia-os start   # needs a model
./target/debug/apollia-os eval run evals/tools/native-tools-offline.toml \
    --out /tmp/apollia-eval-tools/offline.jsonl

python3 evals/tools/verify_effects.py /tmp/apollia-eval-tools/offline.jsonl
```

The workspace is a fixed literal path, `/tmp/apollia-eval-tools`, because the
harness's `file_exists` assertion takes a literal path and expands nothing. The
real `~/.apollia` is never opened: every command the scripts run is given
`HOME=/tmp/apollia-eval-tools/home`.

The workspace is **single use**. Five tasks change state, so their `runs` is 1
and a second pass measures what the first left behind. Re-seed between runs;
`check_suite.py` holds that rule mechanically.

## What the assertions rest on

Each prompt names the tool, the exact target, and the exact single line the
answer must be. The value in that line is a token that exists only inside the
seeded workspace (`seed_workspace.py`, `TOKENS`), so a model that answers
without calling the tool cannot produce it. `check_suite.py` fails when a
seeded token stops being named by a suite, which is how the fixture and the
assertions are kept from drifting apart.

`verify_effects.py` exists because the harness cannot read a file's content: it
asserts an exit code, that a path exists, and a regex over the answer. A run
that writes the wrong text into the right file passes the suite and fails the
oracle.

## The ledger, and the three holes in it

**`ask_user` is not driven, and cannot be on this path.** The task-mode runner
constructs its dispatcher with `pending_user_inputs: None`
(`crates/apollia-cli/src/commands/start/runner.rs:144` and `:251`), and the
dispatcher only registers `ask_user` when that field is `Some`
(`crates/apollia-tools/src/native_dispatcher.rs`). An agent driven by
`apollia-os eval run` therefore receives `UnknownTool`. Covering it needs the
chat surface, where `pending_user_inputs` is set, and a responder for the
question; the desktop gestural automaton is the instrument for that, not this
one. `scripts/automation/chat-llm.json` asks the model for the tool and
answers its card, and `scripts/capability_inventory.py` credits the tool from
that recipe, on the condition that the `-seeded-llama` run is green.

**`http_fetch` is driven only on its refusal path.** The same runner passes
`http_allowlist: None` (`runner.rs:143` and `:250`), and with no allowlist the
tool denies every request before opening a socket
(`crates/apollia-tools/src/tools/http_fetch.rs`). The task asserts that exact
refusal. A successful fetch is not measured here.

**`permission_rule_add` and `permission_rule_remove` declare themselves subject
to human-in-the-loop approval.** Whether an approval suspends the task in this
mode has not been measured. If it does, those two tasks reach the harness's
300-second timeout; `verify_effects.py` reads a timeout as *not measured*, not
as a defect, so the run cannot report a tool broken for a reason that is a gate.

## What this instrument does not measure

- It measures the pair (tool, model). A red row says the observable effect did
  not appear; it does not say which of the two failed. Read the answer text in
  the JSONL before blaming the tool.
- It does not exercise the desktop or chat paths. It drives one Python agent
  through `apollia-os eval run` and nothing else.
- The two network tasks depend on what a search engine ranks and what a public
  page says today. They are not a gate.
- It exercises one shape per tool: one argument set, one success path. Options
  the tools accept (offsets, `replace_all`, context lines, backends) are
  untouched.
- Nothing here has run against a live model. The suites parse under the
  product's own parser, the workspace builds, and the oracle answers 0, 1 and 2
  for the right reasons; that a model actually drives the seventeen tools to
  green remains unmeasured.

## Finding, outside this instrument's criterion

`memory import` inserts rows into `episodic_memories` but writes nothing into
`memory_fts` (`crates/apollia-memory/src/export.rs`, whose only mention of the
index is the `DELETE` on line 183), while `memory_search` reads the index and
joins the tables (`crates/apollia-memory/src/search.rs:198`). Memory imported
from an export is therefore invisible to search. `seed_workspace.py` writes the
index row itself, the way the episodic writer does, and then proves the entry
is findable through `apollia-os memory search` before declaring the workspace
usable. That workaround is a fixture concern; the export/import round trip is a
product concern and belongs in the queue.
