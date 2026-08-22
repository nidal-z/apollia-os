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

# Resolved but never run: these either never return or change the machine.
NEVER_RUN = ("start", "stop", "serve", "chat", "shell", "update", "onboard")

BLOCK = re.compile(r"```(?:bash|sh|shell|console)\n(.*?)```", re.S)
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
            if result.returncode == 2 and PARSE_REFUSAL.search(output):
                refused.append((rel, line, output.strip().splitlines()[0]))

    print(f"documents read        : {len(documents())}")
    print(f"invocations replayed  : {len(found)}")
    print(f"refused by the binary : {len(refused)}")

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
    print("OK: every invocation the entry documents publish is accepted by the binary")
    return 0


if __name__ == "__main__":
    sys.exit(main())
