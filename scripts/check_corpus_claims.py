#!/usr/bin/env python3
"""Replay the rulebook's own claims against the tree it describes.

`docs/agents/*.md` and the root `AGENTS.md` are read before every change, and
they were written as targets rather than as descriptions. Nothing ever
confronted them with the tree: `check_claims.py` reads `docs/CLAIMS.toml`,
`check_claim_anchors.py` reads the documentation site, and the link job reads
the four companion files at the root. So the rulebook drifted where no reader
could see it: it sent an agent to a file merged away, named eight types the
code does not carry, and prescribed nine test tools no manifest declares.

Four crossings, each two-sided where the claim is a negative one.

  1. Paths. Every path cited in backticks resolves against the tree. A path
     the corpus states is *absent* is listed in `ABSENT_ON_PURPOSE` and is red
     when it starts existing: a document that says "there is no `tests/visual/`
     suite" is as wrong when the suite lands as it was when the sentence was
     written about a suite that existed.
  2. Symbols. Every CamelCase identifier cited in backticks is a word of the
     code somewhere under `crates/`, `sdk/apollia/` or `agents/`. Foreign and
     illustrative names (`TypedDict`, `UpperCamelCase`, a naming counter-example)
     are named one by one in `FOREIGN_SYMBOLS`, with the reason; a name the
     corpus warns against because nothing defines it goes in
     `ABSENT_SYMBOLS_ON_PURPOSE` and is red the day someone defines it.
  3. Tools. The test and build vocabulary of the corpus is crossed with the
     manifests, the workflows, the justfile and `package.json`. A tool the
     corpus prescribes must have a trace there; a tool the corpus names as
     deliberately absent (`CITED_AS_ABSENT`) must have none.
  4. Line references. A citation of the form `path:NN` or `` `path` line NN ``
     points inside the file it names.

Corpus. The judged files are the root `AGENTS.md` and `docs/agents/*.md`. The
subtree `AGENTS.md` files are discovered the same way but sit in
`AWAITING_PASS` until their own claims are replayed: their defects are printed
on every run, they do not fail the guard, and the day one of them comes back
clean the guard fails asking for its waiver to be removed, so the ratchet
cannot grow in the dark.

Exit codes: 0 when every judged claim holds, 1 when one does not, 2 when
nothing was measured (no corpus file found), which is not a pass.

Usage:
    python3 scripts/check_corpus_claims.py
    python3 scripts/check_corpus_claims.py --selftest
"""

import argparse
import re
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]

BACKTICK = re.compile(r"`([^`\n]+)`")
CAMEL_CASE = re.compile(r"^[A-Z][A-Za-z0-9]*[a-z][A-Za-z0-9]*[A-Z][A-Za-z0-9]*$")
LINE_SUFFIX = re.compile(r":(\d+)(?:-(\d+))?$")
LINE_PROSE = re.compile(r"`([^`\n]+)`\s+line\s+(\d+)")

# A bare filename is judged only when its extension is one this repository
# actually carries in source. `governance.db` and `apollia.sock` are names a
# running process creates, not files a clone holds, and demanding them would
# make the guard red on a correct sentence.
SOURCE_EXT = (
    ".md", ".rs", ".py", ".toml", ".sh", ".ts", ".tsx", ".svelte",
    ".json", ".yml", ".yaml", ".css", ".js", ".mjs", ".plist",
)

# Paths the corpus cites as illustrations rather than as files. Each is an
# example of a form, so no tree can satisfy it.
ILLUSTRATIVE_PATHS = {
    "Kebab-Case-Or-Title-Case.md": "NAMING file-naming table, a pattern",
    "Architecture-Vue-Ensemble.md": "NAMING file-naming table, a pattern",
    "DESIGN-SYSTEM.md": "NAMING file-naming table, a pattern",
    "snake_case.py": "NAMING file-naming table, a pattern",
    "snake_case.rs": "NAMING file-naming table, a pattern",
    "kebab-case.sh": "NAMING file-naming table, a pattern",
    "agent_schemas.py": "NAMING file-naming table, a pattern",
    "foo/mod.rs": "RUST-PATTERNS module layout example",
    "AGENTS.local.md": "documented optional override, gitignored, absent from a clone",
    "AGENTS.override.md": "documented optional override, gitignored, absent from a clone",
}

