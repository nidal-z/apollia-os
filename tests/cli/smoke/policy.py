#!/usr/bin/env python3
"""Named tables the CLI smoke sweep reads: what it refuses to play, and with
which arguments it plays the rest.

Two rules govern this file.

A leaf is EXCLUDED only by an entry here, and every entry carries a class and a
reason. A leaf skipped in silence is a hole disguised as coverage, so the sweep
counts what it PLAYED, never what it enumerated, and it prints this table in
full. The sweep also fails when an entry names a leaf the binary no longer has:
a stale exclusion is the same hole, one release later.

An argument vector is synthesised from the leaf's own `--help` usage line, so a
new subcommand is played the day it is merged without anyone editing this file.
`ARGV` overrides that synthesis where a seeded identifier turns a `not found`
into a real read path, and `TIMEOUTS` overrides the per-leaf deadline.

Values come from the committed fixture at tests/cli/seed (2 projects, 4 chat
sessions, 4 agents, 4 triggers, 2 MCP servers, 3 LLM backends, 5 memory
namespaces, 5 tasks, 4 audit runs, 3 transcriptions). Changing the seed changes
what these identifiers resolve to.

Stdlib only, no import of the sweep engine: this file is data plus its reasons.
"""

# ── Exclusions ─────────────────────────────────────────────────────────────
#
# leaf -> (class, reason)
#
# Classes, and what each one means:
#   blocking          the happy path does not return on its own
#   destructive-host  it writes outside the throwaway HOME, or replaces the
#                     binary under test
#   browser           it opens a browser window on the operator's desktop
#   network           its only path leaves the machine, so a run measures the
#                     network rather than the product
#
# `start` and `stop` are absent on purpose: offline they are played (a `stop`
# with no daemon is a deterministic refusal), and in --daemon mode the sweep
# excludes them itself, since the harness is what boots and kills that daemon.
EXCLUSIONS: dict[str, tuple[str, str]] = {
    "start": (
        "blocking",
        "boots the daemon in the foreground and never returns; the sweep's own "
        "--daemon mode is what exercises it, and offline it would hang",
    ),
    "mcp server": (
        "blocking",
        "serves the MCP stdio protocol on stdin until the peer closes; with "
        "stdin on /dev/null it exits on EOF, which measures the EOF path and "
        "not the server",
    ),
    "update": (
        "destructive-host",
        "downloads a GitHub release and replaces the binary under test, "
        "including the one the sweep is measuring",
    ),
    "mcp oauth login": (
        "browser",
        "opens the system browser on an authorisation URL and waits for a "
        "redirect on a loopback port",
    ),
    "connector test": (
        "network",
        "calls the live provider API with a stored token; without one it is a "
        "refusal, with one it is a network measurement",
    ),
    "mcp oauth discover": (
        "network",
        "fetches the remote server's OAuth metadata document",
    ),
    "model search": (
        "network",
        "queries the HuggingFace Hub search API",
    ),
    "model show": (
        "network",
        "fetches a HuggingFace repository manifest",
    ),
    "stt model download": (
        "network",
        "downloads a GGML whisper model from the network",
    ),
    "agent install": (
        "network",
        "the documented sources are a git URL and a registry name; the local "
        "path variant is covered by `agent validate` and `agent create`",
    ),
    "eval run": (
        "blocking",
        "runs an evaluation suite through the model, minutes per suite and "
        "non-deterministic; crates/apollia-eval is its own harness",
    ),
}

# Leaves the sweep excludes on top of EXCLUSIONS when a daemon of its own is
# running, because playing them would end the run rather than measure it.
DAEMON_EXCLUSIONS: dict[str, tuple[str, str]] = {
    "stop": (
        "blocking",
        "stops the daemon the rest of the sweep is measuring; the harness "
        "calls it itself at the end of the run",
    ),
}

