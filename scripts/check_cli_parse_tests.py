#!/usr/bin/env python3
"""Every CLI leaf command has at least one argument-parsing test.

`crates/apollia-cli/AGENTS.md` section 4 states that every sub-command has
parsing tests, and pins a target of 150+ of them. The target was the only thing
measured: the crate carried 247 `parse_from` sequences while 61 of the 199
leaves the binary publishes had none, so a leaf could lose its path, rename a
flag or change an argument arity without a single test noticing.

This guard measures the leaves, not the total. The inventory is not a
checked-in list: it is enumerated from the built binary by walking `--help`
recursively under a throwaway HOME, so a new sub-command joins the floor the
day it is merged. The sequences are read from the crate sources and resolved
against that same tree:

  - a sequence is the list of string literals inside a `parse_from([...])`,
    `try_parse_from([...])` or `parse(&[...])` call; comment lines are stripped
    first, so a commented-out call counts for nothing;
  - argv[0] is dropped, since `parse_from` consumes the program name;
  - the remaining tokens are walked down the command tree, a flag (and the
    value that follows it, when that value names no child) is skipped, and the
    walk stops at the first leaf or the first token that names no child.

Where the walk starts depends on the file. A test module that wraps a noun's
sub-command enum (the `TestCli` pattern used across `commands/`) parses a path
relative to that noun, and one that wraps the whole command enum parses the
noun itself, so both starts are tried: the file's own node, then the root. A
leaf is credited only when it sits under the file's node, so a sequence read in
`project.rs` can never credit a leaf of `mcp`. The file's node comes from its
stem, or from MODULE_NODE for the renamed and the nested ones; every entry of
that table is checked against the tree, since an entry naming a node the binary
no longer publishes would silently drop that file's sequences.

Verdict by exit code, since the caller reads it rather than the text:

  0  every leaf carries at least one parsing sequence
  1  at least one leaf carries none, or MODULE_NODE names a node that is gone
  2  nothing was measured: the binary is absent (build it with
     `cargo build -p apollia-cli`), the tree walk produced no leaf, or the
     crate sources hold no parsing sequence at all

`--selftest` exercises the reader and the resolver on fixtures, in both
directions: a commented-out call, a sequence naming another noun's leaf and a
leaf nobody parses must be reported, and a real call, a flag-prefixed path, a
nested path and a path that repeats its own noun must be credited.

Usage:
    python3 scripts/check_cli_parse_tests.py [--bin PATH] [--crate DIR]
    python3 scripts/check_cli_parse_tests.py --selftest
"""

import argparse
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_BIN = REPO_ROOT / "target/debug/apollia-os"
DEFAULT_CRATE = REPO_ROOT / "crates/apollia-cli/src"

# A parsing call and the bracketed argv it is given.
SEQUENCE = re.compile(r"(?:try_parse_from|parse_from|parse)\(\s*&?\[([^\]]*)\]", re.S)
LITERAL = re.compile(r'"((?:[^"\\]|\\.)*)"')
COMMENT = re.compile(r"^\s*//")

# Files whose tests parse a path relative to a node other than the root. The
# stem alone answers for most of them (`mcp.rs` reads `mcp`), so only the
# renamed and the nested ones are named here; every entry is checked against
# the published tree before it is used.
MODULE_NODE = {
    "commands/agent/mod.rs": "agent",
    "commands/agent/tests.rs": "agent",
    "commands/chat_config.rs": "chat config",
    "commands/chat_stream.rs": "chat",
    "commands/do_cmd.rs": "do",
    "commands/mcp_oauth.rs": "mcp oauth",
    "commands/mcp_server.rs": "mcp server",
    "commands/plan_cache.rs": "plan cache",
    "commands/user_memory.rs": "profile",
}

# Files whose sequences drive the whole binary, so their walk starts at the root.
ROOT_MODULES = {"main.rs", "parse_tests.rs"}


def help_text(bin_path: str, path: list[str], env: dict) -> str:
    try:
        out = subprocess.run(
            [bin_path, *path, "--help"],
            capture_output=True,
            text=True,
            timeout=20,
            env=env,
        )
    except (subprocess.SubprocessError, OSError):
        return ""
    return (out.stdout or "") + (out.stderr or "")


