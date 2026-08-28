#!/usr/bin/env python3
"""Generate the EventBus catalogue of the docs site from the Rust source.

`crates/apollia-runtime/AGENTS.md` says renaming a variant is a wire-format
change and tells the reader to document it under `docs/site/docs/reference/`.
Nothing was ever published there: the catalogue existed only as a 1600-line
Rust enum, and the page that would have said which variants are under contract
did not exist. Twenty-one of them were orphaned on one side or the other before
anyone counted, and nine named a subsystem no crate implements.

So the table is derived, never copied. Two sources, both read here:

  * `crates/apollia-core/src/events/runtime_event.rs`, the variants and the
    first line of each doc-comment;
  * `crates/apollia-desktop/src/events.rs`, function `categorize`, the category
    the desktop bridge routes each variant under. That category, not the variant
    name, is what the webview dispatches on, so a page that omits it describes
    half the contract.

Output is spliced between two markers in both locales, so the hand-written prose
around it survives regeneration:

    <!-- BEGIN GENERATED: eventbus-catalogue -->
    <!-- END GENERATED: eventbus-catalogue -->

Both are declared in `SOURCES` and crossed with the tree before the first read.
The file half of that check was here from the start; the symbol half was not,
and the symbol is what a module split moves. A sibling generator lost the whole
`### [llm]` table of the configuration reference that way, warning and exiting 0.

Both are declared in `SOURCES` and crossed with the tree before the first read.
The file half of that check was here from the start; the symbol half was not,
and the symbol is what a module split moves. A sibling generator lost the whole
`### [llm]` table of the configuration reference that way, warning and exiting 0.

Run via `docs/site/regen.sh`, and replayed by the `docs-generated` CI job.
"""

import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import declared_sources  # noqa: E402
from declared_sources import Source  # noqa: E402

REPO_ROOT = Path(__file__).resolve().parents[3]
ENUM_RS = "crates/apollia-core/src/events/runtime_event.rs"
BRIDGE_RS = "crates/apollia-desktop/src/events.rs"
ENUM = REPO_ROOT / ENUM_RS
BRIDGE = REPO_ROOT / BRIDGE_RS

# Declared once, crossed with the tree before the first read, and read from the
# outside by `scripts/check_doc_generators.py`. The file check alone was here
# already; naming the symbol is what a module split breaks, and what a path
# check cannot see.
SOURCES = [
    Source(ENUM_RS, "pub enum RuntimeEvent", why="the variants and their doc-comments"),
    Source(BRIDGE_RS, "fn categorize(", why="the bridge category of each variant"),
]

BEGIN = "<!-- BEGIN GENERATED: eventbus-catalogue -->"
END = "<!-- END GENERATED: eventbus-catalogue -->"

# The doc-comments of the enum are English, and "one language per file"
# (`docs/agents/DOCS-WRITING.md` section 4) forbids serving them under `/fr/`.
# The French page therefore carries the same rows without that column: the
# variant names and the payload shapes are the part a French reader needs, and
# the prose around the table says the rest in French.
#
# (page, column headers, payload-shape wording, "no doc-comment" filler)
LOCALES = [
    (
        REPO_ROOT / "docs/site/docs/reference/events.md",
        ("Variant", "Payload", "What it reports"),
        {"tuple": "tuple", "{": "named fields", "none": "none"},
        "No description in the source.",
    ),
    (
        REPO_ROOT
        / "docs/site/i18n/fr/docusaurus-plugin-content-docs/current/reference/events.md",
        ("Variante", "Charge utile"),
        {"tuple": "tuple", "{": "champs nommés", "none": "aucune"},
        None,
    ),
]

VARIANT = re.compile(
    r"((?:^[ \t]*///[^\n]*\n)*)"  # doc-comment block, possibly empty
    r"^    ([A-Z][A-Za-z0-9]*)\s*(\(|\{|,)",
    re.M,
)


