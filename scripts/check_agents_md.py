#!/usr/bin/env python3
"""A crate rulebook names its own crate, or it is a specification of somewhere else.

The seven subtree `AGENTS.md` were written as specifications and never replayed
against the code. Measured on `f6d89f24`, before this guard existed, sixteen
symbols or structures they described did not exist: `StepBudgetConfig` with
five dimensions where `apollia-core` carries three, `Budget::tick`,
`OriaError::InvalidBudget`, `CircuitBreakerConfig`,
`RuntimeEvent::TaskBudgetExceeded`, `RuntimeEvent::BackendCircuitOpen`,
`RuntimeEvent::HumanInputRequested`, `TaskAbandoned`, `ctx.cache.invalidate`,
a TOML `[pipeline.X]` runner, `~/.apollia/runtime.db`, `~/.apollia/auth.toml`,
a `SIGHUP` reload, `pub enum AipError`, an `agent.aip.*` span family and
`AgentTimeoutError`. Each one is a `git grep -c` away from being refuted, and
nothing was doing it.

Three rules, each two-sided.

  A1  every path cited in backticks exists. Generic forms (`<crate>`, a glob,
      a URL, a path outside the repository) are excluded by shape, never by
      name, and a relative path is resolved against the repository root and
      then against the directory of the file that cites it, since a crate
      rulebook writes `src/engine.rs`.

  A2  every CamelCase identifier cited in backticks and presented as a name in
      force exists in `crates/`, `sdk/apollia/` or `agents/`. What counts as
      "in force" is the shape of the citation: a path-qualified name
      (`RuntimeEvent::TaskBudgetExceeded`), a call (`Budget::tick(...)`), or a
      bare identifier on a line the document did not mark as illustrative.
      Names inside a fenced code block are read too, since that is where the
      struct definitions live.

  A3  the rule the root `AGENTS.md` states about creating a new subtree
      rulebook is decidable. Every crate whose `src/` exceeds
      `SIZE_THRESHOLD` tracked lines either carries an `AGENTS.md` or is named
      in `COVERED_BY_GLOBAL_RULEBOOK` with the corpus file that covers it, and
      that file exists. Without the second half the list would be a way of
      passing rather than a verdict: fifteen crates were over the threshold
      with no rulebook and no entry anywhere.

Verdict by exit code, since the caller reads it rather than the text:

  0  every rule holds on every file measured
  1  at least one rule is broken
  2  nothing was measured: no subtree `AGENTS.md` is tracked, or the git
     inventory could not be read

`--selftest` drives every classifier in both directions on fabricated inputs:
a dead path, a live one, an absent symbol, a present one, an illustrative name
that must not be reported, a crate over the threshold with neither rulebook nor
entry, and one with each.

Usage:
    python3 scripts/check_agents_md.py
    python3 scripts/check_agents_md.py --list
    python3 scripts/check_agents_md.py --selftest
"""

import argparse
import re
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]

# A crate whose src/ passes this many tracked lines owes a rulebook or an entry
# in the table below. The root AGENTS.md states the rule as "~500 lines"; this
# is that number, made decidable.
SIZE_THRESHOLD = 500

# The second half of the root rule ("AND introduces patterns the global rules
# do not cover") is a judgement, so it is recorded rather than inferred: each
# crate over the threshold without its own AGENTS.md names the corpus file that
# already carries its patterns. An entry whose file does not exist is a red,
# which is what keeps this from being a list that only ever grows.
COVERED_BY_GLOBAL_RULEBOOK: dict[str, str] = {
    "apollia-memory": "docs/agents/RUST-PATTERNS.md",
    "apollia-triggers": "docs/agents/RUST-PATTERNS.md",
    "apollia-auth": "docs/agents/SECURITY.md",
    "apollia-notifications": "docs/agents/RUST-PATTERNS.md",
    "apollia-connectors": "docs/agents/SECURITY.md",
    "apollia-workspace": "docs/agents/RUST-PATTERNS.md",
    "apollia-permissions": "docs/agents/SECURITY.md",
    "apollia-stt": "docs/agents/RUST-PATTERNS.md",
    "apollia-runner": "docs/agents/RUST-PATTERNS.md",
    "apollia-eval": "docs/agents/TESTING.md",
    "apollia-prompts": "docs/agents/RUST-PATTERNS.md",
}