def subcommands(text: str) -> list[str]:
    lines = text.splitlines()
    try:
        start = next(i for i, ln in enumerate(lines) if ln.strip() == "Commands:")
    except StopIteration:
        return []
    subs = []
    for ln in lines[start + 1 :]:
        if not ln.strip():
            break
        m = re.match(r"^  (\S+)", ln)
        if m and m.group(1) != "help":
            subs.append(m.group(1))
    return subs


def command_tree(bin_path: str, env: dict) -> dict[tuple[str, ...], list[str]]:
    """Every node of the published tree, mapped to its children."""
    tree: dict[tuple[str, ...], list[str]] = {(): subcommands(help_text(bin_path, [], env))}
    stack = [(c,) for c in tree[()]]
    while stack:
        path = stack.pop()
        subs = subcommands(help_text(bin_path, list(path), env))
        tree[path] = subs
        stack.extend(path + (s,) for s in subs)
    return tree


def leaves_of(tree: dict[tuple[str, ...], list[str]]) -> list[tuple[str, ...]]:
    return sorted(path for path, subs in tree.items() if path and not subs)


def sequences_in_source(text: str) -> list[list[str]]:
    """Argv literals of every parsing call, comment lines removed first."""
    body = "\n".join(ln for ln in text.splitlines() if not COMMENT.match(ln))
    out = []
    for m in SEQUENCE.finditer(body):
        tokens = LITERAL.findall(m.group(1))
        if tokens:
            out.append(tokens[1:])  # argv[0] is the program name clap consumes
    return out


def resolve(
    tokens: list[str], tree: dict[tuple[str, ...], list[str]], start: tuple[str, ...]
) -> tuple[str, ...] | None:
    """The leaf a sequence drives, or None when it drives no leaf."""
    if start not in tree:
        return None
    node = start
    i = 0
    while tree.get(node):
        if i >= len(tokens):
            return None
        token = tokens[i]
        if token.startswith("-"):
            i += 1
            if i < len(tokens) and tokens[i] not in tree[node]:
                i += 1  # the flag's value, which names no child
            continue
        if token not in tree[node]:
            return None
        node = node + (token,)
        i += 1
    return node if node != start else None


def resolve_in_module(
    tokens: list[str], tree: dict[tuple[str, ...], list[str]], start: tuple[str, ...]
) -> tuple[str, ...] | None:
    """The leaf a sequence drives, read under its file's node then under the root.

    Both wrapper styles live in this crate: one parses the path below the noun
    (`["x", "list"]` in `project.rs`), the other repeats the noun itself
    (`["x", "logs"]` in `logs.rs`). The second start is bounded by the first, so
    a sequence can only ever credit a leaf under its own file's node.
    """
    for candidate in (start, ()) if start else ((),):
        leaf = resolve(tokens, tree, candidate)
        if leaf is not None and leaf[: len(start)] == start:
            return leaf
    return None


def module_node(rel: str, tree: dict[tuple[str, ...], list[str]]) -> tuple[str, ...] | None:
    """The node a file's sequences are read under, or None when it has none."""
    if rel in ROOT_MODULES:
        return ()
    if rel in MODULE_NODE:
        return tuple(MODULE_NODE[rel].split())
    stem = rel.rsplit("/", 1)[-1][: -len(".rs")]
    for candidate in (
        stem,
        stem.replace("_", "-"),
        *([stem.replace("_", " ")] if "_" in stem else ()),
    ):
        node = tuple(candidate.split())
        if node in tree:
            return node
    return None


