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
"""

import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[3]
OUT = REPO_ROOT / "docs" / "site" / "docs" / "reference" / "configuration.md"

BEGIN = "<!-- BEGIN GENERATED: config-fields -->"
END = "<!-- END GENERATED: config-fields -->"

# (toml section, struct name, source file, one-line purpose)
SECTIONS = [
    ("llm", "LlmConfig", "crates/apollia-llm/src/router.rs",
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
    ("observability", "ObservabilityConfig", "crates/apollia-core/src/observability.rs",
     "Trace capture and retention. Read by the desktop application only."),
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


def summarize(doc_block: str) -> str:
    """First paragraph of a doc-comment, as one line."""
    lines = []
    for raw in doc_block.splitlines():
        line = raw.strip().removeprefix("///").strip()
        if not line:
            break  # first blank line ends the summary paragraph
        lines.append(line)
    text = " ".join(lines)
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


def render_section(toml_name: str, struct: str, rel_path: str, purpose: str) -> str:
    path = REPO_ROOT / rel_path
    source = path.read_text(encoding="utf-8")
    body = struct_body(source, struct)
    if body is None:
        print(f"warning: struct {struct} not found in {rel_path}", file=sys.stderr)
        return ""
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
        return ""

    out = [f"### `[{toml_name}]`", "", purpose, ""]
    out.append("| Key | Type | Default | Meaning |")
    out.append("| --- | --- | --- | --- |")
    out.extend(rows)
    out.append("")
    return "\n".join(out)


def main() -> int:
    blocks = [render_section(*s) for s in SECTIONS]
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
