#!/usr/bin/env python3
"""Hold the mechanisable Rust rules of FORBIDDEN.md and RUST-PATTERNS.md.

The corpus writes fifteen Rust rules and, before this guard, two had a gate.
Every other rule was a wish: eighteen tracing calls carried a format string,
twelve wildcard re-exports sat in module roots, fifteen `#[error]` messages
broke the house style, and nothing could tell a reader whether the next commit
added a nineteenth. This guard reads the production sources the way
`scripts/check_panic_free.py` does (same inventory, same test exclusion,
imported rather than rewritten) and holds each rule either strictly at zero or
on a two-sided named ratchet.

Strict rules, red on the first site:

  panic-macros       panic! / todo! / unimplemented! in production
  print-macros       println! and kin outside apollia-cli, without a REASON
  anyhow             `anyhow` as a dependency or as a path
  tracing-format     a placeholder inside the message of a tracing macro
  wildcard-reexport  pub use ...::* (plain glob imports are reported aside)
  error-msg-style    #[error("...")] starting with a capitalised common word
                     or ending with a period
  unsafe-safety      an `unsafe` block, fn, impl or extern without a SAFETY
                     comment in the contiguous comment block above it,
                     in production and in tests alike
  unbounded          mpsc::unbounded_channel, UnboundedSender/Receiver,
                     blocking thread::sleep, FuturesUnordered in production
  internal-refs      tracker identifiers and numbered planning vocabulary,
                     minus the named waivers below
  error-enums        a public thiserror enum declared without
                     #[non_exhaustive]

Ratchet rules, frozen as named tables that move only with the code:

  async-trait        the traits allowed to keep #[async_trait], and the
                     manifests allowed to declare the dependency; a new
                     trait or a new manifest is red
  module-size        the files allowed to exceed 800 production lines
  arc-mutex          per-file counts of Arc<Mutex|RwLock> sites and the
                     named type aliases that wrap one
  time-sensitive-tests
                     per-file counts of test bodies whose verdict depends on
                     something the machine controls: a sleep, a port, an
                     external process, a wall-clock deadline, a script written
                     for the code under test to run

A ratchet is two-sided: a site above the table is a regression, a site below
it is debt paid that must lower the table in the same commit, otherwise the
table records a maximum nobody comes back to.

Exit codes: 0 clean, 1 at least one rule broken, 2 nothing measured.

Usage:
    python3 scripts/check_rust_rules.py [rule] [--list]
    python3 scripts/check_rust_rules.py --selftest
"""

import argparse
import re
import sys
from bisect import bisect_right
from collections import Counter
from dataclasses import dataclass
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(Path(__file__).resolve().parent))

import check_panic_free as guard  # noqa: E402

MODULE_SIZE_THRESHOLD = 800

# ── ratchet tables ───────────────────────────────────────────────────────────
# Each entry is debt the tree carried when the table was written. Remove the
# entry in the commit that removes the debt; the run turns red in both
# directions until table and tree agree.

# Traits that predate the RPITIT rule and still carry #[async_trait]. A new
# trait must use return-position impl Trait instead.
ASYNC_TRAIT_TRAITS = {
    "crates/apollia-core/src/context.rs::ContextProvider",
    "crates/apollia-core/src/workspace.rs::WorkspaceProvider",
    "crates/apollia-llm/src/tool_helper.rs::ToolInvoker",
    "crates/apollia-llm/src/types.rs::CompletionModel",
    "crates/apollia-mcp/src/config.rs::SecretResolver",
    "crates/apollia-mcp/src/transport/mod.rs::McpTransport",
    "crates/apollia-notifications/src/engine.rs::NotificationChannel",
    "crates/apollia-oria/src/actor.rs::ToolProxyTrait",
    "crates/apollia-runtime/src/chat/agent_chat.rs::ChatAgentRunner",
    "crates/apollia-runtime/src/chat/types.rs::ProjectContextProvider",
    "crates/apollia-tools/src/tools/web_search/backend.rs::SearchBackend",
}

# Manifests allowed to declare the async-trait dependency, for the traits
# above and their impls. Empties as the traits migrate.
ASYNC_TRAIT_MANIFESTS = {
    "crates/apollia-aip/Cargo.toml",
    "crates/apollia-cli/Cargo.toml",
    "crates/apollia-core/Cargo.toml",
    "crates/apollia-desktop/Cargo.toml",
    "crates/apollia-eval/Cargo.toml",
    "crates/apollia-llm/Cargo.toml",
    "crates/apollia-mcp/Cargo.toml",
    "crates/apollia-notifications/Cargo.toml",
    "crates/apollia-oria/Cargo.toml",
    "crates/apollia-runtime/Cargo.toml",
    "crates/apollia-tools/Cargo.toml",
    "crates/apollia-workspace/Cargo.toml",
}

# Files over 800 production lines when the table was written. A split removes
# the entry in the same commit.
MODULE_SIZE_FILES: set[str] = {
    "crates/apollia-aip/src/context.rs",
    "crates/apollia-aip/src/llm.rs",
    "crates/apollia-aip/src/memory.rs",
    "crates/apollia-core/src/events/runtime_event.rs",
    "crates/apollia-desktop/src/backend.rs",
    "crates/apollia-desktop/src/commands/agents.rs",
    "crates/apollia-desktop/src/commands/mcp.rs",
    "crates/apollia-desktop/src/commands/observability.rs",
    "crates/apollia-desktop/src/commands/onboarding.rs",
    "crates/apollia-desktop/src/main.rs",
    "crates/apollia-llm/src/backends/anthropic.rs",
    "crates/apollia-llm/src/backends/openai.rs",
    "crates/apollia-llm/src/hf_registry.rs",
    "crates/apollia-llm/src/router.rs",
    "crates/apollia-mcp/src/manager.rs",
    "crates/apollia-mcp/src/session.rs",
    "crates/apollia-notifications/src/engine.rs",
    "crates/apollia-oria/src/actor.rs",
    "crates/apollia-oria/src/engine.rs",
    "crates/apollia-permissions/src/prefix_rule_engine.rs",
    "crates/apollia-tools/src/executor.rs",
    "crates/apollia-tools/src/task_repository.rs",
    "crates/apollia-triggers/src/engine.rs",
}

