#!/usr/bin/env python3
"""Generate the key tables of the configuration reference from the key registry.

`docs/site/docs/reference/configuration.md` documented two keys out of a hundred
and told the reader to consult the runtime types for the rest. Copying a hundred
fields by hand would have produced a second source that drifts on the first
commit, so the tables are derived, and only the prose around them is written.

The source is the key registry, `apollia_core::config::keys::KEYS`: one entry
per key of `apollia.toml`, nested ones included, with its type, its default, its
documentation, whether the organization policy can fix it,
and whether it is withdrawn. The default file written on a first start and
`apollia-os config keys` read the same registry, and
`scripts/check_config_reference.py` holds the page and its French mirror to it.
The registry is read by that guard's parser, so the two cannot disagree on what
an entry says.

Output is spliced between two markers so the hand-written parts survive
regeneration:

    <!-- BEGIN GENERATED: config-fields -->
    <!-- END GENERATED: config-fields -->

Run via `docs/site/regen.sh`, and replayed by the `docs-generated` CI job. The
French mirror is translated by hand; the guard keeps its keys and defaults in
step.

The registry pointer below is crossed with the tree before anything is written,
by `declared_sources.require`, and a pointer that no longer resolves answers 2
instead of publishing an amputated page.
"""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
sys.path.insert(0, str(Path(__file__).resolve().parents[3] / "scripts"))

import declared_sources  # noqa: E402
from check_config_reference import REGISTRY, read_registry  # noqa: E402
from declared_sources import Source  # noqa: E402

REPO_ROOT = Path(__file__).resolve().parents[3]
OUT = REPO_ROOT / "docs" / "site" / "docs" / "reference" / "configuration.md"

BEGIN = "<!-- BEGIN GENERATED: config-fields -->"
END = "<!-- END GENERATED: config-fields -->"

# One line of purpose per section, in the order the registry declares them.
PURPOSES = {
    "llm": "LLM backends, routing, and the speech-to-text runner. The default "
           "file leaves this section commented: the backends are managed in the "
           "AI settings of the application, which keep them in the engine's "
           "database.",
    "api": "TCP listener, authentication, TLS, Unix socket.",
    "runtime": "EventBus and mailbox capacities.",
    "hitl": "Human-in-the-loop timeout and scan interval.",
    "tools": "Native tools: limits, static disabling, and per-tool settings.",
    "mcp": "MCP client: approvals, tool loading, and the bridge that serves "
           "exposed agents and workflows.",
    "hooks": "Lifecycle hook handlers. `PreToolUse` is applied best effort: a "
             "handler that times out, fails to deliver, or answers with something "
             "unparseable falls back to `allow`, so the tool call proceeds.",
    "chat": "Chat session defaults.",
    "filesystem": "The reversible journal, and the paths an agent works in without "
                  "being asked. `trusted_paths` sets friction rather than a wall, and "
                  "the two surfaces read it differently; see *Trusted paths, and what "
                  "happens outside them* below.",
    "models": "The signed index of model cards, its mirror, the models folder, "
              "and the rule applied to an imported model file.",
    "server": "The compatible server, which answers the OpenAI and Anthropic "
              "formats on the loopback interface.",
    "workflows": "The budget a workflow run starts with. It is shared: every agent "
                 "a step runs and every workflow a step calls spend from it.",
    "desktop": "Settings of the desktop application that the engine reads.",
    "engines": "Settings of the local inference engines.",
    "author": "The publisher name of the agents exported from this machine.",
    "observability": "Trace capture and retention. Read by the desktop application "
                     "only.",
    "ui": "The opt-in link preview of the desktop application.",
}

# The pointer this generator commits to, read by `declared_sources.require`
# below and by `scripts/check_doc_generators.py` from the outside.
SOURCES = [
    Source(str(REGISTRY.relative_to(REPO_ROOT)), "pub const KEYS: &[ConfigKey] = &[",
           why="every key table of the page"),
]

def render(keys) -> str | None:
    live = [k for k in keys if k.withdrawn is None]
    sections: list[str] = []
    for k in live:
        section = k.path.split(".")[0]
        if section not in sections:
            sections.append(section)
    missing = [s for s in sections if s not in PURPOSES]
    if missing:
        print(f"error: no purpose written for section(s) {missing}", file=sys.stderr)
        return None
    out = []
    for section in sections:
        out += [f"### `[{section}]`", "", PURPOSES[section], ""]
        out.append("| Key | Type | Default | Meaning |")
        out.append("| --- | --- | --- | --- |")
        for k in live:
            if k.path.split(".")[0] != section:
                continue
            default = f"`{k.default}`" if k.default is not None else "not set"
            meaning = k.doc.replace("|", "\\|").replace("<", "&lt;").replace(">", "&gt;")
            if k.lockable:
                meaning += " The organization policy can fix it."
            out.append(f"| `{k.path}` | {k.ty} | {default} | {meaning} |")
        out.append("")
    out += ["### Withdrawn keys", "",
            "Accepted in a file written for an earlier version, with a "
            "`config.key.retired` warning, and read by nothing.", "",
            "| Key | Instead |", "| --- | --- |"]
    for k in keys:
        if k.withdrawn is not None:
            out.append(f"| `{k.path}` | {k.withdrawn[0].upper()}{k.withdrawn[1:]}. |")
    out.append("")
    return "\n".join(out)


def main() -> int:
    absent = declared_sources.require("gen_config_ref", SOURCES)
    if absent is not None:
        return absent
    try:
        keys = read_registry(REGISTRY)
    except ValueError as exc:
        print(f"error: the key registry could not be read: {exc}", file=sys.stderr)
        return 1
    generated = render(keys)
    if generated is None:
        print("gen_config_ref: no page written.", file=sys.stderr)
        return 1

    page = OUT.read_text(encoding="utf-8")
    if BEGIN not in page or END not in page:
        print(f"error: markers not found in {OUT}", file=sys.stderr)
        return 1
    head, rest = page.split(BEGIN, 1)
    _, tail = rest.split(END, 1)
    OUT.write_text(
        f"{head}{BEGIN}\n\n{generated}{END}{tail}", encoding="utf-8"
    )
    print(f"gen_config_ref: {len(keys)} registry entries")
    return 0


if __name__ == "__main__":
    sys.exit(main())
