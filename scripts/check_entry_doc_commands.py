#!/usr/bin/env python3
"""Replay every `apollia-os` invocation the entry documents show, against the binary.

The four documents a stranger reads first, the root README, the examples README
and the companion agent's knowledge base, are the only place in this tree where
a command line is published and nothing ever runs it. Five of them were wrong at
once: a section argument `config show` does not take, a `trigger create` shown as
interactive when three arguments are required, a `task run` subcommand that does
not exist, a directory install path for an agent that installs by file, and a
flag documented as working that refuses.

The check is a parse verdict, not a functional one. Each invocation's verb path
is resolved with `--help`, then the full argument line is offered to clap and
only its refusal is read. A runtime failure, no daemon listening or a path that
does not exist, is not a documentation defect and does not fail this guard; a
parse refusal is.

The documentation site is a second corpus, and it was the one nobody replayed:
four published command lines were refused at once (`mcp add` with a positional
URL, `memory import` with a positional namespace, a `--follow` the binary
rejects, and a bare `apollia` binary name that does not exist). Every tracked
page under `docs/site` is therefore read too, fenced blocks and inline code
spans alike, and judged the same way with two differences:

  * site invocations are offered to clap with `--help` appended and never
    executed: 1000+ lines, some destructive, and the entry documents already
    exercise the execution path.
  * synopsis placeholders are replaced by a value before the parse. A bare
    `<name>` becomes `1`; placeholders whose argument only accepts an
    enumerated value are named in `TYPED_PLACEHOLDERS` with a value the enum
    accepts, because `1` would make the guard red on the generated reference,
    which is correct.

A line whose binary is written bare `apollia` is a defect of the same family:
no such binary ships, so the published line fails for every reader who pastes
it.

Two safeguards, both required and neither optional:

  * `HOME` is redirected to a throwaway directory for the whole run, so a
    command that writes state never reaches the real `~/.apollia`.
  * long-running and destructive verbs are resolved but never run.

Verdict by exit code, since the caller reads it rather than the text:

  0  every documented invocation is accepted by the binary
  1  at least one is refused
  2  nothing was measured, so the run says nothing: the binary is absent, or no
     document holds an invocation. Build it with `cargo build -p apollia-cli`.
"""

import argparse
import os
import re
import shlex
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BINARY = ROOT / "target/debug/apollia-os"

DOCUMENTS = ["README.md", "agents/examples/README.md"]
KNOWLEDGE = ROOT / "agents/system/apollia-guide/knowledge"
SITE = "docs/site"

# Resolved but never run: these either never return or change the machine.
NEVER_RUN = ("start", "stop", "serve", "chat", "shell", "update", "onboard")

# Placeholders whose argument only accepts an enumerated value, with a value
# the enum accepts. Everything else in `<...>` is replaced by `1`, which any
# free-form string, path or integer argument parses.
TYPED_PLACEHOLDERS = {
    "<SHELL>": "bash",  # completions <SHELL>
    "<TYPE>": "desktop",  # notify create --kind <TYPE>
}

# The Python SDK installs its own `apollia` console script
# (`sdk/pyproject.toml`, `[project.scripts]`), whose verbs are these. A site
# line citing them under the bare name is the SDK's CLI, not a wrong spelling
# of `apollia-os`.
SDK_CLI_VERBS = ("new", "inspect")

BLOCK = re.compile(r"```(?:bash|sh|shell|console)\n(.*?)```", re.S)
ANY_FENCE = re.compile(r"```.*?```", re.S)
INLINE_SPAN = re.compile(r"`(apollia(?:-os)? [^`\n]+)`")
CONTINUATION = re.compile(r"\\\n\s*")
PARSE_REFUSAL = re.compile(
    r"error: (unexpected argument|unrecognized|invalid value|the following required)"
)


def documents() -> list[Path]:
    paths = [ROOT / d for d in DOCUMENTS]
    if KNOWLEDGE.is_dir():
        paths += sorted(KNOWLEDGE.glob("*.md"))
    return [p for p in paths if p.exists()]


def invocations(path: Path):
    for block in BLOCK.findall(path.read_text()):
        # A trailing backslash means one invocation spans several source lines.
        # Reading them apart reports a defect the document does not carry.
        for line in CONTINUATION.sub(" ", block).splitlines():
            line = line.strip()
            if line.startswith("apollia-os"):
                yield line


def run(argv: list[str], env: dict) -> subprocess.CompletedProcess:
    return subprocess.run(
        [str(BINARY)] + argv, capture_output=True, text=True, timeout=30, env=env, cwd=ROOT
    )