# Per-file counts of Arc<Mutex|RwLock> production sites. The actor migration
# that shrinks a count lowers the entry with it.
ARC_MUTEX_COUNTS: dict[str, int] = {
    "crates/apollia-aip/src/llm.rs": 1,
    "crates/apollia-aip/src/memory.rs": 14,
    "crates/apollia-aip/src/secrets.rs": 2,
    "crates/apollia-auth/src/auth_manager.rs": 1,
    "crates/apollia-auth/src/callback.rs": 2,
    "crates/apollia-auth/src/mcp_oauth_orchestrator.rs": 3,
    "crates/apollia-cli/src/commands/start.rs": 6,
    "crates/apollia-core/src/pending_approvals.rs": 1,
    "crates/apollia-desktop/src/backend.rs": 7,
    "crates/apollia-desktop/src/commands/agent_packages.rs": 6,
    "crates/apollia-desktop/src/commands/agents.rs": 7,
    "crates/apollia-desktop/src/commands/llm.rs": 1,
    "crates/apollia-desktop/src/commands/model_hub.rs": 1,
    "crates/apollia-desktop/src/commands/onboarding.rs": 1,
    "crates/apollia-desktop/src/commands/stt.rs": 2,
    "crates/apollia-desktop/src/commands/user_memory.rs": 1,
    "crates/apollia-desktop/src/main.rs": 7,
    "crates/apollia-desktop/src/stt/flow.rs": 3,
    "crates/apollia-llm/src/backends/vertex.rs": 1,
    "crates/apollia-llm/src/downloader.rs": 1,
    "crates/apollia-llm/src/meta_orchestrator.rs": 1,
    "crates/apollia-llm/src/repository.rs": 1,
    "crates/apollia-llm/src/router.rs": 1,
    "crates/apollia-mcp/src/session.rs": 3,
    "crates/apollia-mcp/src/transport/stdio.rs": 1,
    "crates/apollia-oria/src/actor.rs": 2,
    "crates/apollia-oria/src/engine.rs": 4,
    "crates/apollia-runner/src/server/mod.rs": 1,
    "crates/apollia-runtime/src/a2a/sidechain.rs": 2,
    "crates/apollia-runtime/src/api/routes_llm/backends.rs": 1,
    "crates/apollia-runtime/src/api/routes_mcp.rs": 1,
    "crates/apollia-runtime/src/api/routes_notifications.rs": 1,
    "crates/apollia-runtime/src/api/routes_triggers/support.rs": 1,
    "crates/apollia-runtime/src/api/server.rs": 14,
    "crates/apollia-runtime/src/chat/builtin_agent/invoker.rs": 2,
    "crates/apollia-runtime/src/chat/builtin_agent/mod.rs": 2,
    "crates/apollia-runtime/src/chat/extractor.rs": 4,
    "crates/apollia-runtime/src/chat/manager/handle.rs": 1,
    "crates/apollia-runtime/src/chat/manager/mod.rs": 2,
    "crates/apollia-runtime/src/chat/manager/types.rs": 1,
    "crates/apollia-runtime/src/chat/native_wrappers.rs": 1,
    "crates/apollia-runtime/src/chat/types.rs": 1,
    "crates/apollia-runtime/src/chat/types/approvals.rs": 2,
    "crates/apollia-runtime/src/embedded.rs": 3,
    "crates/apollia-runtime/src/llama_server/mod.rs": 4,
    "crates/apollia-runtime/src/observability/resilience_subscriber.rs": 1,
    "crates/apollia-runtime/src/perf_trace.rs": 1,
    "crates/apollia-runtime/src/runner_supervisor/lifecycle.rs": 3,
    "crates/apollia-runtime/src/runner_supervisor/proxy.rs": 2,
    "crates/apollia-runtime/src/session_metrics.rs": 1,
    "crates/apollia-runtime/src/session_replay.rs": 1,
    "crates/apollia-runtime/src/stt/builder.rs": 1,
    "crates/apollia-runtime/src/supervisor/bootstrap.rs": 2,
    "crates/apollia-runtime/src/supervisor/lifecycle.rs": 4,
    "crates/apollia-runtime/src/supervisor/mod.rs": 3,
    "crates/apollia-runtime/src/supervisor/persistence.rs": 2,
    "crates/apollia-stt/src/audio/capture.rs": 1,
    "crates/apollia-tools/src/agent_repository.rs": 1,
    "crates/apollia-tools/src/package_repository.rs": 1,
    "crates/apollia-tools/src/project_repository.rs": 1,
    "crates/apollia-tools/src/tools/ask_user.rs": 1,
    "crates/apollia-tools/src/tools/file_read.rs": 2,
    "crates/apollia-workspace/src/assembler.rs": 1,
}

# Type aliases that wrap an Arc<Mutex|RwLock>; a new alias is a new shared
# lock and is red.
ARC_MUTEX_ALIASES: set[str] = {
    "crates/apollia-auth/src/callback.rs::ResultCell",
    "crates/apollia-desktop/src/commands/llm.rs::LlmPingCache",
    "crates/apollia-desktop/src/commands/model_hub.rs::SharedDownloadManager",
    "crates/apollia-desktop/src/commands/stt.rs::SttFlowState",
    "crates/apollia-desktop/src/main.rs::SharedLlmRouter",
    "crates/apollia-runtime/src/api/server.rs::SharedLlmRouter",
    "crates/apollia-runtime/src/api/server.rs::SharedSttEngine",
    "crates/apollia-runtime/src/api/server.rs::SharedSttRepository",
    "crates/apollia-runtime/src/session_metrics.rs::SessionMetricsStore",
    "crates/apollia-workspace/src/assembler.rs::SnapshotCache",
}