BACKTICKED = re.compile(r"`([^`\n]+)`")

# A document is allowed to name something that does not exist, and every crate
# rulebook here does: naming the symbol a reader would otherwise go looking for
# is the point of saying it is gone. What separates a refutation from a claim
# is a phrase, and the phrase governs the whole paragraph, because the
# refutations in this corpus run over several lines. The list is short on
# purpose: a broad one (`no`, `not`) would exempt most technical prose and the
# guard would pass on everything.
REFUTATION = re.compile(
    r"there\s+(?:is|are|was|were)\s+no\b"
    r"|is\s+in\s+no\b|are\s+in\s+no\b"
    r"|do(?:es)?\s+not\s+exist|did\s+not\s+exist|never\s+existed"
    r"|no\s+longer\b|used\s+to\s+(?:hold|carry|be|say)"
    r"|\bdeleted\b|answered\s+0|0\s+sites|nowhere\s+today"
    r"|,\s+not\s+`|\),\s+not\s+",
    re.IGNORECASE,
)

PATH_EXT = (
    ".md", ".rs", ".py", ".toml", ".json", ".yml", ".yaml", ".sh", ".ts",
    ".svelte", ".css", ".js", ".sql", ".txt", ".html", ".svg", ".lock",
)

# Shapes that are not a path in this tree: a home-relative or absolute path, an
# HTTP route, a URL, a parent-relative walk, a web asset served by the desktop.
PATH_SKIP_PREFIX = (
    "~/", "http", "/api/", "/tmp/", "/Users/", "/private/", "/var/", "../",
    "/logo", "/agent/", "/task/", "/tool/", "/mailbox/",
    # Produced at run time, never tracked: the automaton's report directory and
    # the throwaway seed HOME the recipes build.
    ".apollia",
)

# The first segment a repository path can start with. Without it, `tools/list`
# (an MCP method), `try/catch` and `svelte/store` are read as paths and
# reported as dead, which is the way a guard ends up behind an exclusion list.
PATH_ROOTS = (
    "crates", "sdk", "docs", "scripts", "tests", "agents", "packaging",
    "clients", "src", "ui", "lib", ".github", ".cargo", ".claude", "target",
)

CAMEL = re.compile(r"^[A-Z][A-Za-z0-9]*(?:::[A-Za-z_][A-Za-z0-9_]*)*$")

# Inside a backticked span or a fenced block, every CamelCase word is a
# candidate. `EventBus::publish(RuntimeEvent::TaskBudgetExceeded { .. })` is one
# span carrying three names, and the campaign's absent variant was the third:
# reading the head alone is how a guard passes on the very defect it exists for.
# Two uppercase letters and at least one lowercase: that is a Rust or Python
# name (`AIPResult`, `JoinSet`, `AipError`) and not an English word in a
# docstring (`Triages`) nor a file stem in a path (`RUST-PATTERNS.md`). A
# single-hump name like `Interrupt` is out of reach on purpose: it is
# indistinguishable from prose, and a guard that reads prose ends up excluded.
CAMEL_WORD = re.compile(
    r"\b(?=[A-Za-z0-9]*[a-z])(?=(?:[A-Za-z0-9]*[A-Z]){2})[A-Z][A-Za-z0-9]{2,}\b"
)

# Names a document invents so a reader has something to look at. They are not
# claims about the tree and must not be reported, and the list is driven from
# both sides: an entry that names something the tree now carries, or that no
# document cites any more, is itself a red. Without that, the list is a way of
# passing rather than a set of exceptions.
ILLUSTRATIVE = {
    "EmailPayload", "EmailResult", "EmailTriage", "MyClass",
}

