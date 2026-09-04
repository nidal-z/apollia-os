#!/usr/bin/env python3
"""Generate the per-field tables of the configuration reference from the Rust source.

`docs/site/docs/reference/configuration.md` documented two keys out of a hundred
and told the reader to consult the runtime types for the rest. Copying a hundred
fields by hand would have produced a second source that drifts on the first
commit, which is the failure this whole pass exists to remove. So the tables are
derived, and only the prose around them is written.

Scope, deliberately narrow. This reads the eight sections a loader actually
consults, listed in `SECTIONS` below and kept in step with
`apollia-cli/src/config.rs::KNOWN_SECTIONS`, plus `[observability]`, which the
desktop reads from its own struct. A section that no loader consults is not
documented as configuration: see the withdrawn-sections prose in the page.

Output is spliced between two markers so the hand-written parts survive
regeneration:

    <!-- BEGIN GENERATED: config-fields -->
    <!-- END GENERATED: config-fields -->

Run via `docs/site/regen.sh`, and replayed by the `docs-generated` CI job.

Every entry of `SECTIONS` is a pointer into the tree, and a pointer is only as
good as the check on it. `LlmConfig` moved from `router.rs` to `router/config.rs`
in a module split; this generator went on looking for it in the old file, printed
a warning, exited 0, and the regeneration deleted the whole `### [llm]` table
from the published page. The declarations below are therefore crossed with the
tree before anything is written, by `declared_sources.require`, and a pointer
that no longer resolves answers 2 instead of publishing an amputated page.
"""

import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import declared_sources  # noqa: E402
from declared_sources import Source  # noqa: E402

REPO_ROOT = Path(__file__).resolve().parents[3]
OUT = REPO_ROOT / "docs" / "site" / "docs" / "reference" / "configuration.md"

BEGIN = "<!-- BEGIN GENERATED: config-fields -->"
END = "<!-- END GENERATED: config-fields -->"

# (toml section, struct name, source file, one-line purpose)
SECTIONS = [
    ("llm", "LlmConfig", "crates/apollia-llm/src/router/config.rs",
     "LLM backends and routing."),
    ("runtime", "RuntimeConfig", "crates/apollia-core/src/config/runtime.rs",
     "EventBus and mailbox capacities."),
    ("api", "ApiConfig", "crates/apollia-core/src/config/api.rs",
     "TCP listener, authentication, TLS, Unix socket."),
    ("hitl", "HitlConfig", "crates/apollia-core/src/config/hitl.rs",
     "Human-in-the-loop timeout and scan interval."),
    ("tools", "ToolsConfig", "crates/apollia-core/src/config/tools.rs",
     "Native tools: static disabling and per-tool settings."),
    ("mcp", "McpConfig", "crates/apollia-core/src/config/mcp.rs",
     "MCP client: tool loading and response limits."),
    ("hooks", "HooksConfig", "crates/apollia-core/src/config/hooks.rs",
     "Lifecycle hook handlers. `PreToolUse` is outside the supported surface of "
     "`v0.1.0-preview`: its decision is applied best effort, and a handler that "
     "times out, fails to deliver, or answers with something unparseable falls "
     "back to `allow`, so the tool call proceeds."),
    ("chat", "ChatConfig", "crates/apollia-core/src/config/chat.rs",
     "Chat session defaults."),
    ("filesystem", "FilesystemConfig", "crates/apollia-core/src/config/filesystem.rs",
     "The reversible journal, and the paths an agent works in without being "
     "asked. `trusted_paths` sets friction rather than a wall, and the two "
     "surfaces read it differently; see *Trusted paths, and what happens "
     "outside them* below."),
    ("observability", "ObservabilityConfig", "crates/apollia-core/src/observability.rs",
     "Trace capture and retention. Read by the desktop application only."),
]

# The pointers this generator commits to, read by `declared_sources.require`
# below and by `scripts/check_doc_generators.py` from the outside. One list, so
# the guard cannot answer green on a table the generator no longer uses.
SOURCES = [
    Source(path, f"pub struct {struct}", why=f"the `[{toml}]` field table")
    for toml, struct, path, _purpose in SECTIONS
]

# The pointers this generator commits to, read by `declared_sources.require`
# below and by `scripts/check_doc_generators.py` from the outside. One list, so
# the guard cannot answer green on a table the generator no longer uses.
SOURCES = [
    Source(path, f"pub struct {struct}", why=f"the `[{toml}]` field table")
    for toml, struct, path, _purpose in SECTIONS
]

