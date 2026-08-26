#!/usr/bin/env python3
"""Cross every `pub` item of a library crate with the places that use it.

A Rust library gets no compiler warning for a `pub` item nobody calls:
`dead_code` stops at the crate boundary, and everything a crate exports is
reachable in principle. So an item can be written, documented, and never
referenced anywhere, and the build stays green forever. Eleven module-level
items and forty-three methods were in exactly that state when this guard was
written, including an error enum nobody ever constructs, a whole mDNS
announcement path for a role Apollia does not play, and eight repository
methods duplicating their synchronous twin.

The second shape is worse, because it reads as covered: an item whose only
callers are tests. The tests pass, the coverage counts it, and production
never reaches it. `MemoryManager::start_auto_purge` sat there while
`manifest.rs` promised that `auto_purge = true` triggers a purge pass at
startup, and `llm_timings::observe_completion` documented itself as "the call
site a completion path uses" with no completion path calling it.

So two classes, and one verdict each:

  dead       zero reference outside its own definition, in production or in
             test code. Always a defect: delete it.
  test-only  referenced, but only from test code. A defect unless the item
             carries an explicit `TEST-ONLY:` line in the twelve lines above
             its definition, saying why it exists and what production uses
             instead. The marker follows the `// SAFETY:` and `# REASON:`
             doctrine of `docs/agents/FORBIDDEN.md`: an exemption is written
             down where the reader stands, or it does not exist.

One family is handed over rather than judged here: a `pub fn with_*` builder
that installs an optional capability belongs to `check_optional_builders.py`,
which names each uncalled one with the verdict someone reached by reading its
call path. Judging those twice would mean two guards demanding opposite things
of the same line, so this one skips the builders that guard already names.

What counts as test code: any file under a `tests/`, `benches/`, `examples/`
or `fuzz/` directory, any `tests.rs`, and the `#[cfg(test)] mod` block at the
end of a production file. Items declared under `#[cfg(fuzzing)]` are not
judged: they are compiled only by cargo-fuzz, whose targets live under
`fuzz/`, which this guard reads as test code.

The reading is lexical, on identifiers with a word boundary, comments
stripped. Two consequences, both stated rather than hidden:

  - a name shared by several definitions (`new`, `len`) is credited with every
    occurrence, so the guard under-reports dead items and never over-reports
    one. That bias is the safe direction for a gate.
  - a name that appears only in a doc-comment is not a use. The prototype this
    guard replaces counted comments, which let a `/// see [`X`]` line three
    lines above the definition resurrect a dead item.

Verdict by exit code, since the caller reads it rather than the text:

  0  no dead item, and every test-only item carries its justification
  1  at least one dead item, or a test-only item with no justification
  2  nothing was measured: no crate, or no `.rs` file, was found

`--list` prints every item of both classes rather than the summary.
`--selftest` runs the guard against a built subject, in a temporary
directory, never against this tree.
"""

import argparse
import re
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import NamedTuple

REPO_ROOT = Path(__file__).resolve().parent.parent

# `pub fn|struct|enum|trait|type|const|static NAME`, indented or not.
# `pub(crate)` and `pub(super)` are not exported surface and are left alone;
# the compiler already warns about an unused one.
DEFINITION = re.compile(
    r"^(\s*)pub\s+(?:(?:async|unsafe|const|extern\s+\"C\")\s+)*"
    r"(fn|struct|enum|trait|type|const|static)\s+([A-Za-z_][A-Za-z0-9_]*)"
)
# Top-level construct a definition can sit inside; only `impl` and `trait`
# turn the definition into a method.
BLOCK_HEADER = re.compile(
    r"^(impl|trait|mod|fn|pub\s+mod|pub\s+trait|pub\s+fn|pub\(crate\)\s+\w+|extern)\b"
)
IDENTIFIER = re.compile(r"[A-Za-z_][A-Za-z0-9_]*")
LINE_COMMENT = re.compile(r"//.*$", re.M)
TEST_MODULE = re.compile(r"^#\[cfg\(test\)\]\s*\nmod\s", re.M)
TEST_DIRECTORIES = {"tests", "benches", "examples", "fuzz"}
MARKER = "TEST-ONLY:"
MARKER_WINDOW = 12
# `"name@crate...": "verdict"` keys of the BASELINE table of the sibling guard.
BUILDER_BASELINE_KEY = re.compile(r'^\s*"(with_\w+)@[^"]+":', re.M)
BUILDER_GUARD = Path("scripts/check_optional_builders.py")