# Paths the corpus states are absent. Asserted absent, so the sentence and the
# tree are checked against each other in both directions.
ABSENT_ON_PURPOSE = {
    "tests/visual/": "TESTING §7 states no visual baseline suite exists",
    "crates/apollia-desktop/ui/tests/visual/": "TESTING §7, same sentence, full path",
    "conftest.py": "TESTING §6 states the SDK has none, so its slow marker selects nothing",
}

# Names the corpus cites that belong to another vocabulary: the Python stdlib,
# a naming counter-example, an operating-system field. None of them is a claim
# about this tree.
FOREIGN_SYMBOLS = {
    "UpperCamelCase": "a naming convention, not a type",
    "StdIn": "NAMING counter-example of an acronym written wrong",
    "EmailTriage": "NAMING example of an agent name",
    "CancelledError": "asyncio, Python standard library",
    "LiteralString": "typing, Python standard library",
    "MemTotal": "a field of /proc/meminfo",
    "JoinSet": "tokio, the API the rule points at rather than a type of this tree",
    "TypedDict": "typing, Python standard library",
}

# Names the corpus states the code does not carry. Asserted absent, so the
# sentence that warns against writing one is red the day someone writes it.
ABSENT_SYMBOLS_ON_PURPOSE = {
    "AgentTimeoutError": "PYTHON-PATTERNS §4 states nothing defines it",
}

# The test and build vocabulary, and the search that proves each tool is part
# of this tree. The haystack is the manifests, the workflows, the justfile and
# the desktop package manifest: the places a tool has to appear to be run.
TOOL_TRACES = {
    "nextest": r"nextest",
    "insta": r"\binsta\b",
    "mockall": r"\bmockall\b",
    "pretty_assertions": r"\bpretty_assertions\b",
    "assert_cmd": r"\bassert_cmd\b",
    "axum-test": r"\baxum-test\b",
    "wiremock": r"\bwiremock\b",
    "proptest": r"\bproptest\b",
    "criterion": r"\bcriterion\b",
    "serial_test": r"\bserial_test\b",
    "quickcheck": r"\bquickcheck\b",
    "hypothesis": r"\bhypothesis\b",
    "syrupy": r"\bsyrupy\b",
    "respx": r"\brespx\b",
    "pytest-benchmark": r"pytest-benchmark",
    "atheris": r"\batheris\b",
    "hyperfine": r"\bhyperfine\b",
    "Codecov": r"(?i)codecov",
    "pnpm": r"\bpnpm\b",
    "tauri-driver": r"tauri-driver",
    "requests_mock": r"requests_mock",
    "cargo-mutants": r"cargo-mutants|cargo mutants",
    "llvm-cov": r"llvm-cov",
    "cargo-fuzz": r"cargo-fuzz|cargo \+nightly fuzz",
}

TOOL_HAYSTACK = (
    "*.toml",
    ".github/workflows/*.yml",
    "justfile",
    "crates/apollia-desktop/ui/package.json",
)

# Tools the corpus names in order to say they are not used here. Asserted
# absent from the haystack, for the same reason as ABSENT_ON_PURPOSE.
CITED_AS_ABSENT = {
    "quickcheck": "TESTING §8 prefers proptest over it",
    "tauri-driver": "TESTING §7 states there is no tauri-driver setup",
    "requests_mock": "TESTING §6 forbids it, sync-only",
    "nextest": "TESTING §2 states the suite runs under cargo test, not nextest",
    "Codecov": "TESTING §11 states coverage is gated in CI, not uploaded",
}