# Identifiers that are real and live outside the three source roots: the
# language, the ecosystem, and the tools the documents legitimately name.
FOREIGN_SYMBOLS = {
    "Apollia", "Observer", "Reasoner", "Actor", "Rust", "Python", "Tokio",
    "SQLite", "JSON", "TOML", "HTTP", "HTTPS", "TTY", "GIL", "WAL", "FTS5",
    "Jinja", "Jinja2", "OAuth", "MCP", "SDK", "CLI", "UI", "API", "ORIA",
    "AgentKit", "Tauri", "Svelte", "Vitest", "Playwright", "TypeScript",
    "Tailwind", "WCAG", "DevTools", "PR", "ICU", "MessageFormat", "SSE",
    "Duration", "Instant", "String", "Option", "Result", "Vec", "Arc",
    "Mutex", "RwLock", "Semaphore", "Protocol", "TypedDict", "NotRequired",
    "Literal", "Exception", "Sync", "Send", "Self", "None", "True", "False",
    "Promise", "Route", "Dialog", "Popover", "Select", "DropdownMenu",
    "CancelledError", "KeyboardInterrupt", "NotImplementedError",
    "Logger", "Sparkles", "SIGINT", "SIGTERM", "SIGHUP", "PEP", "UTF",
}


def tracked(pattern: str) -> list[str]:
    out = subprocess.run(
        ["git", "ls-files", pattern],
        cwd=REPO_ROOT, capture_output=True, text=True, check=False,
    )
    return [] if out.returncode != 0 else out.stdout.split()


def subtree_rulebooks() -> list[str]:
    """The tracked AGENTS.md that are not the root one."""
    return sorted(p for p in tracked("*AGENTS.md") if p != "AGENTS.md")


def is_path_candidate(token: str) -> bool:
    if not token or " " in token or "$" in token or "<" in token or ">" in token:
        return False
    if "(" in token or "=" in token or "*" in token or "{" in token:
        return False
    if token.startswith(PATH_SKIP_PREFIX) or token.startswith(("-", "#", "/")):
        return False
    if "::" in token:
        return False
    if "/" in token:
        head = token.split("/", 1)[0]
        if head in PATH_ROOTS or token.endswith("/"):
            return True
        return any(seg.endswith(PATH_EXT) for seg in token.split("/"))
    # A bare `.svelte` or `.ts` is an extension being named, not a file.
    if token.startswith("."):
        return False
    return token.endswith(PATH_EXT)


def refuted_lines(text: str) -> set[int]:
    """Line numbers whose paragraph refutes rather than claims.

    A paragraph is a run of lines between blank lines; a fenced block belongs
    to the paragraph that introduces it, since a struct definition is written
    below the sentence that says whether it exists.
    """
    lines = text.splitlines()
    refuted: set[int] = set()
    start = 0
    fenced = False
    block: list[int] = []
    for idx, line in enumerate(lines, 1):
        if line.strip().startswith("```"):
            fenced = not fenced
        if not fenced and not line.strip():
            if block and REFUTATION.search("\n".join(lines[start:idx - 1])):
                refuted.update(block)
            block = []
            start = idx
            continue
        block.append(idx)
    if block and REFUTATION.search("\n".join(lines[start:])):
        refuted.update(block)
    return refuted


def dead_paths(rel_file: str, text: str, exists, basenames: set[str] | None = None) -> list[str]:
    """A1 on one document. `exists` answers whether a repo-relative path is there."""
    found = []
    parent = str(Path(rel_file).parent)
    skip = refuted_lines(text)
    for lineno, line in enumerate(text.splitlines(), 1):
        if lineno in skip:
            continue
        for token in BACKTICKED.findall(line):
            if not is_path_candidate(token):
                continue
            bare = re.sub(r":\d+(-\d+)?$", "", token).rstrip(".,;:")
            if exists(bare):
                continue
            if parent != "." and exists(f"{parent}/{bare}"):
                continue
            # A crate rulebook cites a sibling by the crate-relative name
            # (`apollia-cli/Cargo.toml` from `crates/apollia-cli/AGENTS.md`),
            # and a bare basename (`plan_cache.rs`) when the directory is
            # obvious from the sentence. Both resolve against the inventory.
            grand = str(Path(rel_file).parent.parent)
            if grand not in (".", "") and exists(f"{grand}/{bare}"):
                continue
            if "/" not in bare and basenames is not None and bare in basenames:
                continue
            found.append(f"{rel_file}:{lineno}: `{token}` does not exist")
    return found