# ── Ratchets ────────────────────────────────────────────────────────────────

# Items this guard reports that the pass which created it did not close. Two
# reasons, both stated rather than folded into a green: they sit outside the
# count that pass was scoped to, and two of them belong to a call path another
# pass is wiring at the same time. Each carries the verdict a reader reached by
# following its call path.
#
# The list can only shrink. An entry whose item is gone, or that acquires a
# reference, fails the guard as stale, which is what stops an exemption list
# from rotting into decoration.
DEAD_RATCHET: dict[str, str] = {
    "default_input@apollia-stt": (
        "convenience wrapper over `AudioCapture::open(None)`; every caller names a device or resolves one first."
    ),
    "delete_draft@apollia-connectors": "same chain as `list_drafts`.",
    "duck_type_agent@apollia-aip": (
        "strict duck-typing against a venv `site-packages`; the CLI install path defers it per agent and never calls it."
    ),
    "from_backend_configs@apollia-llm": (
        "a thin forward to `from_backend_configs_with_override`, which is the one every caller uses."
    ),
    "is_significant_for_inactivity@apollia-core": (
        "the inactivity watcher documents this predicate in a doc-comment and filters on a list of its own."
    ),
    "list_drafts@apollia-connectors": (
        "Gmail draft management is exposed by the client and by no connector action, so no agent and no route can reach it."
    ),
    "load_package@apollia-aip": (
        "the package-wide duck-typing entry, deferred for the same reason as `duck_type_agent`."
    ),
    "run_task_with_alternatives@apollia-oria": (
        "the CLI consumes `PlanAlternativesProposed` and nothing emits it; the missing half is the emission, not this method."
    ),
    "with_style_detection@apollia-workspace": (
        "the style provider is installed by `from_providers_config` from the stored configuration, never by this builder."
    ),
}

# `pub` items reached only from test code on the day this guard was written.
# They are held as one block, with one verdict, because that is the honest
# shape of what was measured: the pass that wrote this guard read the promises
# (`start_auto_purge`, wired; `observe_completion` and `TODO_WRITE_SCHEMA`,
# deleted; the `empty_shared_*` constructors, whose doc claimed a boot path
# that does not exist) and did not read the remaining sixty-six one by one.
# Writing sixty-six invented verdicts would be the "unjudged capability"
# entry this corpus already paid for once.
#
# So the block is a ratchet, not an acquittal: nothing new may join it, and an
# entry that leaves must leave the list too. The way out of it, item by item,
# is a `TEST-ONLY:` line above the definition saying why it exists.
TEST_ONLY_RATCHET: frozenset[str] = frozenset([
    "always_accept_default@apollia-runtime", "backend_names@apollia-llm",
    "build_a2a_context@apollia-runtime", "build_router_for_test@apollia-runtime", "complete_with_fallback@apollia-llm",
    "context_window_pct@apollia-core", "create_project_async@apollia-tools", "data_dir_or_err@apollia-core",
    "estimate_tokens@apollia-oria", "filter_kinds@apollia-runtime", "from_config_with_bus@apollia-llm",
    "from_profile@apollia-prompts", "from_repository@apollia-llm", "from_risk_score@apollia-tools",
    "get_choice@apollia-memory", "get_covered_topics@apollia-memory", "get_last_onboarding_session@apollia-memory",
    "get_onboarding_skipped@apollia-memory", "get_state@apollia-triggers", "global_tracker_clear@apollia-memory",
    "handle_budget_update@apollia-notifications", "has_reasoner@apollia-oria", "in_memory@apollia-mcp",
    "into_tool_filter@apollia-runtime", "is_enabled@apollia-tools", "language_footer@apollia-prompts",
    "list_entries@apollia-memory", "list_extras@apollia-memory", "load_step_provenance@apollia-oria",
    "log_plan_choice@apollia-memory", "maybe_purge@apollia-memory", "new_for_test@apollia-runtime",
    "new_with_cwd@apollia-oria", "new_with_workspace@apollia-runtime", "notify_agent_free@apollia-triggers",
    "observe@apollia-oria", "open_with_signer@apollia-runtime", "parse_tool_name@apollia-mcp",
    "plan_with_tombstones@apollia-runtime", "prune_deleted@apollia-memory", "query_by_task@apollia-llm",
    "query_last@apollia-tools", "recall_all_for_injection@apollia-memory", "register_tool@apollia-oria",
    "remove_rule@apollia-permissions", "remove_rules_by_creator@apollia-permissions",
    "reset_timer@apollia-notifications", "resolve_backend@apollia-runtime", "seed_session_cost_usd@apollia-llm",
    "set_last_onboarding_session@apollia-memory", "set_section@apollia-aip", "set_turn_id@apollia-aip",
    "supports_service@apollia-connectors", "sweep_now@apollia-runtime", "telemetry_for@apollia-runtime",
    "tracked_turns@apollia-memory", "try_with_default_backends@apollia-tools", "validate_id@apollia-triggers",
    "with_consequences@apollia-core", "with_index_path@apollia-auth", "with_max_output_chars@apollia-tools",
    "with_namespace@apollia-aip", "with_pattern@apollia-tools", "with_produced_plan@apollia-runtime",
    "with_storage@apollia-auth",
])