# Per-file counts of test bodies that hang their verdict on something the
# machine controls: a sleep, a port, an external process, a wall-clock
# deadline. The counted forms are listed on `rule_time_sensitive_tests`.
#
# The entries are not equal, and the table does not pretend they are. Three
# families sit in it:
#
#  * the transport is the subject. `apollia-tools` executors, the MCP stdio
#    transport, `subprocess_env`, `subprocess_window`: they exist to prove a
#    child process is spawned and answers. A spawn cannot be removed from a
#    test of spawning.
#  * the clock is the subject. `apollia-triggers` cron, interval, file-watch
#    and oneshot sources, `inactivity_watcher`, `retry`, `resilience`: what
#    they assert is that something fires after a delay.
#  * the dependency is incidental. This is the debt. A test whose subject is a
#    decision, an ordering or a composition, and that reaches for a port or a
#    process to observe it, answers about the machine as often as about the
#    code. `shutdown.rs` and `hooks/executor.rs` were both in this family and
#    both cost a red verdict on a green tree; each lost the sites it did not
#    need, which is why their entries here are lower than the tree once was.
#
# `test_support.rs` is the reservation helper itself: its four sites are the
# probe listener and the poll interval that every other test borrows.
#
# An entry is lowered in the commit that removes the site, like every other
# ratchet here. A new site above the entry is red and has to be argued.
TIME_SENSITIVE_TEST_COUNTS: dict[str, int] = {
    "crates/apollia-aip/src/context.rs": 2,
    "crates/apollia-auth/src/callback.rs": 2,
    "crates/apollia-auth/src/mcp_oauth.rs": 1,
    "crates/apollia-auth/src/mcp_oauth_orchestrator.rs": 2,
    "crates/apollia-cli/src/commands/agent/tests.rs": 1,
    "crates/apollia-cli/src/commands/eval.rs": 1,
    "crates/apollia-cli/src/commands/start.rs": 1,
    "crates/apollia-core/src/subprocess_env.rs": 3,
    "crates/apollia-core/src/subprocess_window.rs": 2,
    "crates/apollia-llm/src/meta_orchestrator.rs": 1,
    "crates/apollia-llm/src/repository.rs": 1,
    "crates/apollia-llm/src/retry.rs": 1,
    "crates/apollia-mcp/src/transport/http.rs": 3,
    "crates/apollia-mcp/src/transport/sse.rs": 3,
    "crates/apollia-mcp/src/transport/stdio.rs": 1,
    "crates/apollia-notifications/src/channels/webhook.rs": 8,
    "crates/apollia-notifications/src/engine.rs": 1,
    "crates/apollia-notifications/src/inactivity_watcher.rs": 5,
    "crates/apollia-oria/src/actor.rs": 4,
    "crates/apollia-oria/src/budget.rs": 5,
    "crates/apollia-oria/src/engine.rs": 10,
    "crates/apollia-oria/src/resilience.rs": 3,
    "crates/apollia-runtime/src/api/server.rs": 12,
    "crates/apollia-runtime/src/chat/builtin_agent/tests.rs": 1,
    "crates/apollia-runtime/src/hooks/executor.rs": 9,
    "crates/apollia-runtime/src/llama_server/mod.rs": 1,
    "crates/apollia-runtime/src/perf_trace.rs": 2,
    "crates/apollia-runtime/src/router.rs": 2,
    "crates/apollia-runtime/src/runner_supervisor/gpu_detection.rs": 1,
    "crates/apollia-runtime/src/session_metrics.rs": 1,
    "crates/apollia-runtime/src/supervisor/tests.rs": 21,
    "crates/apollia-runtime/src/test_support.rs": 4,
    "crates/apollia-tools/src/audit.rs": 8,
    "crates/apollia-tools/src/executor.rs": 1,
    "crates/apollia-tools/src/file_path_extractor.rs": 2,
    "crates/apollia-tools/src/journal.rs": 2,
    "crates/apollia-tools/src/tools/bash_executor.rs": 2,
    "crates/apollia-tools/src/tools/http_fetch.rs": 2,
    "crates/apollia-tools/src/tools/rlimits.rs": 2,
    "crates/apollia-tools/src/tools/web_read/mod.rs": 1,
    "crates/apollia-tools/src/tools/web_search/brave.rs": 1,
    "crates/apollia-tools/src/tools/web_search/duckduckgo.rs": 1,
    "crates/apollia-triggers/src/definition_repository.rs": 1,
    "crates/apollia-triggers/src/engine.rs": 14,
    "crates/apollia-triggers/src/sources/cron.rs": 1,
    "crates/apollia-triggers/src/sources/file_watch.rs": 8,
    "crates/apollia-triggers/src/sources/interval.rs": 1,
    "crates/apollia-triggers/src/sources/mod.rs": 1,
    "crates/apollia-triggers/src/sources/oneshot.rs": 2,
}

# Empty since the prose rule extension reworded the last waived message
# (the persistor lag warning). A new entry here is a defect this guard
# exists to refuse; the table stays so the rule keeps its two-way shape.
INTERNAL_REFS_WAIVED: dict[str, int] = {}

# ── source loading, on the guard's classification ────────────────────────────


@dataclass
class Source:
    path: str
    crate: str
    raw: str
    raw_lines: list[str]
    masked: str  # comments and strings blanked, tests kept
    code: str  # comments, strings and test regions blanked
    prod_lines: set[int]  # 1-based lines outside test regions


def build_source(path: str, raw: str) -> Source:
    masked = guard.blank_comments_and_strings(raw)
    regions = guard.test_regions(masked)
    pieces, cursor = [], 0
    for start, end in regions:
        pieces.append(masked[cursor:start])
        pieces.append("".join("\n" if c == "\n" else " " for c in masked[start:end]))
        cursor = end
    pieces.append(masked[cursor:])
    code = "".join(pieces)
    offsets = guard._line_offsets(masked)
    lines = raw.split("\n")
    starts = [r[0] for r in regions]
    prod_lines: set[int] = set()
    for idx, off in enumerate(offsets):
        line = lines[idx] if idx < len(lines) else ""
        probe = off + len(line) - len(line.lstrip())
        slot = bisect_right(starts, probe) - 1
        if slot >= 0 and probe < regions[slot][1]:
            continue
        prod_lines.add(idx + 1)
    crate = path.split("/")[1] if path.startswith("crates/") else path
    return Source(path, crate, raw, lines, masked, code, prod_lines)


def load() -> list[Source]:
    paths = guard.tracked_sources()
    if not paths:
        return []
    cache: dict[str, str | None] = {}

    def read(path: str) -> str | None:
        if path not in cache:
            target = REPO_ROOT / path
            cache[path] = (
                target.read_text(encoding="utf-8", errors="replace") if target.is_file() else None
            )
        return cache[path]

    excluded = guard.excluded_modules(paths, read)
    out: list[Source] = []
    for path in paths:
        if path in excluded:
            continue
        raw = read(path)
        if raw is None:
            continue
        out.append(build_source(path, raw))
    return out


def line_of(text: str, offset: int) -> int:
    return text.count("\n", 0, offset) + 1


def _exempted(s: Source, n: int, marker: str) -> bool:
    """True when `marker` sits on line `n` or in the contiguous block of
    comment and attribute lines directly above it. A blank line breaks it."""
    if marker in s.raw_lines[n - 1]:
        return True
    k = n - 2
    while k >= 0:
        line = s.raw_lines[k].strip()
        if not line or not line.startswith(("//", "#[", "*", "/*")):
            break
        if marker in line:
            return True
        k -= 1
    return False