# Corpus files whose claims have not been replayed yet. Their defects are
# printed and do not fail the run. A file that comes back clean fails the run
# until its waiver is removed.
AWAITING_PASS = {
    "crates/apollia-aip/AGENTS.md": "subtree rulebook, claims not replayed yet",
    "crates/apollia-desktop/ui/AGENTS.md": "subtree rulebook, claims not replayed yet",
    "crates/apollia-oria/AGENTS.md": "subtree rulebook, claims not replayed yet",
    "crates/apollia-runtime/AGENTS.md": "subtree rulebook, claims not replayed yet",
}


def _git(*args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["git", *args], cwd=REPO_ROOT, capture_output=True, text=True, check=False
    )


def tracked_files() -> list[str]:
    out = _git("ls-files")
    return out.stdout.splitlines() if out.returncode == 0 else []


def corpus_paths(tracked: list[str]) -> list[str]:
    """The rulebook: the root AGENTS.md, docs/agents/, every subtree AGENTS.md."""
    return sorted(
        rel
        for rel in tracked
        if rel == "AGENTS.md"
        or rel.startswith("docs/agents/")
        and rel.endswith(".md")
        or rel.endswith("/AGENTS.md")
    )


# ── Rule 1: a cited path resolves ────────────────────────────────────────────


def path_candidates(text: str) -> list[tuple[int, str]]:
    """Every backtick segment this rule agrees to judge, with its line."""
    found = []
    for n, line in enumerate(text.splitlines(), 1):
        for raw in BACKTICK.findall(line):
            seg = raw.strip()
            if any(c in seg for c in " $<>=()|"):
                continue
            if seg.startswith(("-", "#", "/", "~", "http", "..")):
                continue
            if "/" in seg:
                found.append((n, seg))
            elif seg.endswith(SOURCE_EXT) and not seg.startswith("."):
                found.append((n, seg))
    return found


def _basename_index(tracked: list[str]) -> dict[str, list[str]]:
    index: dict[str, list[str]] = {}
    for rel in tracked:
        index.setdefault(rel.rsplit("/", 1)[-1], []).append(rel)
    return index


def _judged_path(seg: str, top_level: set[str], basenames: dict[str, list[str]]) -> bool:
    """A candidate is judged when it claims to name a file of this repository.

    Either it is rooted at a top-level entry, or its basename is one git
    knows, which is what makes `Integrations.svelte` under the wrong directory
    a defect rather than a resolution.
    """
    bare = LINE_SUFFIX.sub("", seg).rstrip("/")
    if not bare:
        return False
    if "/" in bare:
        return bare.split("/", 1)[0] in top_level or bare.rsplit("/", 1)[-1] in basenames
    if "*" in bare or "{" in bare:
        return False
    return bare in basenames or bare.endswith(SOURCE_EXT)


def _is_ignored(bare: str) -> bool:
    """A path `.gitignore` covers is absent from a clone, so it is not judged.

    The corpus names several on purpose (`docs/internal/`, the generated CLI
    report), and demanding them would make the guard red on a sentence whose
    whole point is that the path is not committed.
    """
    probe = bare if "." in bare.rsplit("/", 1)[-1] else bare.rstrip("/") + "/"
    return _git("check-ignore", "-q", probe).returncode == 0


def _resolves(
    seg: str, tracked_set: set[str], basenames: dict[str, list[str]], citing_dir: str = ""
) -> bool:
    bare = LINE_SUFFIX.sub("", seg).rstrip("/")
    # A crate's own AGENTS.md cites `src/foo.rs`, meaning its own src/.
    roots = [""] + ([citing_dir] if citing_dir else [])
    for root in roots:
        candidate = f"{root}/{bare}" if root else bare
        if candidate in tracked_set:
            return True
        if (REPO_ROOT / candidate).exists():
            return True
        if "*" in candidate or "{" in candidate:
            if any(any(REPO_ROOT.glob(p)) for p in _brace_free(candidate)):
                return True
        prefix = candidate.rstrip("/") + "/"
        if any(rel.startswith(prefix) for rel in tracked_set):
            return True
    if "/" not in bare and bare in basenames:
        return True
    return _is_ignored(bare)


