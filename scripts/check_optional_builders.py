#!/usr/bin/env python3
"""Find `with_*` builders that install a capability nobody installs.

Three times now this repository has produced the same shape: a `pub fn with_x`
that sets an optional field to `Some`, a default path that leaves it `None`, and
a documented capability that therefore never runs.

  - `with_permission_engine`: the permission engine documented on five pages,
    wired in no shipped binary.
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
# The body assigns Some(...) to a field, which is the shape that matters: a
# builder setting a plain value cannot leave a capability silently absent.
SETS_SOME = re.compile(r"=\s*Some\(|:\s*Some\(")


# Every builder with no production caller, and why it is tolerated. Reviewed
# against the call path on 2026-07-31; `defect` entries are open, not accepted.
BASELINE: dict[str, str] = {
    "with_judge@apollia-eval": "defect, contained: the CLI builds the runner with "
    "`new`, so no judge is ever installed. The assertion now fails explicitly "
    "instead of counting as passed, so the capability is absent but no longer "
    "silently green.",
    "with_journal@apollia-tools:file_write": "defect, open: `apollia-os rollback` "
    "reads a journal that nothing writes. All three production sites build the "
    "tool with `new`, and every `JournalWriter::spawn` in the workspace is in a "
    "test. The published how-to `audit-verify-rollback` documents the undo.",
    "with_journal@apollia-tools:file_edit": "defect, open: same chain as "
    "file_write, same rollback page.",
    "with_plan_cache@apollia-oria": "defect, open: the repository is opened at "
    "boot and exposed for REST stats and clear, but never handed to the engine, "
    "so `lookup_cached_plan` returns on its first line and no plan is ever "
    "reused. `explanation/the-plan-model.md` says plans are cached and reused.",
    "with_tool_offload@apollia-oria": "defect, open: no offload store is ever "
    "installed, so large tool results are never spilled to disk. "
    "`architecture/05-building-blocks.md` lists disk offload as a capability.",
    "with_db_path@apollia-oria": "defect, open: nothing sets it, so the engine "
    "falls back to `:memory:` in production. Not documented either way, which is "
    "its own problem.",
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
    "with_permission_engine@apollia-tools": "known and documented: the first "
    "instance of this class. The corpus now states it is inert unless a host "
    "embeds Apollia and installs one, guarded by the claim "
    "`permission-engine-not-wired`. Accepted, not a defect.",
    "with_agent_manifest@apollia-tools": "unjudged capability, no doc claim.",
    "with_session_filter@apollia-tools": "unjudged capability, no doc claim.",
    "with_file_path_extractor@apollia-tools": "unjudged capability, no doc claim.",
    "with_timestamp_cache@apollia-tools": "unjudged capability, no doc claim.",
    "with_telemetry@apollia-runtime": "unjudged capability, no doc claim.",
    "with_thinking@apollia-core": "unjudged capability, no doc claim.",
    "with_details@apollia-runner": "unjudged capability, no doc claim.",
    "with_wall_clock_secs@apollia-aip:bridge": "unjudged capability, no doc claim.",
    "with_wall_clock_secs@apollia-aip:context": "unjudged capability, no doc claim.",
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
    sources = {path: path.read_text(encoding="utf-8", errors="ignore") for path in files}

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
        callers: list[str] = []
        for path, text in sources.items():
            if is_test_path(path):
                continue
            # The defining file is searched too, not skipped. `engine.rs` defines
            # `with_memory_manager` on one type and calls it on another a
            # thousand lines apart; skipping the file reported a wired
            # capability as dead.
            for n, line in enumerate(strip_tests(text).splitlines(), 1):
                # A doc-comment naming the builder is not a caller. Counting one
                # reports a dead capability as wired, and this file exists
                # because that direction of error is the expensive one. The
                # crates are dense with `/// chain [\`X::with_y\`] to enable...`.
                if line.lstrip().startswith("//"):
                    continue
                if f".{name}(" in line or f"::{name}(" in line:
                    callers.append(f"{path.relative_to(REPO_ROOT)}:{n}")
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