def measure(bin_path: str, crate_dir: str) -> int:
    if not Path(bin_path).is_file():
        print(f"cli-parse-tests: no binary at {bin_path}", file=sys.stderr)
        print("cli-parse-tests: build it with `cargo build -p apollia-cli`", file=sys.stderr)
        return 2

    home = tempfile.mkdtemp(prefix="apollia-parse-tests-")
    env = dict(os.environ, HOME=home, NO_COLOR="1")
    tree = command_tree(bin_path, env)
    leaves = leaves_of(tree)
    if not leaves:
        print("cli-parse-tests: the tree walk produced no leaf", file=sys.stderr)
        return 2

    stale = sorted(name for name, node in MODULE_NODE.items() if tuple(node.split()) not in tree)
    if stale:
        print(f"cli-parse-tests: {len(stale)} MODULE_NODE entry/entries name a node the")
        print("cli-parse-tests: binary no longer publishes, so those files are read blind:")
        for name in stale:
            print(f"  {name} -> {MODULE_NODE[name]!r}")
        return 1

    sources = sorted(Path(crate_dir).rglob("*.rs"))
    covered: set[tuple[str, ...]] = set()
    total = 0
    unresolved = 0
    for path in sources:
        rel = path.relative_to(crate_dir).as_posix()
        start = module_node(rel, tree)
        for tokens in sequences_in_source(path.read_text(encoding="utf-8")):
            total += 1
            leaf = resolve_in_module(tokens, tree, start) if start is not None else None
            if leaf is None:
                unresolved += 1
            else:
                covered.add(leaf)

    if total == 0:
        print(f"cli-parse-tests: no parsing sequence found under {crate_dir}", file=sys.stderr)
        return 2

    missing = [leaf for leaf in leaves if leaf not in covered]
    print(f"cli-parse-tests: {len(leaves)} leaves, {total} parsing sequences read")
    print(f"cli-parse-tests: {total - unresolved} resolved to a leaf, {unresolved} to none")
    if missing:
        print(f"cli-parse-tests: {len(missing)} leaf/leaves with no parsing test:")
        for leaf in missing:
            print(f"  {' '.join(leaf)}")
        return 1
    print("cli-parse-tests: every leaf carries at least one parsing sequence")
    return 0


# ── Selftest ─────────────────────────────────────────────────────────────────

FIXTURE_TREE: dict[tuple[str, ...], list[str]] = {
    (): ["mcp", "doctor"],
    ("mcp",): ["list", "oauth"],
    ("mcp", "list"): [],
    ("mcp", "oauth"): ["login"],
    ("mcp", "oauth", "login"): [],
    ("doctor",): [],
}

failures: list[str] = []


def case(name: str, ok: bool, why: str = "") -> None:
    print(f"  {'ok  ' if ok else 'FAIL'} {name}")
    if not ok:
        failures.append(name)
        if why:
            print(f"       {why}")