# ── strict rules ─────────────────────────────────────────────────────────────


def rule_panic_macros(sources):
    pat = re.compile(r"(?<![A-Za-z0-9_:])(panic|todo|unimplemented)!\s*[\(\[\{]")
    hits = []
    for s in sources:
        for m in pat.finditer(s.code):
            hits.append(f"{s.path}:{line_of(s.code, m.start())}: {m.group(1)}! in production")
    return hits, {}


def rule_print_macros(sources):
    pat = re.compile(r"(?<![A-Za-z0-9_:])(println|eprintln|print|eprint|dbg)!\s*\(")
    hits, reasoned = [], []
    for s in sources:
        if s.crate == "apollia-cli":
            continue  # user-facing output is this binary's job
        for m in pat.finditer(s.code):
            n = line_of(s.code, m.start())
            entry = f"{s.path}:{n}: {m.group(1)}! outside apollia-cli"
            if _exempted(s, n, "REASON"):
                reasoned.append(entry)
            else:
                hits.append(entry)
    return hits, {"outside apollia-cli with a REASON comment (aside)": reasoned}


def rule_anyhow(sources):
    hits = []
    for manifest in sorted((REPO_ROOT / "crates").glob("*/Cargo.toml")):
        for i, line in enumerate(manifest.read_text(encoding="utf-8").split("\n"), 1):
            if re.match(r"^\s*anyhow\s*=", line):
                hits.append(f"{manifest.relative_to(REPO_ROOT)}:{i}: dependency: {line.strip()}")
    pat = re.compile(r"(?<![A-Za-z0-9_])anyhow(?:::|!)")
    for s in sources:
        for m in pat.finditer(s.code):
            n = line_of(s.code, m.start())
            hits.append(f"{s.path}:{n}: {s.raw_lines[n - 1].strip()[:100]}")
    return hits, {}


_TRACING_MACRO = re.compile(
    r"(?<![A-Za-z0-9_])(?:tracing::)?(trace|debug|info|warn|error|event)!\s*\("
)
_PLACEHOLDER = re.compile(r"(?<!\{)\{(?!\{)[^{}]*\}")


def _split_top_level(text: str) -> list[tuple[int, str]]:
    parts, depth, start = [], 0, 0
    for i, ch in enumerate(text):
        if ch in "([{":
            depth += 1
        elif ch in ")]}":
            depth -= 1
        elif ch == "," and depth == 0:
            parts.append((start, text[start:i]))
            start = i + 1
    parts.append((start, text[start:]))
    return parts


def rule_tracing_format(sources):
    """A placeholder in the message string of a tracing macro call.

    The message is the first top-level argument that is a string literal and
    is not a `key = value` field nor a `target:`/`parent:`/`name:` option.
    """
    hits, aside = [], []
    for s in sources:
        for m in _TRACING_MACRO.finditer(s.code):
            open_at = m.end() - 1
            depth, j = 0, open_at
            while j < len(s.code):
                if s.code[j] == "(":
                    depth += 1
                elif s.code[j] == ")":
                    depth -= 1
                    if depth == 0:
                        break
                j += 1
            body_masked = s.code[open_at + 1 : j]
            body_raw = s.raw[open_at + 1 : j]
            if body_masked.strip() == "" and body_raw.strip() != "":
                continue  # the call itself sat in a test region
            for off, arg_masked in _split_top_level(body_masked):
                arg_raw = body_raw[off : off + len(arg_masked)]
                stripped = arg_raw.strip()
                if not stripped:
                    continue
                if re.match(r"^(target|parent|name)\s*:", stripped):
                    continue
                if re.match(r"^Level\s*::", stripped):
                    continue
                if re.match(r"^[A-Za-z_][A-Za-z0-9_.]*\s*=", stripped):
                    if "format!(" in stripped:
                        n = line_of(s.code, open_at + 1 + off)
                        aside.append(f"{s.path}:{n}: field built with format!: {stripped[:90]}")
                    continue
                if stripped.startswith(('"', 'r"', "r#")):
                    if _PLACEHOLDER.search(stripped):
                        n = line_of(s.code, open_at + 1 + off)
                        hits.append(f"{s.path}:{n}: {m.group(1)}!: {stripped[:100]}")
                    break
                break  # positional expression, message built elsewhere
    return hits, {"fields built with format!(..) (aside)": aside}


def rule_wildcard_reexport(sources):
    pat = re.compile(r"(?m)^[ \t]*pub(?:\s*\([^)]*\))?[ \t]+use[ \t]+[^;\n]*::\*[ \t]*;")
    plain = re.compile(r"(?m)^[ \t]*use[ \t]+[^;\n]*::\*[ \t]*;")
    hits, aside = [], []
    for s in sources:
        for m in pat.finditer(s.code):
            n = line_of(s.code, m.start())
            hits.append(f"{s.path}:{n}: {s.raw_lines[n - 1].strip()}")
        for m in plain.finditer(s.code):
            n = line_of(s.code, m.start())
            aside.append(f"{s.path}:{n}: {s.raw_lines[n - 1].strip()}")
    return hits, {"plain glob imports, not re-exports (aside)": aside}


# Proper nouns allowed to open an #[error] message with a capital.
_PROPER = re.compile(
    r"Python|Tokio|Rust|Tauri|Windows|Linux|Keychain|Keyring|Whisper|Llama|Brave|Google|"
    r"Microsoft|Gmail|Outlook|Notion|Git|Docker|Cargo|Apollia|Ollama|Anthropic|OpenAI|"
    r"JSON|YAML|TOML|HTTP|SQL|MCP|OAuth|CSRF|URL|UTF|GGUF|API|SDK|A2A|STT|LLM|CLI"
)
_ERROR_CAP = re.compile(r'^\s*#\[error\("([A-Z][a-z]+)\b')
_ERROR_PERIOD = re.compile(r'^\s*#\[error\("[^"]*\."\)\]')


def rule_error_msg_style(sources):
    hits = []
    for s in sources:
        for i, line in enumerate(s.raw_lines):
            if "#[error(" not in line or (i + 1) not in s.prod_lines:
                continue
            m = _ERROR_CAP.match(line)
            if m and not _PROPER.match(m.group(1)):
                hits.append(f"{s.path}:{i + 1}: capitalised: {line.strip()[:90]}")
            if _ERROR_PERIOD.match(line):
                hits.append(f"{s.path}:{i + 1}: trailing period: {line.strip()[:90]}")
    return hits, {}


