#!/usr/bin/env python3
"""Generate the evaluation-suite schema reference from the Rust types.

The operator help page documented this schema by hand and got three of its four
assertions wrong: `value` where the code reads `equals`, `target` where it reads
`on`, and a `prompt` plus `pass_if` pair where the code takes a single `rubric`.
An operator following that page wrote a suite the parser rejects.

Hand-copying a schema is how that happens, so the tables here are derived. Only
the prose around them is written.

Output is spliced between two markers so the hand-written parts survive
regeneration:

    <!-- BEGIN GENERATED: eval-schema -->
    <!-- END GENERATED: eval-schema -->

Run via `docs/site/regen.sh`, and replayed by the `docs-generated` CI job.

The four types read here are pointers into the tree, and each one used to fail
silently: a missing struct printed a warning and returned an empty table, and a
missing enum returned an empty table without even the warning. The generator
then exited 0 over a page with the schema removed. That is what happened to the
configuration reference when a module split moved `LlmConfig`, so the same
crossing is applied here: `SOURCES` is declared, `declared_sources.require`
proves it against the tree before anything is written, and a type that is no
longer there answers 2 rather than publishing a page without it.
"""

import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import declared_sources  # noqa: E402
from declared_sources import Source  # noqa: E402

REPO_ROOT = Path(__file__).resolve().parents[3]
SOURCE = REPO_ROOT / "crates" / "apollia-eval" / "src" / "suite.rs"
OUT = REPO_ROOT / "docs" / "site" / "docs" / "reference" / "eval-suites.md"

BEGIN = "<!-- BEGIN GENERATED: eval-schema -->"
END = "<!-- END GENERATED: eval-schema -->"

SUITE_RS = "crates/apollia-eval/src/suite.rs"

# The four types this generator publishes, declared once and crossed with the
# tree before the first read. `scripts/check_doc_generators.py` reads the same
# list from the outside.
SOURCES = [
    Source(SUITE_RS, "pub struct EvalSuite", why="the suite table"),
    Source(SUITE_RS, "pub struct EvalTask", why="the task table"),
    Source(SUITE_RS, "pub enum Assertion", why="the assertion table"),
    Source(SUITE_RS, "pub enum OutputChannel", why="the `on` values"),
]


def braced_body(source: str, header: str) -> str | None:
    """The braced body that follows `header`, by brace matching."""
    start = source.find(header)
    if start == -1:
        return None
    open_brace = source.find("{", start)
    if open_brace == -1:
        return None
    depth = 0
    for i in range(open_brace, len(source)):
        if source[i] == "{":
            depth += 1
        elif source[i] == "}":
            depth -= 1
            if depth == 0:
                return source[open_brace + 1 : i]
    return None


def summarize(doc_block: str) -> str:
    lines = []
    for raw in doc_block.splitlines():
        line = raw.strip().removeprefix("///").strip()
        if not line:
            break
        lines.append(line)
    return " ".join(lines).replace("|", "\\|")


FIELD_RE = re.compile(
    r"((?:^[ \t]*///[^\n]*\n)+)"
    r"((?:^[ \t]*#\[[^\n]*\]\n)*)"
    r"^[ \t]*(?:pub )?(\w+):\s*(.+?),\s*$",
    re.MULTILINE,
)

# A variant is a doc-comment, a name, then its own braced field list.
VARIANT_RE = re.compile(
    r"((?:^[ \t]*///[^\n]*\n)+)^[ \t]*(\w+)\s*\{(.*?)^[ \t]*\},",
    re.MULTILINE | re.DOTALL,
)


def struct_table(source: str, name: str, optional_note: dict[str, str]) -> str | None:
    body = braced_body(source, f"pub struct {name}")
    if body is None:
        # `require` proved the declaration line is present, so an unreadable
        # body is a parse defect rather than an absent subject.
        print(f"error: the body of struct {name} does not close", file=sys.stderr)
        return None
    rows = []
    for doc, attrs, field, rust_type in FIELD_RE.findall(body):
        if "serde(skip" in attrs:
            continue
        required = "optional" if ("serde(default" in attrs or rust_type.startswith("Option<")) else "**required**"
        rows.append(f"| `{field}` | `{rust_type.strip()}` | {required} | {summarize(doc)} |")
    if not rows:
        print(f"error: struct {name} yielded no documented field", file=sys.stderr)
        return None
    return "\n".join(["| Key | Type | Required | Meaning |", "| --- | --- | --- | --- |", *rows])


def assertion_table(source: str) -> str | None:
    body = braced_body(source, "pub enum Assertion")
    if body is None:
        print("error: the body of enum Assertion does not close", file=sys.stderr)
        return None
    rows = []
    for doc, variant, fields in VARIANT_RE.findall(body):
        # The serde tag is snake_case of the variant name.
        tag = re.sub(r"(?<!^)(?=[A-Z])", "_", variant).lower()
        names = [f"`{f}`" for _d, _a, f, _t in FIELD_RE.findall(fields)]
        rows.append(f"| `{tag}` | {', '.join(names) or 'none'} | {summarize(doc)} |")
    if not rows:
        print("error: enum Assertion yielded no variant", file=sys.stderr)
        return None
    return "\n".join(["| `type` | Its fields | What it checks |", "| --- | --- | --- |", *rows])


def enum_values(source: str, name: str) -> str | None:
    body = braced_body(source, f"pub enum {name}")
    if body is None:
        print(f"error: the body of enum {name} does not close", file=sys.stderr)
        return None
    out = []
    for doc, variant in re.findall(r"((?:^[ \t]*///[^\n]*\n)+)^[ \t]*(\w+),", body, re.MULTILINE):
        tag = re.sub(r"(?<!^)(?=[A-Z])", "_", variant).lower()
        out.append(f"| `{tag}` | {summarize(doc)} |")
    if not out:
        print(f"error: enum {name} yielded no documented value", file=sys.stderr)
        return None
    return "\n".join(["| Value | Meaning |", "| --- | --- |", *out])


def main() -> int:
    absent = declared_sources.require("gen_eval_ref", SOURCES)
    if absent is not None:
        return absent

    source = SOURCE.read_text(encoding="utf-8")

    blocks = [
        "### The suite\n",
        struct_table(source, "EvalSuite", {}),
        "\n### A task\n",
        struct_table(source, "EvalTask", {}),
        "\n### Assertions\n",
        "Each entry under `[[tasks.assertions]]` carries a `type` key that selects\nthe shape. The fields listed are the ones that shape accepts, and no others.\n",
        assertion_table(source),
        "\n### `on`, the channel a `regex` assertion matches against\n",
        enum_values(source, "OutputChannel"),
        "",
    ]
    if any(b is None for b in blocks):
        print("gen_eval_ref: a declared type was found but could not be read. "
              "No page written.", file=sys.stderr)
        return 1
    if any(b is None for b in blocks):
        print("gen_eval_ref: a declared type was found but could not be read. "
              "No page written.", file=sys.stderr)
        return 1
    generated = "\n".join(b for b in blocks if b)

    page = OUT.read_text(encoding="utf-8")
    if BEGIN not in page or END not in page:
        print(f"error: markers not found in {OUT}", file=sys.stderr)
        return 1
    head, rest = page.split(BEGIN, 1)
    _, tail = rest.split(END, 1)
    OUT.write_text(f"{head}{BEGIN}\n\n{generated}\n{END}{tail}", encoding="utf-8")
    print(f"gen_eval_ref: {generated.count(chr(10) + '| `')} rows")
    return 0


if __name__ == "__main__":
    sys.exit(main())
