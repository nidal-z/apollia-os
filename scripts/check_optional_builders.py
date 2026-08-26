#!/usr/bin/env python3
"""Find `with_*` builders that install a capability nobody installs.

Three times now this repository has produced the same shape: a `pub fn with_x`
that sets an optional field to `Some`, a default path that leaves it `None`, and
a documented capability that therefore never runs.

  - the opt-in permission engine of `apollia-tools`, documented on five pages
    and wired in no shipped binary. It has since been removed, which is what a
    builder of this class earns.
  - `with_judge`: every `llm_judge` assertion silently counted as passed,
    because the CLI builds the runner with `new`.
  - the same shape elsewhere, which is what this script is for.

The failure mode is specific and worth naming. The builder compiles. Its tests
pass, because tests are exactly the callers that use it. The capability reads as
present in the type, in the documentation and in review. Only production never
asks for it.

So the check is: for every `pub fn with_*(...) -> Self` that assigns `Some(...)`
to a field, is there a caller outside test code AND outside the defining crate?
A caller inside the crate is usually another builder or a test helper; a caller
outside it is a binary that actually chose the capability.

A hit is not automatically a defect. A builder can legitimately exist for an
embedder, or be superseded by a narrower one that production does call. It is a
defect when the documentation says the capability is active.

So the gate is not "zero hits". It is a baseline: every currently-uncalled
builder is listed below with a verdict someone reached by reading the call path.
A new one that nobody has judged fails the build, and a baseline entry that
acquires a caller also fails, because a stale exemption is how a list like this
rots into decoration.

Usage:
    python3 scripts/check_optional_builders.py            # report
    python3 scripts/check_optional_builders.py --strict   # CI gate
"""

import argparse
import re
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]

# `pub fn with_something(...) -> Self` or `-> Result<Self, _>`.
BUILDER = re.compile(r"^\s*pub (?:const )?fn (with_\w+)\s*(?:<[^>]*>)?\s*\(", re.M)
# The body installs an `Option`, which is the shape that matters: a builder
# setting a plain value cannot leave a capability silently absent. Three forms,
# and the last two were added because the first missed four real cases.
#
# A literal `Some(` is the obvious one. `self.field = <expr>.ok()` and
# `self.field = Option::from(...)` install exactly the same thing without ever
# writing `Some`, and the four builders of `crates/apollia-aip/src/context.rs`
# are all of that shape: `Python::with_gil(|py| Py::new(py, x).ok())`.
#
# The anchor on `self.<field> =` is not decoration. Without it the pattern also
# matched an unrelated `.ok()` sitting inside the 4000-character window that
# `body_after` reads, which turned `with_ssrf_guard@apollia-tools` into a fifth
# hit; that builder sets a `bool`
# (`crates/apollia-tools/src/tools/http_fetch.rs:142-145`). Anchored: four hits,
# all real.
SETS_SOME = re.compile(
    r"=\s*Some\(|:\s*Some\(|"
    r"^\s*self\.\w+\s*=\s*.*\.ok\(\)|"
    r"^\s*self\.\w+\s*=\s*Option::from\(",
    re.M,
)


