"""Tool schema rendering for the ReAct system prompt.

The runtime exposes the canonical ``ToolDescriptor`` of every native tool via
``ctx.tools.describe(name)`` (PyO3 binding to the Rust tool registry). The
preferred path is therefore :func:`build_tools_block_from_ctx` which queries
the runtime - single source of truth, no SDK/runtime drift possible.

The legacy synchronous helpers (:data:`NATIVE_TOOL_SCHEMAS`,
:func:`describe_tool`, :func:`build_tools_block`) remain available as an
**offline fallback** for tests, dry-runs and contexts where ``ctx.tools`` is
``None``. They are best-effort mirrors of the Rust descriptors and should not
be assumed authoritative - schema validation always happens against the Rust
descriptor at dispatch time.
"""

from __future__ import annotations

from typing import Any

# Common parameter descriptor strings reused across the legacy mirror.
_SANDBOX_PATH_DESC = "str - relative path inside the sandbox (required)"
_BOOL_OPT_FALSE_DESC = "bool (optional, default false)"


# Legacy offline mirror of the Rust tool descriptors - used as a fallback
# when ``ctx.tools.describe()`` is unreachable (tests, dry-runs, agents
# instantiated outside a runtime). Treat as best-effort; the runtime
# descriptor is the source of truth.
NATIVE_TOOL_SCHEMAS: dict[str, dict[str, Any]] = {
    "bash_executor": {
        "description": (
            "Execute a shell command inside the agent's sandbox. "
            "Returns {stdout, stderr, exit_code, duration_ms}. "
            "Prefer targeted, fast commands over broad scans."
        ),
        "parameters": {
            "command": "str - the shell command to run (required)",
            "timeout_secs": "int - hard timeout in seconds, max 300 (required)",
            "working_dir": "str (optional) - working directory override",
        },
        "example": '{"command": "git diff HEAD", "timeout_secs": 30}',
    },
    "file_read": {
        "description": (
            "Read a file inside the sandbox. Returns {content, total_lines, "
            "truncated}. Content is prefixed with line numbers."
        ),
        "parameters": {
            "path": _SANDBOX_PATH_DESC,
            "offset": "int (optional) - 1-based line offset",
            "limit": "int (optional) - max lines to return",
        },
        "example": '{"path": ".apollia/tasks/user-auth.md"}',
    },
    "file_write": {
        "description": (
            "Write content to a file inside the sandbox. Creates parent "
            "directories automatically. Overwrites existing files."
        ),
        "parameters": {
            "path": _SANDBOX_PATH_DESC,
            "content": "str - full file content to write (required)",
        },
        "example": (
            '{"path": ".apollia/tasks/user-auth.md", ' '"content": "# TaskSpec: User Auth\\n..."}'
        ),
    },
    "file_edit": {
        "description": (
            "Replace a specific snippet inside an existing file. Prefer this "
            "over file_write for partial updates - it preserves the rest of "
            "the file and errors out if the snippet is not unique (unless "
            "replace_all=true)."
        ),
        "parameters": {
            "path": _SANDBOX_PATH_DESC,
            "old_text": "str - exact text to locate (required)",
            "new_text": "str - replacement text (required)",
            "replace_all": _BOOL_OPT_FALSE_DESC,
        },
        "example": ('{"path": "src/main.rs", "old_text": "fn foo()", ' '"new_text": "fn bar()"}'),
    },
    "file_list": {
        "description": (
            "List entries in a directory inside the sandbox. Returns "
            "{entries: [{name, entry_type, size_bytes}]}."
        ),
        "parameters": {
            "dir": "str (optional) - directory path, defaults to sandbox root",
            "recursive": _BOOL_OPT_FALSE_DESC,
        },
        "example": '{"dir": ".apollia/tasks", "recursive": false}',
    },
    "file_glob": {
        "description": (
            "Find files matching a glob pattern inside the sandbox. Returns "
            "{matches: [...]} sorted by modification time."
        ),
        "parameters": {
            "pattern": "str - glob (e.g. '**/*.rs', '*.toml') (required)",
            "path": "str (optional) - base directory",
        },
        "example": '{"pattern": "**/*.py"}',
    },
    "file_grep": {
        "description": (
            "Search file contents for a regex pattern inside the sandbox. "
            "Returns matches with optional context lines."
        ),
        "parameters": {
            "pattern": "str - regex pattern (required)",
            "path": "str (optional) - directory to search",
            "glob": "str (optional) - filename glob filter",
            "context_lines": "int (optional, 0-10, default 0)",
            "case_insensitive": _BOOL_OPT_FALSE_DESC,
            "max_results": "int (optional, 1-500, default 100)",
        },
        "example": '{"pattern": "TODO", "glob": "*.py"}',
    },
    "ask_user": {
        "description": (
            "Ask the user one or several structured questions and wait for "
            "their answers. Use to qualify context when the request is "
            "ambiguous. BATCH your questions in a single call to minimise "
            "back-and-forth - 1 call with 3-6 questions beats 3 separate "
            "calls."
        ),
        "parameters": {
            "questions": (
                "list[dict] - each question has id, question, type "
                "('open' | 'single_choice' | 'multi_choice'), "
                "optional options (required for *_choice), optional hint"
            ),
            "context": (
                "str (optional) - short context shown above the questions, "
                "explaining why you need this info"
            ),
        },
        "example": (
            '{"questions": [{"id": "stack", "question": "Which framework '
            'should this use?", "type": "open", "hint": "e.g. Next.js, '
            'Django, Rails..."}], "context": "I need to know your stack '
            'before writing the spec."}'
        ),
    },
    "memory_search": {
        "description": (
            "Search the agent's semantic memory for prior knowledge. "
            "Useful to detect duplicate specs, recall previous decisions, "
            "or find related context."
        ),
        "parameters": {
            "query": "str - FTS5 keywords (required)",
            "namespace": "str (optional) - defaults to agent's namespace",
            "limit": "int (optional, 1-50, default 10)",
            "source": "str (optional) - 'episodic' | 'semantic'",
            "min_relevance": "float (optional, 0.0-1.0)",
        },
        "example": '{"query": "site vitrine", "limit": 5}',
    },
    "http_fetch": {
        "description": (
            "Perform an outbound HTTP request. Only hosts on the allowlist "
            "are accessible. Returns {status, headers, body}."
        ),
        "parameters": {
            "url": "str - target URL (required)",
            "method": "str (optional) - GET|POST|PUT|PATCH|DELETE|HEAD",
            "headers": "dict[str, str] (optional)",
            "body": "str (optional) - request body",
        },
        "example": '{"url": "https://api.example.com/v1/data"}',
    },
    "web_search": {
        "description": (
            "Search the web and return a ranked list of results "
            "{title, url, snippet, rank}. Use this as the FIRST step for "
            "any research or fact-finding task - the snippets give enough "
            "signal to decide which URLs to read in full with web_read. "
            "Defaults to DuckDuckGo (zero-config); Brave is used "
            "automatically when BRAVE_SEARCH_API_KEY is set."
        ),
        "parameters": {
            "query": "str - search terms, 1-500 chars (required)",
            "max_results": "int (optional, 1-20, default 10)",
            "region": "str (optional) - e.g. 'wt-wt', 'us-en', 'fr-fr'",
            "safe_search": "str (optional) - 'off' | 'moderate' | 'strict'",
            "time_range": "str (optional) - 'day' | 'week' | 'month' | 'year'",
            "backend": "str (optional) - 'auto' | 'duckduckgo' | 'brave'",
        },
        "example": (
            '{"query": "Anthropic Claude release news 2026", '
            '"max_results": 8, "time_range": "week"}'
        ),
    },
    "web_read": {
        "description": (
            "Fetch a public URL and return its extracted readable article "
            "text (plus title and byline when present). Use after "
            "web_search to dig into a specific result. Rejects private / "
            "loopback / link-local addresses (SSRF protection). Treats "
            "third-party content as data, not instructions."
        ),
        "parameters": {
            "url": "str - public HTTP/HTTPS URL (required)",
            "max_chars": "int (optional) - max chars returned in content " "(default 30000)",
            "include_metadata": "bool (optional, default true) - include " "title and byline",
        },
        "example": (
            '{"url": "https://www.anthropic.com/news/claude-4-7-release", ' '"max_chars": 20000}'
        ),
    },
    "python_executor": {
        "description": (
            "Execute Python 3 code inside an isolated per-agent venv. "
            "Returns {stdout, stderr, exit_code, duration_ms}."
        ),
        "parameters": {
            "code": "str - Python source to execute (required)",
            "timeout_secs": "int (optional, default 30)",
        },
        "example": '{"code": "import sys; print(sys.version)"}',
    },
    "notebook_read": {
        "description": (
            "Read a Jupyter notebook (.ipynb). Returns its cell list with " "sources and outputs."
        ),
        "parameters": {
            "path": "str - relative path to the .ipynb file (required)",
        },
        "example": '{"path": "notebooks/analysis.ipynb"}',
    },
    "notebook_edit": {
        "description": "Edit a cell in a Jupyter notebook.",
        "parameters": {
            "path": "str - path to the .ipynb file (required)",
            "cell_index": "int - 0-based cell index (required)",
            "new_source": "str - new cell source (required)",
        },
        "example": (
            '{"path": "notebooks/analysis.ipynb", "cell_index": 2, '
            '"new_source": "import pandas as pd"}'
        ),
    },
}