def selftest() -> int:
    print("check_cli_parse_tests selftest")

    # The reader, both directions.
    case(
        "a real parsing call is read",
        sequences_in_source('let c = TestCli::parse_from(["x", "list"]);') == [["list"]],
    )
    case(
        "a commented-out call is not read",
        sequences_in_source('// let c = TestCli::parse_from(["x", "list"]);') == [],
    )
    case(
        "the helper form `parse(&[...])` is read",
        sequences_in_source('let c = parse(&["apollia-os", "doctor"]);') == [["doctor"]],
    )
    case(
        "a call with no literal is not read",
        sequences_in_source("let c = TestCli::parse_from(argv);") == [],
    )

    # The resolver, both directions.
    case(
        "a path read under the root reaches its leaf",
        resolve(["mcp", "list"], FIXTURE_TREE, ()) == ("mcp", "list"),
    )
    case(
        "a path read under its noun reaches the same leaf",
        resolve(["list"], FIXTURE_TREE, ("mcp",)) == ("mcp", "list"),
    )
    case(
        "the same path read under the wrong noun credits nothing",
        resolve(["list"], FIXTURE_TREE, ()) is None,
        "a sequence resolved from the root by accident would credit a leaf nobody parses",
    )
    case(
        "a leading global flag is skipped",
        resolve(["--json", "mcp", "list"], FIXTURE_TREE, ()) == ("mcp", "list"),
    )
    case(
        "a flag with a value is skipped whole",
        resolve(["--socket", "/tmp/s", "mcp", "list"], FIXTURE_TREE, ()) == ("mcp", "list"),
    )
    case(
        "a nested path reaches the deep leaf",
        resolve(["oauth", "login"], FIXTURE_TREE, ("mcp",)) == ("mcp", "oauth", "login"),
    )
    case(
        "a path stopping on an internal node credits nothing",
        resolve(["oauth"], FIXTURE_TREE, ("mcp",)) is None,
        "an internal node is not a leaf, and crediting it would hide the leaves under it",
    )
    case(
        "an unknown verb credits nothing",
        resolve(["bogus"], FIXTURE_TREE, ("mcp",)) is None,
    )
    case(
        "an empty sequence credits nothing",
        resolve([], FIXTURE_TREE, ()) is None,
    )

    # The two wrapper styles, and the bound that keeps the second honest.
    case(
        "a sequence below its noun is credited",
        resolve_in_module(["list"], FIXTURE_TREE, ("mcp",)) == ("mcp", "list"),
    )
    case(
        "a sequence repeating its own noun is credited",
        resolve_in_module(["doctor"], FIXTURE_TREE, ("doctor",)) == ("doctor",),
        "a leaf whose file parses its own arguments through an Args struct "
        "names itself in the argv, and would otherwise be reported uncovered",
    )
    case(
        "a sequence naming another noun's leaf credits nothing",
        resolve_in_module(["mcp", "list"], FIXTURE_TREE, ("doctor",)) is None,
        "the root fallback must stay bounded by the file's own node, or one "
        "file would answer for the whole tree",
    )

    # The module table, both directions.
    case(
        "main.rs is read under the root",
        module_node("main.rs", FIXTURE_TREE) == (),
    )
    case(
        "a command file is read under the node its stem names",
        module_node("commands/mcp.rs", FIXTURE_TREE) == ("mcp",),
    )
    case(
        "an underscored stem is read under its spaced node",
        module_node("commands/mcp_oauth.rs", FIXTURE_TREE) == ("mcp", "oauth"),
    )
    case(
        "a file naming no node is read under none",
        module_node("commands/fuzzy.rs", FIXTURE_TREE) is None,
        "a file with no node must be skipped, not resolved from the root",
    )

    # The whole measure, on a planted tree: red, then green.
    with tempfile.TemporaryDirectory() as tmp:
        crate = Path(tmp) / "src"
        (crate / "commands").mkdir(parents=True)
        (crate / "main.rs").write_text('fn t() { parse(&["apollia-os", "doctor"]); }\n')
        (crate / "commands" / "fuzzy.rs").write_text(
            'fn t() { TestCli::parse_from(["x", "mcp", "list"]); }\n'
        )
        (crate / "commands" / "mcp.rs").write_text("fn t() { }\n")
        planted = _measure_against(FIXTURE_TREE, crate)
        case(
            "positive control: a leaf nobody parses is reported, and a file "
            "naming no node answers for none",
            planted == {("mcp", "list"), ("mcp", "oauth", "login")},
            f"the planted tree reported {planted!r}",
        )
        (crate / "commands" / "mcp.rs").write_text(
            'fn t() { TestCli::parse_from(["x", "list"]); '
            'TestCli::parse_from(["x", "oauth", "login"]); }\n'
        )
        cured = _measure_against(FIXTURE_TREE, crate)
        case(
            "negative control: the same tree with its sequences passes",
            cured == set(),
            f"the cured tree still reported {cured!r}",
        )

    if failures:
        print(f"\nselftest: {len(failures)} case(s) failed", file=sys.stderr)
        return 1
    print("\nselftest: every case holds")
    return 0


def _measure_against(tree: dict[tuple[str, ...], list[str]], crate: Path) -> set:
    """The leaves a fixture crate leaves uncovered, without touching a binary."""
    covered = set()
    for path in sorted(crate.rglob("*.rs")):
        rel = path.relative_to(crate).as_posix()
        start = module_node(rel, tree)
        if start is None:
            continue
        for tokens in sequences_in_source(path.read_text(encoding="utf-8")):
            leaf = resolve_in_module(tokens, tree, start)
            if leaf is not None:
                covered.add(leaf)
    return {leaf for leaf in leaves_of(tree) if leaf not in covered}


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Assert that every CLI leaf command carries a parsing test."
    )
    parser.add_argument("--bin", default=str(DEFAULT_BIN), help="path to the apollia-os binary")
    parser.add_argument("--crate", default=str(DEFAULT_CRATE), help="CLI crate source directory")
    parser.add_argument(
        "--selftest", action="store_true", help="run the reader and the resolver on fixtures"
    )
    args = parser.parse_args()
    if args.selftest:
        return selftest()
    return measure(args.bin, args.crate)


if __name__ == "__main__":
    sys.exit(main())