def cited_symbols(text: str) -> list[tuple[int, str]]:
    """Every CamelCase identifier a document presents as a name in force.

    Two sources. A backticked span, read word by word rather than head first,
    so `RuntimeEvent::TaskBudgetExceeded` inside a call is read. And a fenced
    block tagged `rust` or `python`, read line by line, because that is where a
    struct definition is written and where five of the campaign's sixteen
    absent symbols were: a `pub struct CircuitBreakerConfig` inside a fence
    carries no backticks at all.

    A fence tagged anything else (`toml`, `sh`, `svelte`, `ts`, untagged) is a
    sample of configuration or of shell, where a capitalised word is not a Rust
    or Python name. Refutations are filtered by the caller, paragraph by
    paragraph; invented names come out of `ILLUSTRATIVE`.
    """
    out: list[tuple[int, str]] = []
    fence_lang: str | None = None
    for lineno, line in enumerate(text.splitlines(), 1):
        stripped = line.strip()
        if stripped.startswith("```"):
            fence_lang = None if fence_lang is not None else stripped[3:].strip().lower()
            continue
        if fence_lang in ("rust", "python", "py"):
            candidates = CAMEL_WORD.findall(line)
        else:
            candidates = [
                w for token in BACKTICKED.findall(line) for w in CAMEL_WORD.findall(token)
            ]
        for name in candidates:
            if name in FOREIGN_SYMBOLS or name in ILLUSTRATIVE:
                continue
            out.append((lineno, name))
    return out


def absent_symbols(rel_file: str, text: str, known: set[str]) -> list[str]:
    """A2 on one document."""
    found = []
    skip = refuted_lines(text)
    for lineno, name in cited_symbols(text):
        if lineno in skip:
            continue
        if name not in known:
            found.append(f"{rel_file}:{lineno}: `{name}` is in no source of the tree")
    return sorted(set(found))


def stale_illustrations(invented: set[str], cited: set[str], known: set[str]) -> list[str]:
    """The exemption list, driven from the side that would let it rot.

    An invented name the tree now carries is no longer an invention, and an
    invented name no document cites any more is an exemption outliving its
    subject. Both are reported, so the list can only shrink or be justified.
    """
    found = []
    for name in sorted(invented):
        if name in known:
            found.append(f"`{name}`: listed as an invented example, and the tree carries it")
        elif name not in cited:
            found.append(f"`{name}`: listed as an invented example, and no rulebook cites it")
    return found


def source_symbols() -> set[str]:
    """Every identifier the Rust, Python and agent sources define or name."""
    names: set[str] = set()
    word = re.compile(r"\b[A-Za-z_][A-Za-z0-9_]*\b")
    files = [
        p for p in tracked("crates/*") + tracked("sdk/apollia/*") + tracked("agents/*")
        if p.endswith((".rs", ".py"))
    ]
    for rel in files:
        try:
            body = (REPO_ROOT / rel).read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        names.update(word.findall(body))
    return names


def crate_src_lines(crate_dir: str) -> int:
    total = 0
    for rel in tracked(f"{crate_dir}/src/*"):
        if not rel.endswith(".rs"):
            continue
        try:
            body = (REPO_ROOT / rel).read_bytes()
        except OSError:
            continue
        total += body.count(b"\n")
    return total