def tracked_rust_files(root: Path) -> list[Path]:
    """Every `.rs` file git tracks, as paths relative to `root`."""
    listing = subprocess.run(
        ["git", "ls-files", "--", "*.rs"],
        cwd=root,
        capture_output=True,
        text=True,
        check=False,
    )
    if listing.returncode != 0:
        return []
    return [Path(line) for line in listing.stdout.split() if line]


def library_crates(root: Path) -> dict[str, Path]:
    """Crate name to source directory, for every crate that has a `src/lib.rs`.

    Read from the tree rather than from `cargo metadata`: the guard runs in
    pre-commit and on machines where a cargo invocation would wait on the
    build lock for minutes.
    """
    crates: dict[str, Path] = {}
    for manifest in sorted((root / "crates").glob("*/Cargo.toml")):
        directory = manifest.parent
        if (directory / "src" / "lib.rs").is_file():
            crates[directory.name] = directory.relative_to(root)
    return crates


def builders_judged_elsewhere(root: Path) -> set[str]:
    """Builder names `check_optional_builders.py` already carries a verdict for."""
    guard = root / BUILDER_GUARD
    if not guard.is_file():
        return set()
    text = guard.read_text(encoding="utf-8", errors="replace")
    return set(BUILDER_BASELINE_KEY.findall(text))


def is_test_file(path: Path) -> bool:
    """Integration tests, benches, examples and fuzz targets are not production."""
    return bool(set(path.parts) & TEST_DIRECTORIES) or path.name == "tests.rs"


def split_production(text: str) -> tuple[str, str]:
    """Return (production text, test text) by cutting at the `#[cfg(test)] mod`.

    Cutting at the first bare `#[cfg(test)]` would be wrong: the attribute also
    sits on a lone const or helper in the middle of a file, and everything below
    it would be read as test code, which reports live items as dead.
    """
    match = TEST_MODULE.search(text)
    if match is None:
        return text, ""
    return text[: match.start()], text[match.start() :]


def strip_comments(text: str) -> str:
    """Drop `//` comments, doc-comments included. A mention is not a use."""
    return LINE_COMMENT.sub("", text)


def collect_definitions(path: Path, production: str) -> list[dict]:
    """Every `pub` item defined in the production part of one file."""
    items: list[dict] = []
    lines = production.splitlines()
    context: str | None = None
    fuzz_only = False
    for number, line in enumerate(lines, 1):
        stripped = line.strip()
        if stripped.startswith("#[cfg(fuzzing)]"):
            fuzz_only = True
            continue
        if line[:1] not in (" ", "\t") and stripped:
            header = BLOCK_HEADER.match(line)
            context = header.group(1).split()[0] if header else None
            if context == "pub":
                context = line.split()[1]
        match = DEFINITION.match(line)
        if match is None:
            if stripped and not stripped.startswith(("#[", "///", "//")):
                fuzz_only = False
            continue
        indent, kind, name = match.groups()
        if fuzz_only:
            fuzz_only = False
            continue
        justified = any(
            MARKER in lines[index]
            for index in range(max(0, number - 1 - MARKER_WINDOW), number - 1)
        )
        items.append(
            {
                "file": path,
                "line": number,
                "kind": kind,
                "name": name,
                "method": bool(indent) and context in ("impl", "trait"),
                "justified": justified,
            }
        )
    return items


def count_occurrences(text: str) -> dict[str, int]:
    counts: dict[str, int] = {}
    for match in IDENTIFIER.finditer(text):
        name = match.group(0)
        counts[name] = counts.get(name, 0) + 1
    return counts


class Measure(NamedTuple):
    """What one pass over a tree found."""

    dead: list[dict]
    test_only: list[dict]
    new_dead: list[dict]
    new_test_only: list[dict]
    stale: list[str]
    judged: int