def rule_unsafe_safety(sources):
    """Every `unsafe`, production or test, carries a SAFETY comment."""
    pat = re.compile(r"(?<![A-Za-z0-9_])unsafe\s*(\{|fn|impl|extern)")
    hits, ok = [], []
    for s in sources:
        for m in pat.finditer(s.masked):
            n = line_of(s.masked, m.start())
            kind = "prod" if n in s.prod_lines else "test"
            entry = f"{s.path}:{n}: [{kind}] unsafe {m.group(1)}: {s.raw_lines[n - 1].strip()[:80]}"
            (ok if _exempted(s, n, "SAFETY") else hits).append(entry)
    return hits, {"unsafe with a SAFETY comment (aside)": ok}


_UNBOUNDED = re.compile(
    r"mpsc::unbounded_channel|UnboundedSender|UnboundedReceiver"
    r"|(?<![A-Za-z0-9_])thread::sleep\(|FuturesUnordered<"
)


def rule_unbounded(sources):
    hits = []
    for s in sources:
        for m in _UNBOUNDED.finditer(s.code):
            hits.append(f"{s.path}:{line_of(s.code, m.start())}: {m.group(0)} in production")
    return hits, {}


# Tracker identifiers and numbered planning vocabulary. Assembled so this
# file never carries a matchable form of its own rule.
_INTERNAL = re.compile(
    r"(?<![A-Za-z0-9])ADR-?\d{2,3}(?![0-9])"
    r"|(?<![A-Za-z0-9])(?:CAP|LOT|GRP|STORY)-\d{2,3}(?![0-9])"
    r"|\b(?:Lot|LOT|Sprint|SPRINT|Batch|Story|STORY|Epic|EPIC|Vague|Chantier)\s?\d+\b"
    r"|(?i:\bsprint\b|\buser[ ]story\b|\bepic\b)"
)


def rule_internal_refs(sources):
    found: Counter = Counter()
    detail: dict[str, list[str]] = {}
    for s in sources:
        for n in sorted(s.prod_lines):
            line = s.raw_lines[n - 1]
            for m in _INTERNAL.finditer(line):
                found[s.path] += 1
                detail.setdefault(s.path, []).append(
                    f"{s.path}:{n}: {m.group(0)!r} in: {line.strip()[:100]}"
                )
    hits = []
    for path, count in sorted(found.items()):
        allowed = INTERNAL_REFS_WAIVED.get(path, 0)
        if count > allowed:
            hits.extend(detail[path])
    for path, allowed in sorted(INTERNAL_REFS_WAIVED.items()):
        if found.get(path, 0) < allowed:
            hits.append(
                f"{path}: waived for {allowed} planning reference(s), found "
                f"{found.get(path, 0)}. The debt went down: shrink "
                f"INTERNAL_REFS_WAIVED in this same commit"
            )
    return hits, {}


_ENUM_DECL = re.compile(r"(?m)^((?:\s*#\[[^\n]*\]\s*\n)+)\s*pub\s+enum\s+([A-Za-z0-9_]+)")


def rule_error_enums(sources):
    hits = []
    for s in sources:
        for m in _ENUM_DECL.finditer(s.raw):
            attrs, name = m.group(1), m.group(2)
            if "Error" not in attrs and not name.endswith("Error"):
                continue
            if not re.search(r"derive\([^)]*Error", attrs):
                continue
            n = line_of(s.raw, m.start(2))
            if n not in s.prod_lines:
                continue
            if "non_exhaustive" not in attrs:
                hits.append(
                    f"{s.path}:{n}: pub enum {name} derives Error without "
                    f"#[non_exhaustive]. Adding a variant later is a breaking "
                    f"change for every caller that matches on it"
                )
    return hits, {}


# ── ratchet rules ────────────────────────────────────────────────────────────


def _two_sided(found: dict[str, int], table: dict[str, int], label: str) -> list[str]:
    hits = []
    for path, count in sorted(found.items()):
        allowed = table.get(path, 0)
        if count > allowed:
            hits.append(
                f"{path}: {count} {label} against a table entry of {allowed}. "
                f"Remove the new site, or move the debt into the table in the "
                f"same commit, knowingly"
            )
    for path, allowed in sorted(table.items()):
        if found.get(path, 0) < allowed:
            hits.append(
                f"{path}: table says {allowed} {label}, found {found.get(path, 0)}. "
                f"The debt went down: lower the table entry in this same commit"
            )
    return hits


_ASYNC_ATTR = re.compile(r"#\[\s*(?:async_trait::)?async_trait\s*\]")
_TRAIT_DECL = re.compile(r"^\s*(?:pub(?:\s*\([^)]*\))?\s+)?(?:unsafe\s+)?trait\s+([A-Za-z0-9_]+)")


def rule_async_trait(sources):
    hits = []
    for manifest in sorted((REPO_ROOT / "crates").glob("*/Cargo.toml")):
        rel = str(manifest.relative_to(REPO_ROOT))
        declares = any(
            re.match(r"^\s*async[-_]trait\s*=", line)
            for line in manifest.read_text(encoding="utf-8").split("\n")
        )
        if declares and rel not in ASYNC_TRAIT_MANIFESTS:
            hits.append(f"{rel}: new async-trait dependency; new traits use RPITIT instead")
        if not declares and rel in ASYNC_TRAIT_MANIFESTS:
            hits.append(
                f"{rel}: listed in ASYNC_TRAIT_MANIFESTS but no longer declares the "
                f"dependency. Remove the entry in this same commit"
            )
    seen: set[str] = set()
    for s in sources:
        for m in _ASYNC_ATTR.finditer(s.code):
            # the next non-attribute code line decides: trait or impl
            n = line_of(s.code, m.start())
            k = n
            while k < len(s.raw_lines):
                candidate = s.raw_lines[k].strip()
                k += 1
                if not candidate or candidate.startswith(("#[", "//")):
                    continue
                decl = _TRAIT_DECL.match(candidate)
                if decl:
                    key = f"{s.path}::{decl.group(1)}"
                    seen.add(key)
                    if key not in ASYNC_TRAIT_TRAITS:
                        hits.append(
                            f"{s.path}:{n}: #[async_trait] on new trait {decl.group(1)}; "
                            f"use return-position impl Trait (RPITIT)"
                        )
                break
    for key in sorted(ASYNC_TRAIT_TRAITS - seen):
        hits.append(
            f"{key}: listed in ASYNC_TRAIT_TRAITS but no #[async_trait] trait found. "
            f"Remove the entry in this same commit"
        )
    return hits, {}