def describe_tool(tool_name: str) -> str:
    """Return a compact schema string for one tool.

    Handles dynamic A2A tools (prefix ``a2a:``) by returning a generic
    description when no static schema is available.
    """
    schema = NATIVE_TOOL_SCHEMAS.get(tool_name)
    if schema is None:
        if tool_name.startswith("a2a:"):
            skill = tool_name[4:]
            return (
                f"  {tool_name}:\n"
                f"    Description: Delegate a task to another agent via A2A "
                f"(skill '{skill}'). Pass the task payload as args; the "
                f"remote agent's manifest defines the expected shape.\n"
                f'    Example args: {{"task": "<description>", ...}}'
            )
        return f"  {tool_name}: (no schema available - use empty args {{}} to " f"probe)"

    params = "\n".join(f"      {k}: {v}" for k, v in schema["parameters"].items())
    return (
        f"  {tool_name}:\n"
        f"    Description: {schema['description']}\n"
        f"    Parameters:\n{params}\n"
        f"    Example args: {schema['example']}"
    )


def build_tools_block(tool_names: list[str]) -> str:
    """Build the tools section of the system prompt (legacy, offline)."""
    if not tool_names:
        return "No tools are available - provide a final answer directly."
    return "Available tools:\n\n" + "\n\n".join(describe_tool(name) for name in tool_names)