# ── Argument vectors ───────────────────────────────────────────────────────
#
# leaf -> tokens appended after the leaf path. An entry here replaces the
# synthesis entirely, including its required options.
ARGV: dict[str, list[str]] = {
    # Reads that only mean something against a seeded row.
    "agent show": ["seed-classifier"],
    "agent status": ["seed-classifier"],
    "agent logs": ["seed-classifier"],
    "agent messages": ["seed-classifier"],
    "agent disable": ["seed-classifier"],
    "agent enable": ["seed-classifier"],
    "agent package show": ["seed-office-pack"],
    "chat export": ["seed-session-1"],
    "chat rename": ["seed-session-1", "Renamed by the smoke sweep"],
    "chat delete": ["seed-session-4", "--confirm"],
    "chat config authorizations revoke": ["seed-session-1", "file_read"],
    "audit show": ["seed-run-1"],
    "audit replay": ["seed-run-1"],
    "audit verify": ["seed-run-1"],
    "memory inspect": ["legacy-notes"],
    "memory search": ["legacy-notes", "runtime"],
    "memory forget": ["legacy-notes", "does-not-exist"],
    "memory export": ["--namespace", "legacy-notes"],
    "memory clear": ["--agent", "seed-classifier", "--confirm"],
    "memory purge": ["--namespace", "legacy-notes", "--older-than", "3650"],
    "memory learn-procedure": [
        "--namespace",
        "legacy-notes",
        "--trigger",
        "smoke sweep procedure",
        "--steps",
        "open the report,read the verdict",
    ],
    "project show": ["seed-project-alpha"],
    "project update": ["seed-project-alpha", "--description", "smoke sweep"],
    "project chats": ["seed-project-alpha"],
    "project agents list": ["seed-project-alpha"],
    "project agents add": ["seed-project-alpha", "seed-classifier"],
    "project agents remove": ["seed-project-alpha", "seed-classifier"],
    "project link": ["--session", "seed-session-2", "seed-project-beta"],
    "project delete": ["seed-project-beta", "--confirm"],
    "trigger status": ["seed-trigger-daily-digest"],
    "trigger logs": ["seed-trigger-daily-digest"],
    "trigger disable": ["seed-trigger-daily-digest"],
    "trigger enable": ["seed-trigger-daily-digest"],
    "trigger update": ["seed-trigger-daily-digest", "--detail", "0 6 * * *"],
    "trigger delete": ["seed-trigger-hourly-sync", "--confirm"],
    "trigger fire": ["seed-trigger-daily-digest"],
    "mcp show": ["filesystem"],
    "mcp raw-config": ["filesystem"],
    "mcp test": ["filesystem"],
    "mcp restart": ["filesystem"],
    "mcp update": ["filesystem", "--require-approval", "true"],
    "mcp remove": ["notes", "--confirm"],
    "mcp set-approval": ["filesystem", "read_file"],
    "mcp revoke-approval": ["filesystem", "read_file"],
    "mcp secret set": ["filesystem", "SMOKE_TOKEN", "smoke-value"],
    "mcp secret delete": ["filesystem", "SMOKE_TOKEN"],
    "mcp oauth status": ["filesystem"],
    "mcp oauth logout": ["filesystem"],
    "llm backends show": ["local-llama-server"],
    "llm backends update": ["local-llama-server", "--disable"],
    "llm backends set-default": ["local-llama-server"],
    "llm backends delete": ["openai-gpt4o-mini", "--confirm"],
    "llm backends create": [
        "smoke-backend",
        "--provider",
        "openai",
        "--model",
        "gpt-4o-mini",
    ],
    "llm ping": ["local-llama-server"],
    "notify update": ["seed-channel-desktop", "--enabled", "false"],
    "notify delete": ["seed-channel-webhook", "--confirm"],
    "notify create": ["--kind", "desktop"],
    "task status": ["seed-task-bravo-working"],
    "task inspect": ["seed-task-alpha-completed"],
    "task cancel": ["seed-task-bravo-working"],
    "task resume": ["seed-task-charlie-approval"],
    "trace": ["seed-task-alpha-completed"],
    "stt transcriptions delete": ["seed-transcript-alpha"],
    "permissions revoke": ["4"],
    "permissions add": ["--tool", "web_search", "--action", "allow", "--scope", "global"],
    "chat config permissions delete": ["1"],
    "resilience show": ["web_search"],
    "resilience reset": ["web_search"],
    "tools show": ["web_search"],
    "tools config get": ["web_search"],
    "tools config set": ["web_search.timeout_seconds", "30"],
    "tools disable": ["web_search"],
    "tools enable": ["web_search"],
    "tools credentials list": ["web_search"],
    "tools credentials test": ["web_search"],
    "tools credentials delete": ["web_search", "brave.api_key"],
    "config get": ["runtime.log_level"],
    "config set": ["runtime.log_level", "error"],
    "profile set": ["display_name", "Smoke Sweep"],
    "profile forget": ["display_name"],
    "completions": ["bash"],
    "guide": ["chat"],
    # Paths: the seed's own agent directory, never a path in the repository.
    "agent validate": ["@SEED_AGENT_DIR@"],
    "inspect": ["@SEED_AGENT_DIR@"],
    "agent update": ["seed-classifier", "@SEED_AGENT_DIR@"],
    "agent start": ["@SEED_AGENT_DIR@"],
    # Writes that must land inside the throwaway HOME, never the caller's cwd.
    "audit export": ["--output", "@RUN_DIR@/audit-export.json"],
    "profile export": ["--output", "@RUN_DIR@/profile.json"],
    "profile import": ["--input", "@RUN_DIR@/profile-in.json"],
    "memory import": [
        "--namespace",
        "legacy-notes",
        "--input",
        "@RUN_DIR@/memory-in.json",
    ],
    "eval report": ["@RUN_DIR@/eval.jsonl"],
    "stt transcribe": ["@RUN_DIR@/sample.wav"],
    "llm setup": ["--model", "@SEED_HOME@/.apollia/models/Phi-3-mini-Q4.gguf"],
    "review": ["--diff", "@RUN_DIR@/sample.diff"],
    # Refusals reached on purpose, so the sweep measures a decided path.
    "connector revoke": ["google", "smoke@example.invalid", "--confirm"],
    "connector api-key set": ["brave", "smoke-key"],
    "connector client-id set": ["google", "smoke-client-id"],
    "connector client-secret set": ["google", "smoke-client-secret"],
    "connector drive folder set": ["smoke@example.invalid", "/Smoke"],
    "connector drive folder picked list": ["smoke@example.invalid"],
    "connector drive folder picked remove": ["smoke@example.invalid", "folder-id"],
    "connector drive folder reset": ["smoke@example.invalid"],
    "mcp oauth client-id set": ["SMOKE_MCP_CLIENT_ID", "smoke-value"],
    "mcp oauth client-id clear": ["SMOKE_MCP_CLIENT_ID"],
    "a2a invoke": ["seed.classify", "--caller", "smoke-sweep"],
    "do": ["summarise the seeded project"],
    "explain": ["what a sovereign runtime is"],
    "llm chat": ["say ok"],
    "run": ["seed-classifier", "ping"],
    "agent create": ["smoke-agent"],
    "project create": ["Smoke Project"],
    "mcp add": ["smoke-server", "--command", "/bin/echo"],
    "trigger create": [
        "smoke-trigger",
        "--agent",
        "seed-classifier",
        "--kind",
        "cron",
        "--detail",
        "0 6 * * 1",
    ],
    "agent uninstall": ["seed-classifier", "--confirm"],
    "agent repair": ["seed-classifier"],
    "agent package uninstall": ["seed-office-pack", "--confirm"],
    "agent stop": ["seed-classifier"],
    "notify events set": ["task.completed"],
}