def _brace_free(pattern: str) -> list[str]:
    out = [pattern]
    while any("{" in p for p in out):
        grown = []
        for p in out:
            m = re.search(r"\{([^}]*)\}", p)
            if not m:
                grown.append(p)
                continue
            for alt in m.group(1).split(","):
                grown.append(p[: m.start()] + alt + p[m.end() :])
        out = grown
    return out


def dead_paths(
    corpus: dict[str, str], tracked: list[str]
) -> tuple[list[str], list[str]]:
    """Cited paths that do not resolve, and absent-on-purpose paths that do."""
    tracked_set = set(tracked)
    basenames = _basename_index(tracked)
    top_level = {rel.split("/", 1)[0] for rel in tracked}
    dead = []
    for rel, text in sorted(corpus.items()):
        citing_dir = rel.rsplit("/", 1)[0] if "/" in rel else ""
        for n, seg in path_candidates(text):
            bare = LINE_SUFFIX.sub("", seg)
            if bare in ILLUSTRATIVE_PATHS or bare.rstrip("/") in ILLUSTRATIVE_PATHS:
                continue
            if bare in ABSENT_ON_PURPOSE:
                continue
            if not _judged_path(seg, top_level, basenames):
                continue
            if not _resolves(seg, tracked_set, basenames, citing_dir):
                dead.append(f"{rel}:{n}  {seg}  no such path in the tree")
    resurrected = [
        f"{path}  exists, but the corpus states it does not ({reason})"
        for path, reason in sorted(ABSENT_ON_PURPOSE.items())
        if _resolves(path, tracked_set, basenames)
    ]
    return dead, resurrected


# ── Rule 2: a cited symbol is a word of the code ─────────────────────────────


def symbol_candidates(text: str) -> list[tuple[int, str]]:
    found = []
    for n, line in enumerate(text.splitlines(), 1):
        for raw in BACKTICK.findall(line):
            seg = raw.strip()
            if CAMEL_CASE.match(seg):
                found.append((n, seg))
    return found


# Markdown is excluded from the haystack on purpose: a crate's own AGENTS.md
# lives under `crates/`, so a search that reads it finds every name the
# document invented and reports the document as its own evidence.
CODE_PATHSPECS = (
    ":(glob)crates/**/*.rs",
    ":(glob)crates/**/*.ts",
    ":(glob)crates/**/*.svelte",
    ":(glob)crates/**/*.toml",
    ":(glob)sdk/apollia/**/*.py",
    ":(glob)agents/**/*.py",
    ":(glob)agents/**/*.toml",
)


def _word_in_code(name: str) -> bool:
    return _git("grep", "-lw", name, "--", *CODE_PATHSPECS).returncode == 0


def absent_symbols(corpus: dict[str, str]) -> list[str]:
    sites: dict[str, list[str]] = {}
    for rel, text in sorted(corpus.items()):
        for n, name in symbol_candidates(text):
            if name in FOREIGN_SYMBOLS or name in ABSENT_SYMBOLS_ON_PURPOSE:
                continue
            sites.setdefault(name, []).append(f"{rel}:{n}")
    offenses = [
        f"{sites[name][0]}  `{name}`  no such word under crates/, sdk/apollia/ or agents/"
        + (f" (+{len(sites[name]) - 1} more citation(s))" if len(sites[name]) > 1 else "")
        for name in sorted(sites)
        if not _word_in_code(name)
    ]
    offenses += [
        f"`{name}` is a word of the code, but the corpus states it is not ({reason})"
        for name, reason in sorted(ABSENT_SYMBOLS_ON_PURPOSE.items())
        if _word_in_code(name)
    ]
    return offenses


# ── Rule 3: a prescribed tool has a trace, an excluded one has none ──────────