def variants(text: str) -> list[tuple[str, str, str]]:
    """(name, payload shape, first doc line) for every variant, in source order."""
    body = re.search(r"pub enum RuntimeEvent \{(.*?)\n\}", text, re.S)
    if not body:
        return []
    out: list[tuple[str, str, str]] = []
    for match in VARIANT.finditer(body.group(1)):
        doc_block, name, opener = match.groups()
        lines = [
            line.strip().removeprefix("///").strip()
            for line in doc_block.splitlines()
            if line.strip().startswith("///")
        ]
        summary = ""
        for line in lines:
            if line:
                summary = line
                break
        shape = {"(": "tuple", "{": "{", ",": "none"}[opener]
        out.append((name, shape, summary))
    return out


def categories(text: str) -> dict[str, str]:
    """Map each variant to the category `categorize` routes it under."""
    start = text.find("fn categorize(")
    if start < 0:
        return {}
    brace = text.find("{", start)
    depth = 0
    end = len(text)
    for k in range(brace, len(text)):
        if text[k] == "{":
            depth += 1
        elif text[k] == "}":
            depth -= 1
            if depth == 0:
                end = k + 1
                break
    body = re.sub(r"//[^\n]*", "", text[start:end])
    result: dict[str, str] = {}
    for arm in re.finditer(r"((?:RuntimeEvent::[^=]*?))=>\s*\{?\s*\"([a-z0-9-]+)\"", body, re.S):
        for token in re.finditer(r"RuntimeEvent::([A-Z][A-Za-z0-9]*)\b", arm.group(1)):
            result.setdefault(token.group(1), arm.group(2))
    return result


def render(
    rows: list[tuple[str, str, str]],
    category_of: dict[str, str],
    headers: tuple[str, ...],
    shapes: dict[str, str],
    filler: str | None,
) -> str:
    by_category: dict[str, list[tuple[str, str, str]]] = {}
    for name, shape, summary in rows:
        by_category.setdefault(category_of.get(name, "?"), []).append((name, shape, summary))
    out: list[str] = []
    for category in sorted(by_category):
        out.append(f"### `{category}`")
        out.append("")
        out.append("| " + " | ".join(headers) + " |")
        out.append("|" + "---|" * len(headers))
        for name, shape, summary in sorted(by_category[category]):
            cells = [f"`{name}`", shapes[shape]]
            if filler is not None:
                cells.append((summary or filler).replace("|", "\\|"))
            out.append("| " + " | ".join(cells) + " |")
        out.append("")
    return "\n".join(out)


def main() -> int:
    absent = declared_sources.require("gen_events_ref", SOURCES)
    if absent is not None:
        return absent
    rows = variants(ENUM.read_text(encoding="utf-8"))
    category_of = categories(BRIDGE.read_text(encoding="utf-8"))
    if not rows or not category_of:
        print("error: nothing parsed from the enum or from categorize()", file=sys.stderr)
        return 2
    missing = [name for name, _shape, _doc in rows if name not in category_of]
    if missing:
        print(
            f"error: {len(missing)} variant(s) the bridge does not categorise: "
            f"{' '.join(missing)}",
            file=sys.stderr,
        )
        return 1

    for page, headers, shapes, filler in LOCALES:
        if not page.exists():
            print(f"error: {page} is absent", file=sys.stderr)
            return 1
        text = page.read_text(encoding="utf-8")
        if BEGIN not in text or END not in text:
            print(f"error: markers not found in {page}", file=sys.stderr)
            return 1
        head, rest = text.split(BEGIN, 1)
        _, tail = rest.split(END, 1)
        generated = render(rows, category_of, headers, shapes, filler)
        page.write_text(f"{head}{BEGIN}\n\n{generated}{END}{tail}", encoding="utf-8")

    print(
        f"gen_events_ref: {len(rows)} variants across "
        f"{len(set(category_of[name] for name, _s, _d in rows))} categories"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