# Per-leaf deadline in seconds; DEFAULT_TIMEOUT elsewhere.
TIMEOUTS: dict[str, int] = {
    "doctor": 90,
    "do": 60,
    "explain": 60,
    "llm chat": 60,
    "run": 60,
    "review": 60,
    "digest": 60,
    "guide": 60,
    "onboard": 60,
    "trigger fire": 60,
    "mcp test": 60,
    "mcp restart": 60,
    "a2a invoke": 60,
    "llm ping": 60,
    "llm setup": 90,
}

DEFAULT_TIMEOUT = 30

# Placeholder value used when the usage line names a positional or a required
# option this file says nothing about. A new leaf is played with these rather
# than skipped, which is the point: an unknown identifier still exercises the
# parse, the lookup and the not-found path.
PLACEHOLDER_DEFAULTS: dict[str, str] = {
    "PATH": "@RUN_DIR@/placeholder",
    "FILE": "@RUN_DIR@/placeholder",
    "OUTPUT": "@RUN_DIR@/placeholder-out",
    "INPUT": "@RUN_DIR@/placeholder",
    "JSONL": "@RUN_DIR@/eval.jsonl",
    "SHELL": "bash",
    "DAYS": "3650",
    "NAME": "smoke-placeholder",
    "ID": "smoke-placeholder",
    "TEXT": "smoke placeholder text",
    "QUERY": "smoke",
    "VALUE": "smoke-value",
    "KEY": "smoke.key",
}

GENERIC_PLACEHOLDER = "smoke-placeholder"