def haystack_text() -> str:
    out = _git("ls-files", "--", *TOOL_HAYSTACK)
    if out.returncode != 0:
        return ""
    chunks = []
    for rel in out.stdout.splitlines():
        try:
            chunks.append((REPO_ROOT / rel).read_text(encoding="utf-8", errors="replace"))
        except OSError:
            continue
    return "\n".join(chunks)


def tool_offenses(corpus: dict[str, str], haystack: str) -> list[str]:
    joined = {rel: text for rel, text in corpus.items()}
    offenses = []
    for tool, trace in sorted(TOOL_TRACES.items()):
        present = re.search(trace, haystack) is not None
        cited = []
        for rel, text in sorted(joined.items()):
            for n, line in enumerate(text.splitlines(), 1):
                if re.search(rf"`[^`\n]*\b{re.escape(tool)}\b[^`\n]*`", line):
                    cited.append(f"{rel}:{n}")
                    break
        if tool in CITED_AS_ABSENT:
            if present:
                offenses.append(
                    f"`{tool}` has a trace in the manifests, but the corpus names it "
                    f"as absent ({CITED_AS_ABSENT[tool]})"
                )
            continue
        if cited and not present:
            offenses.append(
                f"{cited[0]}  `{tool}` is prescribed by the corpus and appears in no "
                f"manifest, workflow, justfile or package.json"
                + (f" (+{len(cited) - 1} more file(s))" if len(cited) > 1 else "")
            )
    return offenses


# ── Rule 4: a line reference points inside the file ──────────────────────────


def _line_count(rel: str) -> int | None:
    target = REPO_ROOT / rel
    if not target.is_file():
        return None
    try:
        return len(target.read_text(encoding="utf-8", errors="replace").splitlines())
    except OSError:
        return None


def _resolve_for_lines(seg: str, basenames: dict[str, list[str]]) -> str | None:
    if (REPO_ROOT / seg).is_file():
        return seg
    if "/" not in seg and len(basenames.get(seg, [])) == 1:
        return basenames[seg][0]
    return None


def stale_line_refs(corpus: dict[str, str], tracked: list[str]) -> list[str]:
    basenames = _basename_index(tracked)
    offenses = []
    for rel, text in sorted(corpus.items()):
        for n, line in enumerate(text.splitlines(), 1):
            refs = []
            for raw in BACKTICK.findall(line):
                seg = raw.strip()
                m = LINE_SUFFIX.search(seg)
                if m:
                    refs.append((seg[: m.start()], int(m.group(2) or m.group(1))))
            for path, num in LINE_PROSE.findall(line):
                refs.append((path.strip(), int(num)))
            for path, num in refs:
                target = _resolve_for_lines(path, basenames)
                if target is None:
                    continue
                total = _line_count(target)
                if total is not None and num > total:
                    offenses.append(
                        f"{rel}:{n}  `{path}` line {num}, but {target} has {total} lines"
                    )
    return offenses


# ── Driver ───────────────────────────────────────────────────────────────────


def measure(corpus: dict[str, str], tracked: list[str]) -> list[str]:
    """Every offense the four rules find on the given corpus, one line each."""
    dead, resurrected = dead_paths(corpus, tracked)
    return (
        dead
        + resurrected
        + absent_symbols(corpus)
        + tool_offenses(corpus, haystack_text())
        + stale_line_refs(corpus, tracked)
    )


def _read(rel: str) -> str:
    return (REPO_ROOT / rel).read_text(encoding="utf-8", errors="replace")