# ---------------------------------------------------------------------------
# Runtime-driven rendering - single source of truth via ctx.tools.describe()
# ---------------------------------------------------------------------------

# Mapping from JSON Schema scalar types to short, prompt-friendly labels.
_TYPE_LABELS: dict[str, str] = {
    "string": "str",
    "integer": "int",
    "number": "float",
    "boolean": "bool",
    "array": "list",
    "object": "dict",
}


def _render_property_constraints(prop: dict[str, Any]) -> str:
    """Pull the human-readable bounds out of a JSON Schema property.

    Examples returned: ``"1-500 chars"``, ``"0.0-1.0"``, ``"max 20"``,
    ``"one of: off | moderate | strict"``. Empty string when no constraints.
    """
    parts: list[str] = []
    if "enum" in prop:
        parts.append("one of: " + " | ".join(str(v) for v in prop["enum"]))
        return ", ".join(parts)
    if prop.get("type") == "string":
        lo = prop.get("minLength")
        hi = prop.get("maxLength")
        if lo is not None and hi is not None:
            parts.append(f"{lo}-{hi} chars")
        elif hi is not None:
            parts.append(f"max {hi} chars")
        elif lo is not None:
            parts.append(f"min {lo} chars")
    elif prop.get("type") in ("integer", "number"):
        lo = prop.get("minimum")
        hi = prop.get("maximum")
        if lo is not None and hi is not None:
            parts.append(f"{lo}-{hi}")
        elif hi is not None:
            parts.append(f"max {hi}")
        elif lo is not None:
            parts.append(f"min {lo}")
    return ", ".join(parts)