FIELD_RE = re.compile(
    r"((?:^[ \t]*///[^\n]*\n)+)"          # doc-comment block
    r"((?:^[ \t]*#\[[^\n]*\]\n)*)"        # attributes
    r"^[ \t]*pub (\w+):\s*(.+?),\s*$",    # pub name: Type,  (greedy to the line end)
    re.MULTILINE,
)
DEFAULT_FN_RE = re.compile(r"fn (\w+)\(\)\s*->[^{]*\{\s*([^\n}]+?)\s*\}", re.MULTILINE)


def struct_body(source: str, name: str) -> str | None:
    """The braced body of `pub struct <name>`, by brace matching."""
    start = source.find(f"pub struct {name}")
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


# Rustdoc intra-doc links, which resolve in `cargo doc` and nowhere else. On the
# published page `[`tls_cert`](Self::tls_cert)` is a dead link and
# `[`RuntimeEvent::TokenBudgetUpdated`]` renders as literal brackets, so the code
# span is kept and the link dropped. A target with no `::` is a real URL and is
# left alone.
INTRA_DOC_TARGET = re.compile(r"\[(`[^`]+`)\]\([A-Za-z_][A-Za-z0-9_]*::[^)\s]*\)")
INTRA_DOC_SHORTCUT = re.compile(r"\[(`[^`]+`)\](?!\()")


def summarize(doc_block: str) -> str:
    """First paragraph of a doc-comment, as one line."""
    lines = []
    for raw in doc_block.splitlines():
        line = raw.strip().removeprefix("///").strip()
        if not line:
            break  # first blank line ends the summary paragraph
        lines.append(line)
    text = " ".join(lines)
    text = INTRA_DOC_TARGET.sub(r"\1", text)
    text = INTRA_DOC_SHORTCUT.sub(r"\1", text)
    # A pipe would break the markdown table.
    return text.replace("|", "\\|")


def default_for(attrs: str, rust_type: str, defaults: dict[str, str]) -> str:
    """Render the default value a missing key falls back to."""
    named = re.search(r'default\s*=\s*"(\w+)"', attrs)
    if named:
        return f"`{defaults.get(named.group(1), named.group(1) + '()')}`"
    # serde makes an Option field optional whether or not `default` is present:
    # a missing key deserializes to None. Marking it required would be wrong.
    if rust_type.startswith("Option<"):
        return "`None`"
    if "serde(default)" in attrs or "default)" in attrs:
        if rust_type == "bool":
            return "`false`"
        if rust_type.startswith(("Vec<", "HashMap<", "BTreeMap<")):
            return "empty"
        return "type default"
    return "**required**"


def render_section(toml_name: str, struct: str, rel_path: str, purpose: str) -> str | None:
    path = REPO_ROOT / rel_path
    source = path.read_text(encoding="utf-8")
    body = struct_body(source, struct)
    if body is None:
        # `require` has already proven the declaration line is in the file, so
        # reaching here means the braces do not match: a parse defect, not an
        # absent subject.
        print(f"error: the body of struct {struct} in {rel_path} does not close",
              file=sys.stderr)
        return None
    defaults = {n: v for n, v in DEFAULT_FN_RE.findall(source)}

    rows = []
    for doc, attrs, name, rust_type in FIELD_RE.findall(body):
        if "serde(skip" in attrs:
            continue
        rows.append(
            f"| `{name}` | `{rust_type.strip()}` | {default_for(attrs, rust_type.strip(), defaults)} "
            f"| {summarize(doc)} |"
        )
    if not rows:
        print(f"error: struct {struct} in {rel_path} yielded no documented field",
              file=sys.stderr)
        return None

    out = [f"### `[{toml_name}]`", "", purpose, ""]
    out.append("| Key | Type | Default | Meaning |")
    out.append("| --- | --- | --- | --- |")
    out.extend(rows)
    out.append("")
    return "\n".join(out)


def main() -> int:
    absent = declared_sources.require("gen_config_ref", SOURCES)
    if absent is not None:
        return absent

    blocks = [render_section(*s) for s in SECTIONS]
    if any(b is None for b in blocks):
        print("gen_config_ref: a declared struct was found but could not be read. "
              "No page written.", file=sys.stderr)
        return 1
    generated = "\n".join(b for b in blocks if b)

    page = OUT.read_text(encoding="utf-8")
    if BEGIN not in page or END not in page:
        print(f"error: markers not found in {OUT}", file=sys.stderr)
        return 1
    head, rest = page.split(BEGIN, 1)
    _, tail = rest.split(END, 1)
    OUT.write_text(
        f"{head}{BEGIN}\n\n{generated}{END}{tail}", encoding="utf-8"
    )
    fields = generated.count("\n| `")
    print(f"gen_config_ref: {fields} fields across {len(SECTIONS)} sections")
    return 0


if __name__ == "__main__":
    sys.exit(main())
