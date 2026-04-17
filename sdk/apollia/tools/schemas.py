"""Schema descriptions for native Apollia OS tools.

These schemas are injected into the ReactAgent system prompt so the LLM
knows parameter names and types for each built-in tool. They must stay in
sync with the real tool descriptors in
``crates/apollia-tools/src/tools/*.rs``.
"""

from __future__ import annotations

from typing import Any


NATIVE_TOOL_SCHEMAS: dict[str, dict[str, Any]] = {
    "bash_executor": {
        "description": (
            "Execute a shell command inside the agent's sandbox. "
            "Returns {stdout, stderr, exit_code, duration_ms}. "
            "Prefer targeted, fast commands over broad scans."
        ),
        "parameters": {
            "command": "str — the shell command to run (required)",
            "timeout_secs": "int — hard timeout in seconds, max 300 (required)",
            "working_dir": "str (optional) — working directory override",
        },
        "example": '{"command": "git diff HEAD", "timeout_secs": 30}',
    },
    "file_read": {
        "description": (
            "Read a file inside the sandbox. Returns {content, total_lines, "
            "truncated}. Content is prefixed with line numbers."
        ),
        "parameters": {
            "path": "str — relative path inside the sandbox (required)",
            "offset": "int (optional) — 1-based line offset",
            "limit": "int (optional) — max lines to return",
        },
        "example": '{"path": ".apollia/tasks/user-auth.md"}',
    },
    "file_write": {
        "description": (
            "Write content to a file inside the sandbox. Creates parent "
            "directories automatically. Overwrites existing files."
        ),
        "parameters": {
            "path": "str — relative path inside the sandbox (required)",
            "content": "str — full file content to write (required)",
        },
        "example": (
            '{"path": ".apollia/tasks/user-auth.md", '
            '"content": "# TaskSpec: User Auth\\n..."}'
        ),
    },
    "file_edit": {
        "description": (
            "Replace a specific snippet inside an existing file. Prefer this "
            "over file_write for partial updates — it preserves the rest of "
            "the file and errors out if the snippet is not unique (unless "
            "replace_all=true)."
        ),
        "parameters": {
            "path": "str — relative path inside the sandbox (required)",
            "old_text": "str — exact text to locate (required)",
            "new_text": "str — replacement text (required)",
            "replace_all": "bool (optional, default false)",
        },
        "example": (
            '{"path": "src/main.rs", "old_text": "fn foo()", '
            '"new_text": "fn bar()"}'
        ),
    },
    "file_list": {
        "description": (
            "List entries in a directory inside the sandbox. Returns "
            "{entries: [{name, entry_type, size_bytes}]}."
        ),
        "parameters": {
            "dir": "str (optional) — directory path, defaults to sandbox root",
            "recursive": "bool (optional, default false)",
        },
        "example": '{"dir": ".apollia/tasks", "recursive": false}',
    },
    "file_glob": {
        "description": (
            "Find files matching a glob pattern inside the sandbox. Returns "
            "{matches: [...]} sorted by modification time."
        ),
        "parameters": {
            "pattern": "str — glob (e.g. '**/*.rs', '*.toml') (required)",
            "path": "str (optional) — base directory",
        },
        "example": '{"pattern": "**/*.py"}',
    },
    "file_grep": {
        "description": (
            "Search file contents for a regex pattern inside the sandbox. "
            "Returns matches with optional context lines."
        ),
        "parameters": {
            "pattern": "str — regex pattern (required)",
            "path": "str (optional) — directory to search",
            "glob": "str (optional) — filename glob filter",
            "context_lines": "int (optional, 0-10, default 0)",
            "case_insensitive": "bool (optional, default false)",
            "max_results": "int (optional, 1-500, default 100)",
        },
        "example": '{"pattern": "TODO", "glob": "*.py"}',
    },
    "ask_user": {
        "description": (
            "Ask the user one or several structured questions and wait for "
            "their answers. Use to qualify context when the request is "
            "ambiguous. BATCH your questions in a single call to minimise "
            "back-and-forth — 1 call with 3-6 questions beats 3 separate "
            "calls."
        ),
        "parameters": {
            "questions": (
                "list[dict] — each question has id, question, type "
                "('open' | 'single_choice' | 'multi_choice'), "
                "optional options (required for *_choice), optional hint"
            ),
            "context": (
                "str (optional) — short context shown above the questions, "
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
            "query": "str — FTS5 keywords (required)",
            "namespace": "str (optional) — defaults to agent's namespace",
            "limit": "int (optional, 1-50, default 10)",
            "source": "str (optional) — 'episodic' | 'semantic'",
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
            "url": "str — target URL (required)",
            "method": "str (optional) — GET|POST|PUT|PATCH|DELETE|HEAD",
            "headers": "dict[str, str] (optional)",
            "body": "str (optional) — request body",
        },
        "example": '{"url": "https://api.example.com/v1/data"}',
    },
    "python_executor": {
        "description": (
            "Execute Python 3 code inside an isolated per-agent venv. "
            "Returns {stdout, stderr, exit_code, duration_ms}."
        ),
        "parameters": {
            "code": "str — Python source to execute (required)",
            "timeout_secs": "int (optional, default 30)",
        },
        "example": '{"code": "import sys; print(sys.version)"}',
    },
    "notebook_read": {
        "description": (
            "Read a Jupyter notebook (.ipynb). Returns its cell list with "
            "sources and outputs."
        ),
        "parameters": {
            "path": "str — relative path to the .ipynb file (required)",
        },
        "example": '{"path": "notebooks/analysis.ipynb"}',
    },
    "notebook_edit": {
        "description": "Edit a cell in a Jupyter notebook.",
        "parameters": {
            "path": "str — path to the .ipynb file (required)",
            "cell_index": "int — 0-based cell index (required)",
            "new_source": "str — new cell source (required)",
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
                f"    Example args: {{\"task\": \"<description>\", ...}}"
            )
        return (
            f"  {tool_name}: (no schema available — use empty args {{}} to "
            f"probe)"
        )

    params = "\n".join(
        f"      {k}: {v}" for k, v in schema["parameters"].items()
    )
    return (
        f"  {tool_name}:\n"
        f"    Description: {schema['description']}\n"
        f"    Parameters:\n{params}\n"
        f"    Example args: {schema['example']}"
    )


def build_tools_block(tool_names: list[str]) -> str:
    """Build the tools section of the system prompt."""
    if not tool_names:
        return "No tools are available — provide a final answer directly."
    return "Available tools:\n\n" + "\n\n".join(
        describe_tool(name) for name in tool_names
    )