# Every builder with no production caller, and why it is tolerated. Reviewed
# against the call path on 2026-07-31; `defect` entries are open, not accepted.
BASELINE: dict[str, str] = {
    "with_judge@apollia-eval": "defect, contained: the CLI builds the runner with "
    "`new`, so no judge is ever installed. The assertion now fails explicitly "
    "instead of counting as passed, so the capability is absent but no longer "
    "silently green.",
    "with_journal@apollia-tools:file_write": "held for v0.2: nothing writes the journal, so "
    "there is nothing to revert. The `rollback` command and every page that "
    "promised an undo were removed rather than left to report an empty result, "
    "which an operator cannot tell from a clean session. Wiring this is what "
    "makes the capability real; until then no documentation may imply it.",
    "with_journal@apollia-tools:file_edit": "held for v0.2: same chain as file_write.",
    "with_tool_offload@apollia-oria": "closed by documentation: no offload store "
    "is ever installed, so compaction is in-memory only. "
    "`architecture/05-building-blocks.md` now says so rather than listing disk "
    "offload as a capability. Wiring it is a v0.2 question.",
    "with_db_path@apollia-oria": "closed by documentation, and deliberately not "
    "wired: nothing sets it, so the engine runs on `:memory:`. Unlike the plan "
    "cache and the rollback journal, no page ever claimed a plan run survives a "
    "restart, so this is unstated behaviour rather than a dead capability. "
    "`the-plan-model.md` now states it. Turning on SQLite persistence here would "
    "change durability with no functional validation behind it, which is the "
    "argument that decides an unwired builder either way.",
    "with_force_plan_gate@apollia-oria": "superseded: production calls "
    "`with_plan_gate_override`, which carries the same decision with a tri-state. "
    "Delete rather than wire.",
    "with_llm_router_and_reasoner@apollia-oria": "superseded: `wire_engine_with_llm` "
    "calls `with_llm_router` then `with_reasoner` separately. Delete rather than wire.",
    "with_meta_orchestrator@apollia-oria": "superseded: the meta orchestrator is "
    "reached through the runtime's builtin agent, not through the reasoner. The "
    "capability is alive; this entry point is not.",
    "with_hitl@apollia-mcp": "dead second gate: approval is enforced by the ORIA "
    "actor loop for orchestrated runs and by the chat dispatcher for free chat, "
    "both of which are wired. Redundant defence nobody installs, not a hole.",
    "with_session_filter@apollia-tools": "defect, open, and the chain has no middle: "
    "`apollia run --allowed-tools` / `--disallowed-tools` build a `session_config` "
    "fragment (`commands/run.rs:868`) and push it into the AIP input payload. Nothing "
    "on the runtime side reads that key, so `SessionToolPolicy::into_tool_filter` is "
    "never called and `dispatch()` never evaluates a filter. Both ends exist, the wire "
    "between them does not, and the operator gets no error.",
    "with_file_path_extractor@apollia-tools": "defect, open: nothing installs the "
    "extractor, so a bash command that touches files reports none of them upward. Its "
    "only caller is a unit test of the extraction itself.",
    "with_timestamp_cache@apollia-tools": "defect, open: `FileRead` never receives a "
    "cache, so `RuntimeEvent::FileModifiedSinceRead` has no producer and an agent is "
    "never told a file changed under it. The builder also sits behind `memory-search`, "
    "which only `apollia-runtime` enables.",
    "with_thinking@apollia-core": "unwired by decision: no producer fills the "
    "`thinking` field of a HITL request, so the approval surfaces render the reason "
    "without it. Filling it means deciding what of a model's reasoning may be shown to "
    "a user, which is a product question rather than a repair.",
    "with_details@apollia-runner": "dead: an IPC `ErrorBody` is built by `new` at "
    "every site and no caller ever attaches a details payload. Delete rather than "
    "wire.",
    "with_telemetry@apollia-runtime": "dead, and so is what it installs: nothing "
    "calls the builder, and `invoke_with_logging`, the only method that reads the "
    "telemetry handle, has no caller either. Delete rather than wire.",
    "with_wall_clock_secs@apollia-aip:bridge": "unwired, and the default is the whole "
    "policy: `call_run` falls back to `DEFAULT_WALL_CLOCK_SECS` (300 s) on every "
    "binary, so the per-agent budget this builder exists for is not configurable. "
    "Wiring it means adding a manifest field, not calling a setter.",
    "with_wall_clock_secs@apollia-aip:context": "the context mirror of the bridge "
    "budget above: `ctx.budget.wall_clock_remaining` stays `None` because nothing sets "
    "it, and it stays so until the bridge one is wired. One decision, two call sites.",
    # The four below became visible when SETS_SOME learned the `.ok()` form.
    # Each verdict was reached by reading the call path, on 2026-08-22.
    "with_notify@apollia-aip": "defect, open: nothing calls it, so the getter "
    "hands back `None` on every binary and `ctx.notify.publish` is unreachable. "
    "The reference page now says the service may be unattached; wiring a real "
    "notification channel is product work, not documentation.",
    "with_stt@apollia-aip": "defect, open: same shape as with_notify. Nothing "
    "calls it, so `ctx.stt` is always `None`. The STT backend itself is alive "
    "and reachable through the HTTP routes; only the agent-facing surface is "
    "unattached.",
    "with_workspace_snapshot@apollia-aip": "defect, open, and the second fill "
    "path is dead too: `bridge.rs` can populate `ctx.workspace` from a snapshot, "
    "but only behind `if let Some(ref cwd) = self.cwd`, and `AIPBridge::with_cwd` "
    "has no caller either. The four production `AIPBridge::new` sites do not "
    "chain it, so `ctx.workspace` is `None` on both binaries, always.",
    "with_empty_workspace@apollia-aip": "defect, open: the empty counterpart of "
    "with_workspace_snapshot, called from one unit test only. Wiring it would "
    "give an agent an empty snapshot rather than `None`, which is a contract "
    "decision, not a repair.",
}