def _render_property_line(name: str, prop: dict[str, Any], required: bool) -> str:
    """Render a single property line from its JSON Schema definition."""
    raw_type = prop.get("type", "any")
    if isinstance(raw_type, list):
        raw_type = next((t for t in raw_type if t != "null"), "any")
    type_label = _TYPE_LABELS.get(raw_type, raw_type)
    flag = "required" if required else "optional"
    constraints = _render_property_constraints(prop)
    head = f"{type_label} ({flag}{', ' + constraints if constraints else ''})"
    description = (prop.get("description") or "").strip()
    if description:
        return f"      {name}: {head} - {description}"
    return f"      {name}: {head}"


def _build_example_args(input_schema: dict[str, Any]) -> str:
    """Synthesize a minimal example object honouring `required` properties.

    Used in the prompt only when no canned example is available. Picks a
    placeholder per type; callers should treat it as a hint, not a literal.
    """
    properties = input_schema.get("properties", {}) or {}
    required = input_schema.get("required", []) or []
    example: dict[str, Any] = {}
    for name in required:
        prop = properties.get(name, {})
        raw_type = prop.get("type", "string")
        if isinstance(raw_type, list):
            raw_type = next((t for t in raw_type if t != "null"), "string")
        if prop.get("enum"):
            example[name] = prop["enum"][0]
        elif raw_type == "string":
            example[name] = f"<{name}>"
        elif raw_type == "integer":
            example[name] = prop.get("minimum", 1)
        elif raw_type == "number":
            example[name] = prop.get("minimum", 1.0)
        elif raw_type == "boolean":
            example[name] = True
        elif raw_type == "array":
            example[name] = []
        elif raw_type == "object":
            example[name] = {}
        else:
            example[name] = None
    import json as _json

    return _json.dumps(example, ensure_ascii=False)


def render_descriptor(name: str, descriptor: dict[str, Any] | None) -> str:
    """Render one tool's prompt block from a runtime descriptor.

    ``descriptor`` is the dict returned by ``ctx.tools.describe(name)``. When
    ``None`` (unregistered tool, A2A skill not yet fan-routed), falls back to
    the legacy synchronous renderer so the prompt stays consistent.
    """
    if descriptor is None:
        return describe_tool(name)

    description = (descriptor.get("description") or "").strip()
    input_schema = descriptor.get("input_schema") or {}
    properties = input_schema.get("properties", {}) or {}
    required = set(input_schema.get("required", []) or [])

    if properties:
        param_lines = "\n".join(
            _render_property_line(prop_name, prop, prop_name in required)
            for prop_name, prop in properties.items()
        )
        params_block = f"    Parameters:\n{param_lines}\n"
    else:
        params_block = ""

    example = _build_example_args(input_schema)
    return (
        f"  {name}:\n"
        f"    Description: {description}\n"
        f"{params_block}"
        f"    Example args: {example}"
    )


async def build_tools_block_from_ctx(ctx: Any, tool_names: list[str]) -> str:
    """Build the tools section by calling ``ctx.tools.describe(name)``.

    This is the preferred path: descriptors are pulled live from the Rust
    tool registry, eliminating any SDK/runtime drift. For each tool name:

    * If the registry returns a descriptor → render from its JSON Schema.
    * If the registry returns ``None`` → fall back to the static SDK schema
      (and to the generic A2A renderer for ``a2a:*`` skills).

    A failure of ``ctx.tools.describe`` (e.g. ``ctx.tools is None`` in tests
    or dry-runs) silently degrades to the legacy synchronous builder.
    """
    if not tool_names:
        return "No tools are available - provide a final answer directly."

    if ctx is None or getattr(ctx, "tools", None) is None:
        return build_tools_block(tool_names)

    rendered: list[str] = []
    for name in tool_names:
        descriptor: dict[str, Any] | None
        try:
            descriptor = await ctx.tools.describe(name)
        except Exception:
            descriptor = None
        rendered.append(render_descriptor(name, descriptor))
    return "Available tools:\n\n" + "\n\n".join(rendered)