def coverage_violations(crates: dict[str, int], has_rulebook, covered, file_exists) -> list[str]:
    """A3: every crate over the threshold is judged, and every judgement is live."""
    found = []
    for crate, lines in sorted(crates.items()):
        if lines <= SIZE_THRESHOLD or has_rulebook(crate):
            continue
        reason = covered.get(crate)
        if reason is None:
            found.append(
                f"{crate}: {lines} lines under src/ and no AGENTS.md, named by no "
                f"entry of COVERED_BY_GLOBAL_RULEBOOK"
            )
        elif not file_exists(reason):
            found.append(
                f"{crate}: covered by `{reason}`, which does not exist"
            )
    for crate in sorted(covered):
        if has_rulebook(crate):
            found.append(
                f"{crate}: carries its own AGENTS.md and is still listed as covered "
                f"by the global rulebook"
            )
        elif crate not in crates:
            found.append(f"{crate}: listed as covered, and is not a crate of this tree")
    return found


def measure(show_all: bool) -> int:
    books = subtree_rulebooks()
    if not books:
        print("nothing measured: no subtree AGENTS.md is tracked", file=sys.stderr)
        return 2
    inventory = set(tracked("*"))
    if not inventory:
        print("nothing measured: the git inventory is empty", file=sys.stderr)
        return 2

    def exists(rel: str) -> bool:
        return rel in inventory or (REPO_ROOT / rel).exists()

    basenames = {Path(p).name for p in inventory}
    known = source_symbols()
    if not known:
        print("nothing measured: no Rust or Python source was read", file=sys.stderr)
        return 2

    reds: list[str] = []
    cited: set[str] = set()
    for rel in books:
        body = (REPO_ROOT / rel).read_text(encoding="utf-8", errors="replace")
        reds.extend(dead_paths(rel, body, exists, basenames))
        reds.extend(absent_symbols(rel, body, known))
        # The denominator for the exemption cross-check is every CamelCase word
        # the document carries anywhere, fences included and nothing filtered:
        # an exemption is stale when the name has left the corpus, not when the
        # rule that reads it happens to skip that fence.
        cited.update(CAMEL_WORD.findall(body))
    reds.extend(stale_illustrations(ILLUSTRATIVE, cited, known))

    crates = {
        Path(p).parts[1]: crate_src_lines("/".join(Path(p).parts[:2]))
        for p in tracked("crates/*/Cargo.toml")
    }
    reds.extend(
        coverage_violations(
            crates,
            lambda c: (REPO_ROOT / "crates" / c / "AGENTS.md").is_file(),
            COVERED_BY_GLOBAL_RULEBOOK,
            exists,
        )
    )

    shown = reds if show_all else reds[:40]
    for line in shown:
        print(f"RED  {line}")
    if len(shown) < len(reds):
        print(f"     ... {len(reds) - len(shown)} more (--list)")
    over = {c: n for c, n in crates.items() if n > SIZE_THRESHOLD}
    print(f"subtree rulebooks : {len(books)}")
    print(f"symbols known     : {len(known)} from crates/, sdk/apollia/ and agents/")
    print(f"crates over {SIZE_THRESHOLD:4d}  : {len(over)}, "
          f"{sum(1 for c in over if (REPO_ROOT / 'crates' / c / 'AGENTS.md').is_file())} "
          f"with a rulebook, {len(COVERED_BY_GLOBAL_RULEBOOK)} covered by the global one")
    print(f"invented examples : {len(ILLUSTRATIVE)} exempted by name")
    print(f"claims refuted    : {len(reds)}")
    return 1 if reds else 0