def item_key(item: dict, crates: dict[str, Path]) -> str:
    """`name@crate`, the identity the ratchets are written in."""
    crate = next(
        (name for name, d in crates.items() if str(item["file"]).startswith(f"{d}/src/")),
        "?",
    )
    return f'{item["name"]}@{crate}'


def report(
    root: Path,
    dead_ratchet: dict[str, str] | None = None,
    test_only_ratchet: frozenset[str] | None = None,
) -> tuple[int, Measure]:
    """Measure one tree. Returns (exit code, measure).

    The two ratchets are parameters so the self-test can run against a built
    subject without the repository's own entries turning up stale there.
    """
    dead_ratchet = DEAD_RATCHET if dead_ratchet is None else dead_ratchet
    test_only_ratchet = (
        TEST_ONLY_RATCHET if test_only_ratchet is None else test_only_ratchet
    )
    crates = library_crates(root)
    files = tracked_rust_files(root)
    if not crates or not files:
        return 2, Measure([], [], [], [], [], 0)

    production_counts: dict[str, int] = {}
    test_counts: dict[str, int] = {}
    definitions: list[dict] = []
    for path in files:
        absolute = root / path
        if not absolute.is_file():
            continue
        text = absolute.read_text(encoding="utf-8", errors="replace")
        if is_test_file(path):
            for name, n in count_occurrences(strip_comments(text)).items():
                test_counts[name] = test_counts.get(name, 0) + n
            continue
        production, tests = split_production(text)
        for name, n in count_occurrences(strip_comments(production)).items():
            production_counts[name] = production_counts.get(name, 0) + n
        for name, n in count_occurrences(strip_comments(tests)).items():
            test_counts[name] = test_counts.get(name, 0) + n
        owner = next(
            (name for name, d in crates.items() if str(path).startswith(f"{d}/src/")),
            None,
        )
        if owner is None:
            continue
        definitions.extend(collect_definitions(path, production))

    handed_over = builders_judged_elsewhere(root)
    dead: list[dict] = []
    unjustified: list[dict] = []
    for item in definitions:
        if item["method"] and item["name"] in handed_over:
            continue
        # The definition line itself is one production occurrence of the name.
        elsewhere = production_counts.get(item["name"], 0) - 1
        in_tests = test_counts.get(item["name"], 0)
        if elsewhere <= 0 and in_tests == 0:
            dead.append(item)
        elif elsewhere <= 0 and not item["justified"]:
            unjustified.append(item)

    for item in dead:
        item["key"] = item_key(item, crates)
    for item in unjustified:
        item["key"] = item_key(item, crates)

    new_dead = [i for i in dead if i["key"] not in dead_ratchet]
    new_test_only = [i for i in unjustified if i["key"] not in test_only_ratchet]
    stale = sorted(
        (set(dead_ratchet) - {i["key"] for i in dead})
        | (set(test_only_ratchet) - {i["key"] for i in unjustified})
    )

    code = 1 if new_dead or new_test_only or stale else 0
    return code, Measure(dead, unjustified, new_dead, new_test_only, stale, len(definitions))


def render(measure: Measure, verbose: bool) -> None:
    print(f"{measure.judged} exported `pub` items judged across the library crates")
    print(f"  dead, referenced nowhere    : {len(measure.dead)}"
          f" ({len(measure.new_dead)} outside the ratchet)")
    print(f"  reached only by tests       : {len(measure.test_only)}"
          f" ({len(measure.new_test_only)} outside the ratchet)")
    print(f"  ratchet entries now stale   : {len(measure.stale)}")

    def show(label: str, items: list[dict]) -> None:
        for item in sorted(items, key=lambda i: (str(i["file"]), i["line"])):
            suffix = " (method)" if item["method"] else ""
            print(
                f"  {label} {item['file']}:{item['line']}: "
                f"pub {item['kind']} {item['name']}{suffix}"
            )

    show("dead     ", measure.new_dead if not verbose else measure.dead)
    show("test-only", measure.new_test_only if not verbose else measure.test_only)
    for key in measure.stale:
        print(f"  stale     {key}")

    if measure.new_dead:
        print(
            "\nA `pub` item nothing references is dead: delete it. If it must stay,\n"
            "add it to DEAD_RATCHET with the verdict you reached by reading its\n"
            "call path.",
            file=sys.stderr,
        )
    if measure.new_test_only:
        print(
            f"\nA `pub` item only tests reach is a promise production never keeps.\n"
            f"Wire it, delete it, or write a `{MARKER} <why>` line in the "
            f"{MARKER_WINDOW} lines\nabove its definition, saying what production "
            f"uses instead.",
            file=sys.stderr,
        )
    if measure.stale:
        print(
            "\nA ratchet entry no longer matches an item: the item is gone, or it\n"
            "acquired a reference. Remove the entry so the list keeps meaning\n"
            "something.",
            file=sys.stderr,
        )