def selftest() -> int:
    """Drive the four rules on a fabricated corpus, red side then clean side."""
    tracked = tracked_files()
    if not tracked:
        print("check_corpus_claims: NOTHING MEASURED, git ls-files is empty", file=sys.stderr)
        return 2

    dirty = {
        "fixture/AGENTS.md": "\n".join(
            [
                "Read `docs/agents/A-FILE-THAT-NEVER-EXISTED.md` first.",
                "The engine is `NoSuchEngineNameHere`.",
                "Run the suite with `cargo insta review`.",
                "See `AGENTS.md:99999` for the rest.",
            ]
        )
    }
    clean = {
        "fixture/AGENTS.md": "\n".join(
            [
                "Read `docs/agents/FORBIDDEN.md` first.",
                "The engine is `ORIAEngine`.",
                "Run the suite with `cargo test --workspace --no-fail-fast`.",
                "See `AGENTS.md:1` for the rest.",
            ]
        )
    }

    failures = []
    dead, _ = dead_paths(dirty, tracked)
    if not any("A-FILE-THAT-NEVER-EXISTED" in line for line in dead):
        failures.append("rule 1 did not report a path that does not exist")
    if absent_symbols(dirty) == []:
        failures.append("rule 2 did not report a CamelCase name absent from the code")
    if not any("insta" in line for line in tool_offenses(dirty, haystack_text())):
        failures.append("rule 3 did not report a prescribed tool with no manifest trace")
    if not stale_line_refs(dirty, tracked):
        failures.append("rule 4 did not report a line reference past the end of a file")

    left = measure(clean, tracked)
    if left:
        failures.append(f"the clean fixture was reported red: {left!r}")

    # The absent-on-purpose direction, driven on a path this tree does carry.
    saved = dict(ABSENT_ON_PURPOSE)
    ABSENT_ON_PURPOSE.clear()
    ABSENT_ON_PURPOSE["AGENTS.md"] = "fabricated: a file that does exist"
    _, resurrected = dead_paths(clean, tracked)
    ABSENT_ON_PURPOSE.clear()
    ABSENT_ON_PURPOSE.update(saved)
    if not resurrected:
        failures.append("rule 1 did not report an absent-on-purpose path that exists")

    for line in failures:
        print(f"  KO  {line}", file=sys.stderr)
    if failures:
        print(f"check_corpus_claims: selftest FAILED, {len(failures)} case(s)", file=sys.stderr)
        return 1
    print("check_corpus_claims: selftest, the four rules fire and the clean side passes")
    return 0


def main() -> int:
    tracked = tracked_files()
    corpus = corpus_paths(tracked)
    if not corpus:
        print(
            "check_corpus_claims: NOTHING MEASURED, no rulebook file found",
            file=sys.stderr,
        )
        return 2

    judged = {rel: _read(rel) for rel in corpus if rel not in AWAITING_PASS}
    waived = {rel: _read(rel) for rel in corpus if rel in AWAITING_PASS}
    if not judged:
        print(
            "check_corpus_claims: NOTHING MEASURED, every corpus file is waived",
            file=sys.stderr,
        )
        return 2

    offenses = measure(judged, tracked)
    print(
        f"check_corpus_claims: {len(judged)} judged file(s), {len(waived)} waived, "
        f"{sum(len(t.splitlines()) for t in judged.values())} lines read"
    )

    waived_clean = []
    for rel, text in sorted(waived.items()):
        found = measure({rel: text}, tracked)
        if found:
            print(f"  waived {rel}: {len(found)} claim(s) not replayed yet")
            for line in found:
                print(f"      {line}")
        else:
            waived_clean.append(rel)

    if waived_clean:
        offenses += [
            f"{rel} carries no unreplayed claim: remove its AWAITING_PASS entry, "
            f"a waiver that outlives its condition is the ratchet growing in the dark"
            for rel in waived_clean
        ]

    if offenses:
        print(f"\n{len(offenses)} claim(s) the tree does not back:", file=sys.stderr)
        for line in offenses:
            print(f"  {line}", file=sys.stderr)
        return 1
    print("check_corpus_claims: every judged claim is backed by the tree")
    return 0


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--selftest",
        action="store_true",
        help="drive the four rules on a fabricated corpus, then exit",
    )
    args = parser.parse_args()
    sys.exit(selftest() if args.selftest else main())