def selftest() -> int:
    failures: list[str] = []

    def case(name: str, condition: bool) -> None:
        if condition:
            print(f"  ok    {name}")
        else:
            print(f"  FAIL  {name}")
            failures.append(name)

    live = {"crates/apollia-oria/src/engine.rs", "docs/agents/TESTING.md"}

    def exists(rel: str) -> bool:
        return rel in live

    # A1, both directions, including the crate-relative resolution.
    case(
        "a dead path is reported",
        dead_paths("crates/apollia-oria/AGENTS.md", "see `src/pipeline.rs`\n", exists) != [],
    )
    case(
        "a live crate-relative path passes",
        dead_paths("crates/apollia-oria/AGENTS.md", "see `src/engine.rs`\n", exists) == [],
    )
    case(
        "a live repo-relative path passes",
        dead_paths("sdk/AGENTS.md", "see `docs/agents/TESTING.md`\n", exists) == [],
    )
    case(
        "a URL is not read as a path",
        dead_paths("sdk/AGENTS.md", "see `https://example.invalid/x.md`\n", exists) == [],
    )
    case(
        "a home-relative data file is not read as a path",
        dead_paths("sdk/AGENTS.md", "opens `~/.apollia/plans.db`\n", exists) == [],
    )
    case(
        "a line reference is stripped before the test",
        dead_paths("crates/apollia-oria/AGENTS.md", "at `src/engine.rs:94`\n", exists) == [],
    )

    # A2, both directions.
    known = {"StepBudget", "ORIAError", "BudgetExceeded"}
    case(
        "an absent symbol is reported",
        absent_symbols("x/AGENTS.md", "emits `RuntimeEvent::TaskBudgetExceeded`\n", known) != [],
    )
    case(
        "a present path-qualified symbol passes",
        absent_symbols("x/AGENTS.md", "returns `ORIAError::BudgetExceeded`\n", known) == [],
    )
    case(
        "a symbol the document declares absent is not reported",
        absent_symbols("x/AGENTS.md", "There is no `AipError` in this crate.\n", known) == [],
    )
    case(
        "a language type is not read as a crate symbol",
        absent_symbols("x/AGENTS.md", "a `Duration` and a `String`\n", known) == [],
    )
    case(
        "a symbol inside a fenced block is read",
        absent_symbols("x/AGENTS.md", "```rust\n`CircuitBreakerConfig`\n```\n", known) != [],
    )

    # A3, every direction the list can go wrong in.
    crates = {"big-uncovered": 900, "big-covered": 900, "big-booked": 900, "small": 100}
    case(
        "a crate over the threshold with neither rulebook nor entry is reported",
        coverage_violations(crates, lambda c: c == "big-booked", {"big-covered": "docs/agents/TESTING.md"}, exists)
        == ["big-uncovered: 900 lines under src/ and no AGENTS.md, named by no "
            "entry of COVERED_BY_GLOBAL_RULEBOOK"],
    )
    case(
        "a crate under the threshold is not asked for anything",
        all(
            "small" not in v
            for v in coverage_violations(
                {"small": 100}, lambda c: False, {}, exists
            )
        ),
    )
    case(
        "an entry naming a file that does not exist is reported",
        any(
            "does not exist" in v
            for v in coverage_violations(
                {"big-covered": 900}, lambda c: False, {"big-covered": "docs/agents/GONE.md"}, exists
            )
        ),
    )
    case(
        "an entry for a crate that now has its own rulebook is reported",
        any(
            "still listed as covered" in v
            for v in coverage_violations(
                {"big-booked": 900}, lambda c: True, {"big-booked": "docs/agents/TESTING.md"}, exists
            )
        ),
    )
    case(
        "an entry for a crate that left the tree is reported",
        any(
            "not a crate of this tree" in v
            for v in coverage_violations({}, lambda c: False, {"gone": "docs/agents/TESTING.md"}, exists)
        ),
    )
    case(
        "an empty list over a tree of covered crates is not a pass",
        coverage_violations({"big-uncovered": 900}, lambda c: False, {}, exists) != [],
    )

    if failures:
        print(f"\nselftest: {len(failures)} case(s) failed", file=sys.stderr)
        return 1
    print("\nselftest: every case holds")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Every claim a subtree AGENTS.md makes about its crate is replayed "
        "against the tree, and every crate over the size threshold is judged."
    )
    parser.add_argument("--list", action="store_true", help="print every red, not the first 40")
    parser.add_argument("--selftest", action="store_true", help="drive the classifiers on fixtures")
    args = parser.parse_args()
    if args.selftest:
        return selftest()
    return measure(args.list)


if __name__ == "__main__":
    sys.exit(main())