# ── Self-test ────────────────────────────────────────────────────────────────


SUBJECT_LIB = """\
pub mod live;
pub mod suspect;
"""

SUBJECT_LIVE = """\
use crate::suspect::Kept;

/// Called by the binary.
pub fn entry_point() -> u32 {
    Kept::new().value()
}
"""

SUBJECT_CLEAN_SUSPECT = """\
/// A type production builds.
pub struct Kept {
    value: u32,
}

impl Kept {
    /// Build one.
    pub fn new() -> Self {
        Self { value: 1 }
    }

    /// Read it back.
    pub fn value(&self) -> u32 {
        self.value
    }
}

/// Only the tests build one of these.
// TEST-ONLY: the production path constructs `Kept` directly; this shortcut
// exists so a test can assert on a fixed value.
pub fn fixture() -> Kept {
    Kept { value: 7 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fixture_is_seven() {
        assert_eq!(fixture().value(), 7);
    }
}
"""


def _write_subject(root: Path, suspect: str) -> None:
    (root / "crates" / "sample" / "src").mkdir(parents=True, exist_ok=True)
    (root / "crates" / "sample" / "Cargo.toml").write_text(
        '[package]\nname = "sample"\n', encoding="utf-8"
    )
    # A binary crate, because a library item is only ever alive by being called
    # from outside its crate: a subject without one reports every export dead.
    (root / "crates" / "app" / "src").mkdir(parents=True, exist_ok=True)
    (root / "crates" / "app" / "Cargo.toml").write_text(
        '[package]\nname = "app"\n', encoding="utf-8"
    )
    (root / "crates" / "app" / "src" / "main.rs").write_text(
        "fn main() {\n    let _ = sample::live::entry_point();\n}\n", encoding="utf-8"
    )
    (root / "crates" / "sample" / "src" / "lib.rs").write_text(SUBJECT_LIB, encoding="utf-8")
    (root / "crates" / "sample" / "src" / "live.rs").write_text(SUBJECT_LIVE, encoding="utf-8")
    (root / "crates" / "sample" / "src" / "suspect.rs").write_text(suspect, encoding="utf-8")
    subprocess.run(["git", "init", "-q"], cwd=root, check=True)
    subprocess.run(["git", "add", "-A"], cwd=root, check=True)


def _with_extra(suspect: str, extra: str) -> str:
    """Insert `extra` before the `#[cfg(test)]` block, where production ends."""
    cut = suspect.index("#[cfg(test)]")
    return suspect[:cut] + extra + "\n" + suspect[cut:]