def rule_module_size(sources):
    hits, aside = [], []
    for s in sources:
        prod = len(s.prod_lines)
        over = prod > MODULE_SIZE_THRESHOLD
        if over and s.path not in MODULE_SIZE_FILES:
            hits.append(
                f"{s.path}: {prod} production lines (threshold {MODULE_SIZE_THRESHOLD}). "
                f"Split the module"
            )
        elif not over and s.path in MODULE_SIZE_FILES:
            hits.append(
                f"{s.path}: listed in MODULE_SIZE_FILES but now at {prod} production "
                f"lines. The debt went down: remove the entry in this same commit"
            )
        elif over:
            aside.append(f"{s.path}: {prod} production lines, listed")
    missing = MODULE_SIZE_FILES - {s.path for s in sources}
    for path in sorted(missing):
        hits.append(
            f"{path}: listed in MODULE_SIZE_FILES but absent from the inventory. "
            f"Remove the entry in this same commit"
        )
    return hits, {"listed files still over the threshold (aside)": aside}


# `Arc<` followed by a path whose last segment names a lock type. The path is
# matched as one flat token run and split afterwards: a nested quantifier here
# backtracks for seconds on every `Arc<` that wraps something else.
_ARC_WRAP = re.compile(r"Arc<\s*([A-Za-z0-9_:]+)<")
# Horizontal whitespace only: `\s` crosses the newlines of blanked test
# regions and turns the line anchor quadratic on large files.
_TYPE_ALIAS = re.compile(
    r"(?m)^[ \t]*(?:pub(?:\([^)]*\))?[ \t]+)?type[ \t]+([A-Za-z0-9_]+)[ \t]*="
    r"[ \t]*(?:\n[ \t]*)?Arc<[ \t]*([A-Za-z0-9_:]+)<"
)


def _is_lock_path(path: str) -> bool:
    tail = path.rsplit("::", 1)[-1]
    return tail.endswith(("Mutex", "RwLock"))


def rule_arc_mutex(sources):
    found: Counter = Counter()
    aliases: set[str] = set()
    for s in sources:
        for m in _ARC_WRAP.finditer(s.code):
            if _is_lock_path(m.group(1)):
                found[s.path] += 1
        for m in _TYPE_ALIAS.finditer(s.code):
            if _is_lock_path(m.group(2)):
                aliases.add(f"{s.path}::{m.group(1)}")
    hits = _two_sided(dict(found), ARC_MUTEX_COUNTS, "Arc<Mutex|RwLock> site(s)")
    for alias in sorted(aliases - ARC_MUTEX_ALIASES):
        hits.append(f"{alias}: new type alias wrapping Arc<Mutex|RwLock>")
    for alias in sorted(ARC_MUTEX_ALIASES - aliases):
        hits.append(
            f"{alias}: listed in ARC_MUTEX_ALIASES but no longer declared. "
            f"Remove the entry in this same commit"
        )
    return hits, {}


# A sleep, however it is spelled. `start_paused` makes some of these virtual,
# and the table carries them all the same: the attribute sits on the function,
# the sleep sits in the body, and a guard that had to pair them would be
# guessing at every helper the body calls.
_TEST_SLEEP = re.compile(
    r"(?<![A-Za-z0-9_])(?:tokio::)?time::sleep\s*\("
    r"|(?<![A-Za-z0-9_])thread::sleep\s*\("
    r"|(?<![A-Za-z0-9_])sleep\s*\(\s*Duration"
)
# A port the test picks, reserves, or binds. Every one of these is a number
# the operating system may hand to somebody else.
_TEST_PORT = re.compile(r"reserve_port\s*\(|TcpListener::bind\s*\(|UdpSocket::bind\s*\(")
# An external process the test starts itself.
_TEST_SPAWN = re.compile(r"(?<![A-Za-z0-9_])Command::new\s*\(")
# A wall-clock deadline the test arms.
_TEST_WALL_TIMEOUT = re.compile(
    r"(?<![A-Za-z0-9_])(?:tokio::)?time::timeout\s*\(|(?<![A-Za-z0-9_])timeout\s*\(\s*Duration"
)
# A script the test writes for production code to execute. The spawn is then
# invisible to `_TEST_SPAWN`, since it happens inside the code under test; the
# shebang is what gives the test away.
_TEST_SHEBANG = re.compile(r"#!/(?:bin|usr)/")

_TIME_SENSITIVE_FORMS = {
    "sleep": _TEST_SLEEP,
    "port": _TEST_PORT,
    "spawn": _TEST_SPAWN,
    "wall-timeout": _TEST_WALL_TIMEOUT,
}


def load_tests() -> list[tuple[Source, set[int]]]:
    """Every tracked file, paired with the lines that sit in a test region.

    [`load`] drops the files a `#[cfg(test)] mod` declares, because the rules
    it feeds judge production code. This one judges the tests, so those files
    come back in with every line counted. Without that, the twenty-one port
    reservations of `supervisor/tests.rs` are invisible to a rule written to
    find exactly them.

    Inventory boundary: `crates/*/src/*.rs`, the same as every other rule
    here. A per-crate `tests/` directory is out of reach and stays out of the
    table.
    """
    paths = guard.tracked_sources()
    if not paths:
        return []
    cache: dict[str, str | None] = {}

    def read(path: str) -> str | None:
        if path not in cache:
            target = REPO_ROOT / path
            cache[path] = (
                target.read_text(encoding="utf-8", errors="replace") if target.is_file() else None
            )
        return cache[path]

    excluded = guard.excluded_modules(paths, read)
    out: list[tuple[Source, set[int]]] = []
    for path in paths:
        raw = read(path)
        if raw is None:
            continue
        s = build_source(path, raw)
        every = set(range(1, len(s.raw_lines) + 1))
        out.append((s, every if path in excluded else every - s.prod_lines))
    return out


def _time_sensitive_counts(
    entries: list[tuple[Source, set[int]]],
) -> tuple[Counter, Counter, dict[str, list[str]]]:
    per_file: Counter = Counter()
    per_form: Counter = Counter()
    detail: dict[str, list[str]] = {}
    for s, test_lines in entries:
        def record(n: int, form: str) -> None:
            per_file[s.path] += 1
            per_form[form] += 1
            detail.setdefault(s.path, []).append(
                f"{s.path}:{n}: [{form}] {s.raw_lines[n - 1].strip()[:80]}"
            )

        for form, pat in _TIME_SENSITIVE_FORMS.items():
            for m in pat.finditer(s.masked):
                n = line_of(s.masked, m.start())
                if n in test_lines:
                    record(n, form)
        # the shebang lives in a string literal, so it is read from the raw text
        for n in sorted(test_lines):
            if n <= len(s.raw_lines) and _TEST_SHEBANG.search(s.raw_lines[n - 1]):
                record(n, "shebang")
    return per_file, per_form, detail