def strip_tests(text: str) -> str:
    """Cut the trailing `#[cfg(test)] mod` block, and nothing else.

    Cutting at the first bare `#[cfg(test)]` is the tempting one-liner and it is
    wrong: the attribute also sits on a lone const or helper in the middle of a
    file, and truncating there hides every production caller below it. That bias
    runs one way only, toward reporting a wired capability as uncalled, which is
    exactly the alarm this script exists to make trustworthy.
    """
    match = re.search(r"^#\[cfg\(test\)\]\s*\nmod ", text, re.M)
    return text if match is None else text[: match.start()]


def is_test_path(path: Path) -> bool:
    """Integration tests and benches are callers, but not production ones."""
    parts = set(path.parts)
    return bool(parts & {"tests", "benches", "examples"}) or path.name == "tests.rs"


def crate_of(path: Path) -> str:
    parts = path.parts
    return parts[parts.index("crates") + 1] if "crates" in parts else "?"


def body_after(text: str, start: int) -> str:
    """Roughly the function body: to the next `pub fn` or 60 lines, whichever first."""
    nxt = text.find("\n    pub fn ", start + 1)
    end = nxt if nxt != -1 else len(text)
    return text[start : min(end, start + 4000)]


def production_callers(name: str, sources: dict[Path, str]) -> list[str]:
    """Every `repo/relative/path.rs:line` that calls `name` outside test code.

    Exposed rather than inlined so `check_selftest.py` can pin its behaviour on a
    fixture. The comment rule below is the one that actually bit, twice.
    """
    callers: list[str] = []
    for path, text in sources.items():
        if is_test_path(path):
            continue
        # The defining file is searched too, not skipped. `engine.rs` defines
        # `with_memory_manager` on one type and calls it on another a thousand
        # lines apart; skipping the file reported a wired capability as dead.
        for n, line in enumerate(strip_tests(text).splitlines(), 1):
            # A doc-comment naming the builder is not a caller. Counting one
            # reports a dead capability as wired, and this file exists because
            # that direction of error is the expensive one. The crates are dense
            # with `/// chain [`X::with_y`] to enable ...`, and that line sits
            # directly above the definition, where the name is certain to appear.
            if line.lstrip().startswith("//"):
                continue
            if f".{name}(" in line or f"::{name}(" in line:
                try:
                    label = str(path.relative_to(REPO_ROOT))
                except ValueError:
                    label = str(path)
                callers.append(f"{label}:{n}")
    return callers


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--strict", action="store_true", help="exit 1 on any uncalled builder")
    args = parser.parse_args()

    files = [
        REPO_ROOT / line
        for line in subprocess.run(
            ["git", "ls-files", "crates/**/*.rs"],
            cwd=REPO_ROOT, capture_output=True, text=True, check=True,
        ).stdout.split()
    ]
    # A file deleted in the worktree can still be listed by `git ls-files`.
    sources = {
        path: path.read_text(encoding="utf-8", errors="ignore")
        for path in files
        if path.is_file()
    }

    builders: list[tuple[str, Path]] = []
    for path, text in sources.items():
        prod = strip_tests(text)
        for match in BUILDER.finditer(prod):
            if SETS_SOME.search(body_after(prod, match.start())):
                builders.append((match.group(1), path))

    uncalled: list[tuple[str, Path]] = []
    local_only: list[tuple[str, Path, list[str]]] = []

    for name, definition in builders:
        home = crate_of(definition)
        callers = production_callers(name, sources)
        outside = [c for c in callers if not c.startswith(f"crates/{home}/")]
        if not callers:
            uncalled.append((name, definition))
        elif not outside:
            local_only.append((name, definition, callers))

    # A builder is identified by name and crate, disambiguated by module only
    # when one crate defines the same name twice.
    ambiguous = {n for n, _ in builders if sum(1 for m, _ in builders if m == n) > 1}

    def key_of(name: str, definition: Path) -> str:
        base = f"{name}@{crate_of(definition)}"
        return f"{base}:{definition.stem}" if name in ambiguous else base

    found = {key_of(n, d): d for n, d in uncalled}

    print(f"{len(builders)} optional-capability builders across the workspace")
    print(f"  called from another crate : {len(builders) - len(uncalled) - len(local_only)}")
    print(f"  called only inside their own crate : {len(local_only)}")
    print(f"  never called outside tests : {len(uncalled)}")

    open_defects = sum(1 for k in found if BASELINE.get(k, "").startswith("defect, open"))
    print(f"  of which open defects : {open_defects}")

    unjudged = sorted(found.keys() - BASELINE.keys())
    stale = sorted(BASELINE.keys() - found.keys())

    for key in sorted(found):
        verdict = BASELINE.get(key, "NOT IN BASELINE")
        print(f"\n  {key}\n      {found[key].relative_to(REPO_ROOT)}\n      {verdict}")

    if local_only:
        print("\nCalled only from inside the defining crate, which is production for a")
        print("binary crate. Listed, not gated:")
        for name, definition, callers in sorted(local_only, key=lambda x: x[0]):
            print(f"  {name:<30} <- {callers[0]}")

    if not args.strict:
        return 0

    problems: list[str] = []
    # A baseline entry that says nothing is not a verdict. The docstring above
    # promises "a verdict someone reached by reading the call path", and for
    # eight entries the text read "unjudged capability, no doc claim.", which is
    # the sentence someone writes instead of reading it. The guard was green by
    # inscription rather than by judgement, so the word is refused here.
    for key, verdict in sorted(BASELINE.items()):
        if "unjudged" in verdict.lower() or not verdict.strip():
            problems.append(
                f"{key}: the baseline entry carries no verdict. Read the call path, "
                f"then say what the capability does today and what wiring it would "
                f"take, or delete the builder."
            )
    for key in unjudged:
        problems.append(
            f"{key}: a builder installs an optional capability and nothing in "
            f"production calls it. Read the call path, then either wire it, delete "
            f"it, or add it to BASELINE with the verdict you reached."
        )
    for key in stale:
        problems.append(
            f"{key}: listed as uncalled but it now has a production caller, or the "
            f"builder is gone. Remove the BASELINE entry so the list keeps meaning "
            f"something."
        )

    if problems:
        print(f"\n{len(problems)} problem(s):\n", file=sys.stderr)
        for p in problems:
            print(f"  {p}\n", file=sys.stderr)
        return 1

    print(f"\nevery uncalled builder is accounted for ({open_defects} open defect(s) tracked)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