def selftest() -> int:
    failures: list[str] = []

    def case(label: str, ok: bool, detail: str) -> None:
        print(f"  {'ok  ' if ok else 'FAIL'}  {label}")
        if not ok:
            failures.append(f"{label}: {detail}")

    print("check_pub_unreferenced.py --selftest")

    with tempfile.TemporaryDirectory() as raw:
        root = Path(raw) / "clean"
        root.mkdir()
        _write_subject(root, SUBJECT_CLEAN_SUSPECT)
        code, measure = report(root, {}, frozenset())
        dead, unjustified, total = measure.new_dead, measure.new_test_only, measure.judged
        case(
            "a tree whose test-only item is justified is green",
            code == 0 and not dead and not unjustified,
            f"exit {code}, dead {[i['name'] for i in dead]}, "
            f"unjustified {[i['name'] for i in unjustified]}",
        )
        case("the judged items are counted", total >= 4, f"total {total}")

    with tempfile.TemporaryDirectory() as raw:
        root = Path(raw) / "dead"
        root.mkdir()
        _write_subject(
            root,
            _with_extra(
                SUBJECT_CLEAN_SUSPECT,
                "/// Nothing references this.\npub fn orphan() -> u32 {\n    3\n}\n",
            ),
        )
        code, measure = report(root, {}, frozenset())
        dead, unjustified = measure.new_dead, measure.new_test_only
        case(
            "an item referenced nowhere is a red",
            code == 1 and [i["name"] for i in dead] == ["orphan"],
            f"exit {code}, dead {[i['name'] for i in dead]}",
        )

    with tempfile.TemporaryDirectory() as raw:
        root = Path(raw) / "unjustified"
        root.mkdir()
        _write_subject(root, SUBJECT_CLEAN_SUSPECT.replace(
            "// TEST-ONLY: the production path constructs `Kept` directly; this shortcut\n"
            "// exists so a test can assert on a fixed value.\n",
            "",
        ))
        code, measure = report(root, {}, frozenset())
        dead, unjustified = measure.new_dead, measure.new_test_only
        case(
            "a test-only item without its marker is a red",
            code == 1 and [i["name"] for i in unjustified] == ["fixture"],
            f"exit {code}, unjustified {[i['name'] for i in unjustified]}",
        )

    with tempfile.TemporaryDirectory() as raw:
        root = Path(raw) / "comment"
        root.mkdir()
        _write_subject(
            root,
            _with_extra(
                SUBJECT_CLEAN_SUSPECT,
                "/// See [`orphan`] for the shape.\npub fn orphan() -> u32 {\n    3\n}\n",
            ),
        )
        code, measure = report(root, {}, frozenset())
        dead = measure.new_dead
        case(
            "a name mentioned only by a comment stays dead",
            code == 1 and [i["name"] for i in dead] == ["orphan"],
            f"exit {code}, dead {[i['name'] for i in dead]}",
        )

    with tempfile.TemporaryDirectory() as raw:
        root = Path(raw) / "fuzzing"
        root.mkdir()
        _write_subject(
            root,
            _with_extra(
                SUBJECT_CLEAN_SUSPECT,
                "/// Fuzz shim.\n#[cfg(fuzzing)]\npub fn __fuzz_orphan() -> u32 {\n    3\n}\n",
            ),
        )
        code, measure = report(root, {}, frozenset())
        dead, unjustified = measure.new_dead, measure.new_test_only
        case(
            "a `#[cfg(fuzzing)]` shim is not judged",
            code == 0 and not dead and not unjustified,
            f"exit {code}, dead {[i['name'] for i in dead]}, "
            f"unjustified {[i['name'] for i in unjustified]}",
        )

    with tempfile.TemporaryDirectory() as raw:
        root = Path(raw) / "ratcheted"
        root.mkdir()
        _write_subject(
            root,
            _with_extra(
                SUBJECT_CLEAN_SUSPECT,
                "/// Nothing references this.\npub fn orphan() -> u32 {\n    3\n}\n",
            ),
        )
        code, measure = report(root, {"orphan@sample": "held"}, frozenset())
        case(
            "a ratchet entry silences the item it names",
            code == 0 and not measure.new_dead and not measure.stale,
            f"exit {code}, new {[i['name'] for i in measure.new_dead]}, "
            f"stale {measure.stale}",
        )

    with tempfile.TemporaryDirectory() as raw:
        root = Path(raw) / "stale"
        root.mkdir()
        _write_subject(root, SUBJECT_CLEAN_SUSPECT)
        code, measure = report(root, {"gone@sample": "held"}, frozenset())
        case(
            "a ratchet entry whose item is gone is a red",
            code == 1 and measure.stale == ["gone@sample"],
            f"exit {code}, stale {measure.stale}",
        )

    with tempfile.TemporaryDirectory() as raw:
        root = Path(raw) / "empty"
        root.mkdir()
        subprocess.run(["git", "init", "-q"], cwd=root, check=True)
        code, _ = report(root, {}, frozenset())
        case("a tree with no crate measures nothing", code == 2, f"exit {code}")

    if failures:
        print(f"\n{len(failures)} selftest failure(s):", file=sys.stderr)
        for failure in failures:
            print(f"  {failure}", file=sys.stderr)
        return 1
    print("selftest green")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Cross every `pub` item of a library crate with the places that use it.",
    )
    parser.add_argument(
        "--list", action="store_true", help="print every item of both classes"
    )
    parser.add_argument(
        "--selftest",
        action="store_true",
        help="run the guard against a built subject in a temporary directory",
    )
    parser.add_argument(
        "--root",
        default=str(REPO_ROOT),
        help="tree to measure (default: the repository this script lives in)",
    )
    args = parser.parse_args()

    if args.selftest:
        return selftest()

    root = Path(args.root).resolve()
    code, measure = report(root)
    if code == 2:
        print(f"nothing measured: no library crate or no tracked .rs file under {root}")
        return 2
    render(measure, args.list)
    return code


if __name__ == "__main__":
    sys.exit(main())