def rule_time_sensitive_tests(sources, entries=None):
    """Test bodies whose verdict depends on something the machine controls.

    A sleep, a port, an external process, a wall-clock deadline, or a script
    written for the code under test to execute. None of these is forbidden;
    each one is a reason a test can answer differently on the same tree, so
    each one is counted and frozen.

    The rule exists because two such tests crossed a whole release campaign
    unseen. `shutdown.rs` reserved a port, released it, and asked the server
    to bind the same number: replayed four times on one commit, the guard
    answered green, green, red, red. `hooks/executor.rs` proved "the first
    Deny wins" by spawning two shell scripts, and read `Allow` whenever the
    machine refused to fork. Neither was a regression, and neither could be
    told from one.
    """
    if entries is None:
        entries = load_tests()
    per_file, per_form, detail = _time_sensitive_counts(entries)
    hits = _two_sided(dict(per_file), TIME_SENSITIVE_TEST_COUNTS, "time-sensitive test site(s)")
    asides = {f"{form} site(s) (aside)": [] for form in sorted(per_form)}
    for path in sorted(detail):
        for entry in detail[path]:
            form = entry.split("[", 1)[1].split("]", 1)[0]
            asides[f"{form} site(s) (aside)"].append(entry)
    return hits, asides


RULES = {
    "panic-macros": rule_panic_macros,
    "print-macros": rule_print_macros,
    "anyhow": rule_anyhow,
    "tracing-format": rule_tracing_format,
    "wildcard-reexport": rule_wildcard_reexport,
    "error-msg-style": rule_error_msg_style,
    "unsafe-safety": rule_unsafe_safety,
    "unbounded": rule_unbounded,
    "internal-refs": rule_internal_refs,
    "async-trait": rule_async_trait,
    "module-size": rule_module_size,
    "arc-mutex": rule_arc_mutex,
    "error-enums": rule_error_enums,
    "time-sensitive-tests": rule_time_sensitive_tests,
}


# ── selftest ─────────────────────────────────────────────────────────────────
# Every rule is fed a sample carrying the forbidden form in production next to
# the same form under a test gate, in a string, or behind its exemption; the
# rule must count exactly the production site. A rule that was never seen red
# proves nothing by being green. The samples that would themselves match the
# planning-vocabulary rules are assembled at run time so this guard passes the
# rules it enforces.


def _sample(text: str, path: str = "crates/apollia-x/src/lib.rs") -> Source:
    return build_source(path, text)