def site_pages() -> list[Path]:
    """Return the tracked Markdown pages of the documentation site."""
    result = subprocess.run(
        ["git", "ls-files", "-z", "--", f"{SITE}/**/*.md", f"{SITE}/**/*.mdx"],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        return []
    return sorted(ROOT / p for p in result.stdout.split("\0") if p)


def site_invocations(path: Path):
    """Yield every `apollia(-os)` line a site page shows, blocks then spans."""
    text = path.read_text(encoding="utf-8", errors="replace")
    for block in BLOCK.findall(text):
        for line in CONTINUATION.sub(" ", block).splitlines():
            line = line.strip()
            line = re.sub(r"^\$\s+", "", line)
            if re.match(r"apollia(-os)?(\s|$)", line):
                yield line
    for span in INLINE_SPAN.finditer(ANY_FENCE.sub("", text)):
        yield span.group(1).strip()


def parse_argv(line: str) -> list[str] | None:
    """Turn a documented line into an argv clap can judge, or None to skip.

    Pipes, redirections and chained commands end the argv: what follows is
    the shell's business. Optional synopsis placeholders are dropped, and a
    `<COMMAND>` group synopsis is not a judgeable line.
    """
    try:
        tokens = shlex.split(line, comments=True)[1:]
    except ValueError:
        return None
    argv: list[str] = []
    for token in tokens:
        if token in ("|", "||", "&&", ";", ">", ">>", "2>", "<") or token.startswith("$("):
            break
        if (token.startswith("[") and token.endswith("]")) or token == "...":
            continue
        if token == "<COMMAND>":
            return None
        if token in TYPED_PLACEHOLDERS:
            token = TYPED_PLACEHOLDERS[token]
        elif re.fullmatch(r"<[^<>]+>", token):
            token = "1"
        elif token.startswith("--") and "=<" in token:
            token = token.split("=")[0] + "=1"
        argv.append(token)
    return argv


def site_refusals(env: dict) -> tuple[int, list[tuple[str, str, str]]]:
    """Parse-check the site corpus. Returns (invocations read, refusals)."""
    refused: list[tuple[str, str, str]] = []
    seen: set[tuple[str, ...]] = set()
    count = 0
    for page in site_pages():
        rel = page.relative_to(ROOT).as_posix()
        for line in site_invocations(page):
            count += 1
            if re.match(r"apollia(\s|$)", line):
                first_verb = line.split()[1] if len(line.split()) > 1 else ""
                if first_verb not in SDK_CLI_VERBS:
                    refused.append(
                        (
                            rel,
                            line,
                            "bare `apollia` is the SDK's scaffolding CLI "
                            "(verbs: " + ", ".join(SDK_CLI_VERBS) + "); the "
                            "runtime binary is `apollia-os`",
                        )
                    )
                continue
            argv = parse_argv(line)
            if argv is None:
                continue
            key = tuple(argv)
            if key in seen:
                continue
            seen.add(key)
            verdict = run(argv + ["--help"], env)
            if verdict.returncode != 0:
                first = (verdict.stderr or verdict.stdout).strip().splitlines()
                refused.append((rel, line, first[0] if first else "no output"))
    return count, refused


def main() -> int:
    if not BINARY.exists():
        print(
            f"NOTHING MEASURED: {BINARY.relative_to(ROOT)} is absent, so no invocation\n"
            "                 was offered to anything. Run: cargo build -p apollia-cli",
            file=sys.stderr,
        )
        return 2

    found = [(p, line) for p in documents() for line in invocations(p)]
    if not found:
        print("NOTHING MEASURED: no apollia-os invocation in the entry documents", file=sys.stderr)
        return 2

    with tempfile.TemporaryDirectory() as home:
        env = dict(os.environ, HOME=home)
        refused = []
        for path, line in found:
            rel = path.relative_to(ROOT).as_posix()
            try:
                argv = shlex.split(line, comments=True)[1:]
            except ValueError:
                refused.append((rel, line, "the shell line does not tokenise"))
                continue

            verbs = []
            for token in argv:
                if token.startswith("-"):
                    break
                verbs.append(token)

            help_run = run(verbs + ["--help"], env)
            if help_run.returncode != 0:
                first = (help_run.stderr or help_run.stdout).strip().splitlines()
                refused.append((rel, line, first[0] if first else "no such command"))
                continue

            if verbs and verbs[0] in NEVER_RUN:
                continue
            try:
                result = run(argv, env)
            except subprocess.TimeoutExpired:
                continue
            output = (result.stderr or "") + (result.stdout or "")
            # Usage refusals exit 1 since the CLI aligned on the published
            # contract (1 = usage, 2 = runtime); 2 is kept so a stale binary
            # still reports its refusals instead of passing them.
            if result.returncode in (1, 2) and PARSE_REFUSAL.search(output):
                refused.append((rel, line, output.strip().splitlines()[0]))

        site_count, site_refused = site_refusals(env)

    print(f"documents read        : {len(documents())}")
    print(f"invocations replayed  : {len(found)}")
    print(f"refused by the binary : {len(refused)}")
    print(f"site invocations read : {site_count} (parse verdict only)")
    print(f"site lines refused    : {len(site_refused)}")

    refused += site_refused
    if refused:
        print()
        for rel, line, detail in refused:
            print(f"  REFUSED  {rel}")
            print(f"           {line}")
            print(f"           -> {detail}")
        print()
        print("A published command line the binary refuses is the first thing a")
        print("stranger runs, and the first thing that tells them the tree is stale.")
        return 1

    print()
    print("OK: every invocation the entry documents and the site publish is accepted")
    return 0


if __name__ == "__main__":
    argparse.ArgumentParser(description=__doc__.splitlines()[0]).parse_args()
    sys.exit(main())