def _selftest() -> int:
    failures = 0

    def control(name: str, hits: list[str], expected: int) -> None:
        nonlocal failures
        ok = len(hits) == expected
        print(f"{'ok ' if ok else 'RED'}  {name}: {len(hits)} site(s), expected {expected}")
        if not ok:
            failures += 1
            for h in hits:
                print(f"       {h}")

    def strip_tree(hits: list[str]) -> list[str]:
        # the manifest-reading rules also report the real tree's manifests
        return [h for h in hits if h.startswith("crates/apollia-x/")]

    # 1. panic macros: production fires, test module and string stay silent
    hits, _ = rule_panic_macros(
        [
            _sample(
                'fn a() { panic!("x"); }\n'
                "#[cfg(test)]\nmod t { fn b() { todo!(); } }\n"
                'const S: &str = "unimplemented!()";\n'
            )
        ]
    )
    control("panic-macros", hits, 1)

    # 2. print macros: bare fires, REASON stays, apollia-cli is allowed
    hits, _ = rule_print_macros(
        [
            _sample(
                'fn a() { println!("x"); }\n'
                '// REASON: last line before exit\nfn b() { eprintln!("y"); }\n'
            ),
            _sample('fn c() { println!("z"); }\n', "crates/apollia-cli/src/lib.rs"),
        ]
    )
    control("print-macros", hits, 1)

    # 3. anyhow: a path in production fires, a comment stays silent
    hits, _ = rule_anyhow(
        [_sample("fn a() -> anyhow::Result<()> { Ok(()) }\n// anyhow::bail in a comment\n")]
    )
    control("anyhow", strip_tree(hits), 1)

    # 4. tracing format strings: two placeholders fire, fields and tests stay
    hits, _ = rule_tracing_format(
        [
            _sample(
                'fn a() { tracing::info!(agent = %x, "task.done"); }\n'
                'fn b() { tracing::warn!(k = 1, "bad {}", x); }\n'
                'fn c() { info!(target: "t", "v={v}"); }\n'
                '#[cfg(test)]\nmod t { fn d() { tracing::error!("{}", x); } }\n'
            )
        ]
    )
    control("tracing-format", hits, 2)

    # 5. wildcard re-exports: pub and pub(crate) fire, a plain glob import stays
    hits, _ = rule_wildcard_reexport(
        [_sample("pub use internal::*;\npub(crate) use other::*;\nuse pyo3::prelude::*;\n")]
    )
    control("wildcard-reexport", hits, 2)

    # 6. error message style: capitalised common word and trailing period fire,
    # a proper noun and a clean message stay
    hits, _ = rule_error_msg_style(
        [
            _sample(
                "#[derive(Debug, thiserror::Error)]\n"
                "pub enum EError {\n"
                '    #[error("Invalid input: {0}")] A(String),\n'
                '    #[error("something failed.")] B,\n'
                '    #[error("Python interpreter missing")] C,\n'
                '    #[error("failed to open {0}")] D(String),\n'
                "}\n"
            )
        ]
    )
    control("error-msg-style", hits, 2)

    # 7. unsafe in production without SAFETY fires, with SAFETY stays
    hits, _ = rule_unsafe_safety(
        [_sample("// SAFETY: fine\nfn a() { unsafe { x() } }\nfn b() { unsafe { y() } }\n")]
    )
    control("unsafe-safety production", hits, 1)

    # 8. unsafe in a test region is judged too
    hits, _ = rule_unsafe_safety(
        [
            _sample(
                "#[cfg(test)]\nmod t {\n"
                "    fn a() { unsafe { x() } }\n"
                "    // SAFETY: test env var\n"
                "    fn b() { unsafe { y() } }\n"
                "}\n"
            )
        ]
    )
    control("unsafe-safety test", hits, 1)

    # 9. unbounded forms fire in production, not under test
    hits, _ = rule_unbounded(
        [
            _sample(
                "fn a() { let (tx, rx) = mpsc::unbounded_channel(); }\n"
                "#[cfg(test)]\nmod t { fn b() { std::thread::sleep(d); } }\n"
            )
        ]
    )
    control("unbounded", hits, 1)

    # 10. planning vocabulary fires in production code, and the waiver holds
    # exactly its count; the sample is assembled so this file carries none
    ref = "LOT" + "-042"
    hits, _ = rule_internal_refs(
        [_sample(f'fn a() {{ let s = "see {ref} for the plan"; }}\n')]
    )
    control("internal-refs", strip_tree(hits), 1)

    # 11. a new #[async_trait] trait fires, an impl of a listed trait stays
    hits, _ = rule_async_trait(
        [
            _sample(
                "#[async_trait]\npub trait Fresh { async fn f(&self); }\n"
                "#[async_trait]\nimpl McpTransport for X {}\n"
            )
        ]
    )
    control("async-trait", strip_tree(hits), 1)

    # 12. a module over the threshold fires when unlisted
    hits, _ = rule_module_size([_sample("fn a() {}\n" * 801)])
    control("module-size", strip_tree(hits), 1)

    # 13. the same size under test gating stays silent
    mixed = "fn a() {}\n" * 401 + "#[cfg(test)]\nmod t {\n" + "fn b() {}\n" * 398 + "}\n"
    hits, _ = rule_module_size([_sample(mixed)])
    control("module-size tests excluded", strip_tree(hits), 0)

    # 14. an Arc<Mutex> in a file the table does not carry fires
    hits, _ = rule_arc_mutex(
        [_sample("struct A { m: Arc<Mutex<u8>>, r: Arc<tokio::sync::RwLock<u8>>, s: Arc<u8> }\n")]
    )
    control("arc-mutex new site", strip_tree(hits), 1)

    # 15. a public thiserror enum without #[non_exhaustive] fires; the same
    # enum carrying the attribute, a non-error enum and a test-gated one stay
    hits, _ = rule_error_enums(
        [
            _sample(
                "#[derive(Debug, thiserror::Error)]\n"
                "pub enum OpenError { A }\n"
                "#[derive(Debug, thiserror::Error)]\n"
                "#[non_exhaustive]\n"
                "pub enum SealedError { B }\n"
                "#[derive(Debug, Clone)]\n"
                "pub enum Colour { Red }\n"
                "#[cfg(test)]\n"
                "mod t {\n"
                "    #[derive(Debug, thiserror::Error)]\n"
                "    pub enum FixtureError { C }\n"
                "}\n"
            )
        ]
    )
    control("error-enums", hits, 1)

    # 16. the ratchet is two-sided: an entry the tree no longer carries fires
    stale = _two_sided({}, {"crates/apollia-x/src/lib.rs": 2}, "site(s)")
    control("ratchet two-sided", stale, 1)

    # 17. the five time-sensitive forms are counted in a test region, and the
    # same forms in production are not: a production sleep or spawn is the
    # product working, not a test hanging its verdict on the machine
    sample = _sample(
        "fn prod() {\n"
        "    tokio::time::sleep(d).await;\n"
        "    let c = Command::new(exe);\n"
        "}\n"
        "#[cfg(test)]\n"
        "mod t {\n"
        "    #[tokio::test]\n"
        "    async fn a() {\n"
        "        tokio::time::sleep(d).await;\n"
        "        let p = reserve_port();\n"
        "        let c = Command::new(exe);\n"
        "        let _ = tokio::time::timeout(d, f).await;\n"
        '        write(path, "#!/bin/sh\\nprintf x\\n");\n'
        "    }\n"
        "}\n"
    )
    every = set(range(1, len(sample.raw_lines) + 1))
    per_file, per_form, _ = _time_sensitive_counts([(sample, every - sample.prod_lines)])
    control("time-sensitive forms counted", list(per_form.elements()), 5)
    control(
        "time-sensitive production ignored",
        [f"{k}:{v}" for k, v in per_file.items() if v != 5],
        0,
    )

    # 18. a test-only module is judged whole: `load()` drops it, and the
    # twenty-one port reservations of the runtime's supervisor tests are
    # invisible without this
    whole = _sample(
        "fn a() { let p = reserve_port(); }\n", "crates/apollia-x/src/supervisor/tests.rs"
    )
    _, per_form_whole, _ = _time_sensitive_counts([(whole, {1})])
    control("time-sensitive whole test file", list(per_form_whole.elements()), 1)

    # 19. the rule is red on a file the table does not carry
    hits, _ = rule_time_sensitive_tests([], entries=[(whole, {1})])
    control("time-sensitive new site", strip_tree(hits), 1)

    if failures:
        print(f"\n{failures} control(s) failed", file=sys.stderr)
        return 1
    print("\nevery control holds")
    return 0


# ── entry point ──────────────────────────────────────────────────────────────


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "names", nargs="*", metavar="rule", help="rule name(s) to run (default: every rule)"
    )
    parser.add_argument(
        "--list", action="store_true", help="print every hit instead of the first eight"
    )
    parser.add_argument(
        "--selftest", action="store_true", help="replay the fixture controls instead of measuring the tree"
    )
    args = parser.parse_args(argv[1:])
    if args.selftest:
        return _selftest()
    names = args.names
    if names and any(n not in RULES for n in names):
        print(__doc__)
        return 2
    listing = args.list
    sources = load()
    if not sources:
        print("nothing measured: no tracked production file", file=sys.stderr)
        return 2
    print(f"production files scanned: {len(sources)}")
    worst = 0
    for name in names or list(RULES):
        hits, asides = RULES[name](sources)
        print(f"\n== {name}: {len(hits)} finding(s)")
        for h in hits if listing else hits[:8]:
            print(f"  {h}")
        if not listing and len(hits) > 8:
            print(f"  ... {len(hits) - 8} more (--list)")
        for label, items in asides.items():
            print(f"  -- {label}: {len(items)}")
            if listing:
                for h in items:
                    print(f"     {h}")
        worst = max(worst, 1 if hits else 0)
    if worst == 0:
        print("\nevery mechanisable Rust rule holds")
    return worst


if __name__ == "__main__":
    sys.exit(main(sys.argv))
