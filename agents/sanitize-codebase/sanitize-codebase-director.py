"""sanitize-codebase v0.1.0, Director L2 mirror of the Claude sanitize pass.

Phase 2 of the public-repo sanitize (cf. docs/internal/release/REPO-STATE.md
section 6.3 and plan-release.md section G). The director walks the source
tree file by file, dispatches two A2A workers (sonar-cleanup-worker then
comment-sanitize-worker), gates each touched file through the language
harness (cargo / ruff / pnpm), and persists per-file progress in the
namespace SQLite memory so that interval triggers can resume after a
restart.

This agent is the storytelling hero of the v0.1.0-preview launch:
"Apollia OS reproduced a Claude-quality refactor of its own codebase,
locally, for 0 euro." The mechanics live in README.md, the operator setup
lives in SETUP.md, the replay-and-compare procedure lives in eval/replay.md.

Four pillars:
  - Pilier 1 (Templates) : templates/ Jinja2 + schemas/types.py TypedDict.
  - Pilier 2 (Steps)     : state machine 8 steps in schemas/state.py.
  - Pilier 3 (Datasources) : datasources/ rules.yaml + pool.yaml + exclusions.yaml.
  - Pilier 4 (Memoire)   : keys file:{path}, seen:{sha}, procedural "sanitize-batch".
"""

# Absolute imports only (PyO3 PyModule::from_code has no package context).
# Forced sys.modules purge before re-import: PyO3 reloads the director on
# every task; submodules cached in sys.modules would shadow updated copies
# pushed by `apollia agent install`.

import fnmatch
import hashlib
import json
import re
import sys
from datetime import datetime
from pathlib import Path
from typing import Any

import jinja2
import yaml

from apollia import DomainError, agent, on_message, skill
from apollia.types import Ctx, Message


def _capture_agent_dir() -> Path:
    """Snap the agent directory at module import time.

    `PyModule::from_code` sets `__file__` to the bare filename, so
    `Path(__file__).parent` is unusable. The Rust loader prepends the
    absolute parent path to `sys.path[0]` right before executing this
    module, which is reliable only at import time. A later agent load can
    shadow `sys.path[0]`, so we memoize once here.
    """
    if sys.path:
        candidate = Path(sys.path[0])
        if candidate.is_absolute() and candidate.exists():
            return candidate
    return Path(__file__).resolve().parent


_AGENT_DIR: Path = _capture_agent_dir()


def _agent_dir() -> Path:
    return _AGENT_DIR


# Purge submodules then re-import, see file header.
for _local_mod_name in ("schemas", "schemas.state", "schemas.types"):
    if _local_mod_name in sys.modules:
        del sys.modules[_local_mod_name]

# Make `schemas/` importable as a top-level package from the agent dir.
_SCHEMAS_DIR = str(_agent_dir())
if _SCHEMAS_DIR not in sys.path:
    sys.path.insert(0, _SCHEMAS_DIR)

from schemas.state import SanitizeStep  # type: ignore[import-not-found]  # noqa: E402
from schemas.types import (  # type: ignore[import-not-found]  # noqa: E402
    FileEntry,
    HarnessReport,
)


# ---------------------------------------------------------------------------
# Director
# ---------------------------------------------------------------------------


@agent(
    name="sanitize-codebase-director",
    version="0.1.0",
    description=(
        "Director L2 Apollia OS that mirrors the Claude sanitize pass. "
        "Iterates over the codebase, dispatches sonar then comment workers, "
        "gates via the language harness, persists per-file progress in memory."
    ),
    tags=("sanitize", "self-refactor", "i18n", "sonar", "release-v0.1.0"),
    memory_namespace="sanitize-codebase",
    tools_required=("file_read", "file_write", "bash_executor"),
    packages=("jinja2", "pyyaml"),
    step_budget={"max_steps": 50, "max_tool_calls": 200, "wall_clock_secs": 1800},
)
class SanitizeCodebaseDirector:
    """Mirror director, runs one batch per invocation, idempotent across triggers."""

    SYSTEM_PROMPT = """You are sanitize-codebase-director.

You orchestrate a behavior-preserving sanitize pass over the Apollia OS
codebase, mirroring the Claude pass. You delegate to A2A workers for
edits and only call the LLM as a fallback. The harness gate (cargo /
ruff / pnpm) is non-negotiable: any red harness reverts the file to
"residue" with a reason.
"""

    # ============================================================
    # Entry points
    # ============================================================

    @skill(
        "sanitize.run_batch",
        description=(
            "Pick the next batch of files from the pool, dispatch sonar then "
            "comment workers, gate via build/lint, persist progress."
        ),
        examples=[
            {"prompt": "Run the next sanitize batch", "batch_size": 4},
        ],
    )
    async def run_batch(
        self,
        prompt: str = "",
        batch_size: int = 0,
        ctx: Ctx = None,
    ) -> dict[str, Any]:
        """Execute one batch of the sanitize state machine."""
        if ctx is None:
            raise DomainError("NO_CTX", "Runtime context required")

        start_ts = datetime.now()
        state: dict[str, Any] = {
            "step": SanitizeStep.INIT,
            "today": start_ts.strftime("%Y-%m-%d"),
            "data": {
                "prompt": prompt,
                "batch_size_override": batch_size or 0,
            },
            "progress": [],
            "errors": [],
            "metrics": {
                "tool_calls": 0,
                "llm_calls": 0,
                "wall_clock_start": start_ts.timestamp(),
            },
        }

        max_iterations = 20
        iteration = 0
        while state["step"] != SanitizeStep.DONE:
            if iteration >= max_iterations:
                raise DomainError(
                    "STATE_LOOP",
                    f"Max iterations reached. Progress: {state['progress']}",
                    details={"errors": state["errors"]},
                )
            iteration += 1
            try:
                state = await self._dispatch(state, ctx)
            except Exception as e:  # noqa: BLE001, state machine recovery
                ctx.logger.error(
                    "step failed", extra={"step": state["step"].value, "error": str(e)}
                )
                state["errors"].append({"step": state["step"].value, "error": str(e)})
                state["step"] = self._error_recovery(state["step"])

        state["metrics"]["wall_clock_secs"] = round(
            datetime.now().timestamp() - state["metrics"]["wall_clock_start"], 1
        )

        return {
            "today": state["today"],
            "progress": state["progress"],
            "errors": state["errors"],
            "metrics": state["metrics"],
            "summary": state["data"].get("rendered_summary", ""),
            "batch": state["data"].get("batch_paths", []),
            "counts": state["data"].get("counts", {}),
        }

    @on_message
    async def on_chat(
        self,
        message: str,
        history: list[Message],
        ctx: Ctx,
    ) -> str:
        """Conversational + interval trigger entrypoint, forwards to run_batch."""
        result = await self.run_batch(prompt=message or "", ctx=ctx)
        summary = result.get("summary") or ""
        if summary:
            return summary
        return (
            f"[sanitize-codebase] batch done: "
            f"{result['counts']} (errors={len(result['errors'])})"
        )

    # ============================================================
    # Dispatch
    # ============================================================

    async def _dispatch(self, state: dict, ctx: Any) -> dict:
        handlers = {
            SanitizeStep.INIT: self._step_init,
            SanitizeStep.LOAD_DATASOURCES: self._step_load_datasources,
            SanitizeStep.LOAD_MANIFEST: self._step_load_manifest,
            SanitizeStep.PICK_BATCH: self._step_pick_batch,
            SanitizeStep.DISPATCH_SONAR: self._step_dispatch_sonar,
            SanitizeStep.DISPATCH_COMMENTS: self._step_dispatch_comments,
            SanitizeStep.VERIFY_HARNESS: self._step_verify_harness,
            SanitizeStep.PERSIST_PROGRESS: self._step_persist_progress,
            SanitizeStep.NOTIFY: self._step_notify,
        }
        handler = handlers.get(state["step"])
        if handler is None:
            raise RuntimeError(f"No handler for step {state['step'].value}")
        return await handler(state, ctx)

    def _error_recovery(self, current: SanitizeStep) -> SanitizeStep:
        """Skippable steps continue forward; non-skippable abandon."""
        transitions = {
            SanitizeStep.LOAD_MANIFEST: SanitizeStep.PICK_BATCH,
            SanitizeStep.DISPATCH_SONAR: SanitizeStep.DISPATCH_COMMENTS,
            SanitizeStep.DISPATCH_COMMENTS: SanitizeStep.VERIFY_HARNESS,
            SanitizeStep.VERIFY_HARNESS: SanitizeStep.PERSIST_PROGRESS,
            SanitizeStep.PERSIST_PROGRESS: SanitizeStep.NOTIFY,
            SanitizeStep.NOTIFY: SanitizeStep.DONE,
        }
        return transitions.get(current, SanitizeStep.DONE)

    # ============================================================
    # Steps
    # ============================================================

    async def _step_init(self, state: dict, ctx: Any) -> dict:
        ctx.logger.info(f"sanitize-codebase batch started: {state['today']}")
        state["progress"].append("INIT ok")
        state["step"] = SanitizeStep.LOAD_DATASOURCES
        return state

    async def _step_load_datasources(self, state: dict, ctx: Any) -> dict:
        rules = await self._load_yaml(ctx, "Sanitize Rules", "datasources/rules.yaml")
        pool = await self._load_yaml(ctx, "Sanitize Pool", "datasources/pool.yaml")
        exclusions = await self._load_yaml(
            ctx, "Sanitize Exclusions", "datasources/exclusions.yaml"
        )
        state["data"]["rules"] = rules
        state["data"]["pool"] = pool
        state["data"]["exclusions"] = exclusions
        state["progress"].append(
            f"DATASOURCES ok ({len(pool.get('groups', []))} groups, "
            f"{len(rules.get('languages', {}))} languages)"
        )
        state["step"] = SanitizeStep.LOAD_MANIFEST
        return state

    async def _step_load_manifest(self, state: dict, ctx: Any) -> dict:
        """Rehydrate per-file progress from the namespace memory."""
        manifest: dict[str, FileEntry] = {}
        if ctx.memory is None:
            state["data"]["manifest"] = manifest
            state["progress"].append("LOAD_MANIFEST skip (no memory)")
            state["step"] = SanitizeStep.PICK_BATCH
            return state

        try:
            entries = await ctx.memory.search("file:", limit=2000)
            for r in entries:
                key = r.get("key", "")
                if not key.startswith("file:"):
                    continue
                try:
                    payload: FileEntry = json.loads(r.get("value", "{}"))
                except json.JSONDecodeError:
                    continue
                manifest[key] = payload
        except Exception as e:  # noqa: BLE001
            ctx.logger.warning(f"memory search file: failed: {e}")

        state["data"]["manifest"] = manifest
        state["progress"].append(f"LOAD_MANIFEST ok ({len(manifest)} entries)")
        state["step"] = SanitizeStep.PICK_BATCH
        return state

    async def _step_pick_batch(self, state: dict, ctx: Any) -> dict:
        """Walk the pool, register new candidates, then select up to N to work on."""
        manifest: dict[str, FileEntry] = state["data"]["manifest"]
        pool = state["data"]["pool"]
        exclusions = state["data"]["exclusions"]
        repo_root = Path(pool.get("repo_root", ".")).resolve()
        batch_size = (
            state["data"]["batch_size_override"]
            or int(pool.get("batch_size", 4))
        )

        do_not_touch = exclusions.get("do_not_touch", [])

        # 1. Discover candidates (cheap globbing, capped).
        discovered = 0
        for group in pool.get("groups", []):
            language = group.get("language", "rust")
            includes = group.get("includes", [])
            excludes = group.get("excludes", [])
            for pat in includes:
                for path in repo_root.glob(pat):
                    rel = str(path.relative_to(repo_root))
                    if self._is_excluded(rel, excludes + do_not_touch):
                        continue
                    key = f"file:{rel}"
                    if key in manifest:
                        continue
                    crate = (
                        self._extract_crate(rel)
                        if group.get("crate_from_path")
                        else group.get("crate", language)
                    )
                    entry: FileEntry = {
                        "path": rel,
                        "language": language,
                        "crate": crate,
                        "state": "pending",
                        "attempts": 0,
                    }
                    manifest[key] = entry
                    discovered += 1
                    if discovered >= 5000:  # safety bound
                        break

        # 2. Pick batch: prefer "pending", then "residue" with attempts < 3.
        pending = [
            entry for entry in manifest.values()
            if entry.get("state") == "pending"
        ]
        residue = [
            entry for entry in manifest.values()
            if entry.get("state") == "residue" and int(entry.get("attempts", 0)) < 3
        ]
        batch = (pending + residue)[:batch_size]

        state["data"]["batch"] = batch
        state["data"]["batch_paths"] = [e["path"] for e in batch]
        state["progress"].append(
            f"PICK_BATCH ok (discovered+{discovered}, batch={len(batch)})"
        )
        state["step"] = (
            SanitizeStep.DISPATCH_SONAR if batch else SanitizeStep.NOTIFY
        )
        return state

    async def _step_dispatch_sonar(self, state: dict, ctx: Any) -> dict:
        """Dispatch sanitize.clean_file per file, A2A first, no fallback."""
        batch: list[FileEntry] = state["data"]["batch"]
        results: dict[str, dict[str, Any]] = {}
        for entry in batch:
            path = entry["path"]
            if entry.get("state") in ("code_clean", "comments_clean", "done"):
                results[path] = {"path": path, "state": entry["state"], "skipped": True}
                continue
            try:
                result = await ctx.a2a.invoke(
                    "sanitize.clean_file",
                    {
                        "path": path,
                        "language": entry["language"],
                        "crate": entry.get("crate", entry["language"]),
                        "max_issues": 5,
                    },
                    timeout_secs=900,
                )
                state["metrics"]["tool_calls"] += 1
                payload = self._unwrap_a2a(result)
                results[path] = payload or {
                    "path": path, "state": "residue", "error": "empty_payload"
                }
                # Update manifest in place.
                new_state = (payload or {}).get("state", "residue")
                if new_state in ("code_clean", "done"):
                    entry["state"] = new_state
                else:
                    entry["state"] = "residue"
                    entry["last_reason"] = (payload or {}).get(
                        "error", "sonar_residue"
                    )
                entry["attempts"] = int(entry.get("attempts", 0)) + 1
                entry["last_attempt_ts"] = datetime.now().isoformat()
                entry["sonar_issues_before"] = (payload or {}).get("issues_before", 0)
                entry["sonar_issues_after"] = (payload or {}).get("issues_after", 0)
            except Exception as e:  # noqa: BLE001
                ctx.logger.warning(f"sanitize.clean_file failed for {path}: {e}")
                results[path] = {"path": path, "state": "residue", "error": str(e)}
                entry["state"] = "residue"
                entry["last_reason"] = f"sonar_dispatch_failed: {e}"
                entry["attempts"] = int(entry.get("attempts", 0)) + 1
        state["data"]["sonar_results"] = results
        state["progress"].append(f"DISPATCH_SONAR ok ({len(results)} files)")
        state["step"] = SanitizeStep.DISPATCH_COMMENTS
        return state

    async def _step_dispatch_comments(self, state: dict, ctx: Any) -> dict:
        """Dispatch sanitize.translate_comments per file."""
        batch: list[FileEntry] = state["data"]["batch"]
        results: dict[str, dict[str, Any]] = {}
        for entry in batch:
            path = entry["path"]
            # Skip if already done or hard residue from sonar.
            if entry.get("state") == "done":
                results[path] = {"path": path, "state": "done", "skipped": True}
                continue
            try:
                result = await ctx.a2a.invoke(
                    "sanitize.translate_comments",
                    {
                        "path": path,
                        "language": entry["language"],
                        "dry_run": False,
                    },
                    timeout_secs=600,
                )
                state["metrics"]["tool_calls"] += 1
                payload = self._unwrap_a2a(result)
                results[path] = payload or {
                    "path": path, "state": "residue", "error": "empty_payload"
                }
                if (payload or {}).get("state") == "comments_clean":
                    entry["state"] = (
                        "done" if entry.get("state") == "code_clean"
                        else "comments_clean"
                    )
                else:
                    entry["state"] = "residue"
                    entry["last_reason"] = (payload or {}).get(
                        "error", "comments_residue"
                    )
                entry["attempts"] = int(entry.get("attempts", 0)) + 1
                entry["last_attempt_ts"] = datetime.now().isoformat()
            except Exception as e:  # noqa: BLE001
                ctx.logger.warning(
                    f"sanitize.translate_comments failed for {path}: {e}"
                )
                results[path] = {"path": path, "state": "residue", "error": str(e)}
                entry["state"] = "residue"
                entry["last_reason"] = f"comments_dispatch_failed: {e}"
        state["data"]["comment_results"] = results
        state["progress"].append(f"DISPATCH_COMMENTS ok ({len(results)} files)")
        state["step"] = SanitizeStep.VERIFY_HARNESS
        return state

    async def _step_verify_harness(self, state: dict, ctx: Any) -> dict:
        """Run the language harness per touched crate; revert state on red."""
        batch: list[FileEntry] = state["data"]["batch"]
        rules = state["data"]["rules"].get("languages", {})

        # Group by (language, crate) to avoid repeated cargo runs.
        groups: dict[tuple[str, str], list[FileEntry]] = {}
        for entry in batch:
            if entry.get("state") not in ("code_clean", "comments_clean", "done"):
                continue
            key = (entry["language"], entry.get("crate", entry["language"]))
            groups.setdefault(key, []).append(entry)

        harness_results: list[HarnessReport] = []
        for (language, crate), entries in groups.items():
            lang_rules = rules.get(language, {})
            cmd_tpl = lang_rules.get("harness", "")
            if not cmd_tpl:
                continue
            # Per-file substitution is fine: cargo cares about crate; ruff
            # cares about path. We pick the first entry's path as anchor.
            cmd = cmd_tpl.format(crate=crate, path=entries[0]["path"])
            try:
                bash_result = await ctx.tools.call(
                    "bash_executor",
                    {"command": cmd, "timeout_secs": 600, "cwd": "."},
                )
                state["metrics"]["tool_calls"] += 1
                exit_code = int(bash_result.get("exit_code", 1))
                report: HarnessReport = {
                    "command": cmd,
                    "exit_code": exit_code,
                    "stdout_tail": (bash_result.get("stdout") or "")[-1000:],
                    "stderr_tail": (bash_result.get("stderr") or "")[-1000:],
                    "elapsed_secs": float(bash_result.get("elapsed_secs", 0.0)),
                    "green": exit_code == 0,
                }
                harness_results.append(report)
                if not report["green"]:
                    for entry in entries:
                        entry["state"] = "residue"
                        entry["last_reason"] = f"harness_red: {cmd}"
            except Exception as e:  # noqa: BLE001
                ctx.logger.warning(f"harness failed for {language}/{crate}: {e}")
                for entry in entries:
                    entry["state"] = "residue"
                    entry["last_reason"] = f"harness_exception: {e}"

        state["data"]["harness_results"] = harness_results
        state["progress"].append(
            f"VERIFY_HARNESS ok ({len(harness_results)} runs, "
            f"{sum(1 for r in harness_results if r['green'])} green)"
        )
        state["step"] = SanitizeStep.PERSIST_PROGRESS
        return state

    async def _step_persist_progress(self, state: dict, ctx: Any) -> dict:
        """Persist per-file entries + episode + procedural memory."""
        if ctx.memory is None:
            state["progress"].append("PERSIST skip (no memory)")
            state["step"] = SanitizeStep.NOTIFY
            return state

        batch: list[FileEntry] = state["data"]["batch"]
        for entry in batch:
            key = f"file:{entry['path']}"
            await ctx.memory.remember(
                key, json.dumps(entry, ensure_ascii=False), confidence=0.95
            )
            # seen:{sha} also so we can detect external edits between ticks.
            sha = entry.get("sha_after") or entry.get("sha_before")
            if sha:
                await ctx.memory.remember(
                    f"seen:{sha}",
                    json.dumps({"path": entry["path"], "ts": entry.get("last_attempt_ts")}),
                    confidence=1.0,
                )

        await ctx.memory.record(
            content=(
                f"Sanitize batch {state['today']} on {len(batch)} files; "
                f"counts={self._compute_counts(state['data']['manifest'])}"
            ),
            importance=0.6,
            metadata={"batch_size": len(batch), "today": state["today"]},
        )

        try:
            existing = await ctx.memory.recall_procedure("sanitize-batch")
            if not existing:
                await ctx.memory.learn_procedure(
                    "sanitize-batch",
                    [
                        "Load rules.yaml + pool.yaml + exclusions.yaml",
                        "Rehydrate file:* manifest from memory",
                        "Glob the pool, register new pending entries",
                        "Pick up to batch_size pending or low-attempt residue",
                        "A2A sanitize.clean_file per file (sonar pass)",
                        "A2A sanitize.translate_comments per file",
                        "Run language harness per crate, revert on red",
                        "Persist file:{path} + seen:{sha} + episode + procedure",
                        "Notify desktop with batch summary",
                    ],
                )
        except Exception as e:  # noqa: BLE001
            ctx.logger.debug(f"learn_procedure failed: {e}")

        state["progress"].append(f"PERSIST ok ({len(batch)} entries)")
        state["step"] = SanitizeStep.NOTIFY
        return state

    async def _step_notify(self, state: dict, ctx: Any) -> dict:
        manifest: dict[str, FileEntry] = state["data"].get("manifest", {})
        batch: list[FileEntry] = state["data"].get("batch", [])
        counts = self._compute_counts(manifest)
        state["data"]["counts"] = counts

        progress_done = sum(
            1 for e in manifest.values() if e.get("state") == "done"
        )
        progress_total = max(1, len(manifest))
        progress = {
            "done": progress_done,
            "total": progress_total,
            "percent": round(100.0 * progress_done / progress_total, 1),
        }

        residues = [
            {
                "path": e["path"],
                "last_reason": e.get("last_reason", "unknown"),
            }
            for e in batch
            if e.get("state") == "residue"
        ]

        rendered = self._render_template(
            "batch-summary.md.j2",
            today=state["today"],
            batch=batch,
            counts=counts,
            progress=progress,
            residues=residues,
            wall_clock_secs=round(
                datetime.now().timestamp() - state["metrics"]["wall_clock_start"], 1
            ),
            tool_calls=state["metrics"]["tool_calls"],
        )
        state["data"]["rendered_summary"] = rendered

        if ctx.notify is not None:
            severity = "warning" if residues else "info"
            try:
                await ctx.notify.publish(
                    message=rendered,
                    severity=severity,
                    title="Apollia OS sanitize-codebase",
                )
            except Exception as e:  # noqa: BLE001
                ctx.logger.warning(f"notify failed: {e}")

        state["progress"].append(f"NOTIFY ok ({len(batch)} files)")
        state["step"] = SanitizeStep.DONE
        return state

    # ============================================================
    # Helpers
    # ============================================================

    async def _load_yaml(
        self, ctx: Any, workspace_title: str, local_relative: str
    ) -> dict:
        """Priority: ctx.workspace > local YAML > {} fallback."""
        if ctx.workspace:
            ws_yaml = ctx.workspace.get(workspace_title)
            if ws_yaml:
                try:
                    return yaml.safe_load(ws_yaml) or {}
                except yaml.YAMLError as e:
                    ctx.logger.warning(
                        f"workspace YAML invalid for '{workspace_title}': {e}"
                    )
        local_path = _agent_dir() / local_relative
        if local_path.exists():
            try:
                return yaml.safe_load(local_path.read_text()) or {}
            except yaml.YAMLError as e:
                ctx.logger.warning(f"local YAML invalid {local_path}: {e}")
        return {}

    @staticmethod
    def _is_excluded(rel_path: str, patterns: list[str]) -> bool:
        for pat in patterns:
            if fnmatch.fnmatch(rel_path, pat):
                return True
        return False

    @staticmethod
    def _extract_crate(rel_path: str) -> str:
        m = re.match(r"crates/([^/]+)/", rel_path)
        return m.group(1) if m else "unknown"

    @staticmethod
    def _unwrap_a2a(result: dict[str, Any] | None) -> dict[str, Any] | None:
        """Decode the A2A envelope into the inner result dict."""
        if not isinstance(result, dict):
            return None
        if result.get("status") and result["status"] != "completed":
            return {"error": f"a2a_status:{result['status']}"}
        payload = result.get("result", result)
        if isinstance(payload, dict) and "text" in payload and isinstance(
            payload["text"], str
        ):
            try:
                return json.loads(payload["text"])
            except json.JSONDecodeError:
                return {"error": "a2a_text_not_json"}
        if isinstance(payload, dict):
            return payload
        return None

    @staticmethod
    def _compute_counts(manifest: dict[str, FileEntry]) -> dict[str, int]:
        out = {
            "pending": 0,
            "code_clean": 0,
            "comments_clean": 0,
            "done": 0,
            "residue": 0,
        }
        for entry in manifest.values():
            st = entry.get("state", "pending")
            out[st] = out.get(st, 0) + 1
        return out

    @staticmethod
    def _sha12(content: bytes) -> str:
        return hashlib.sha256(content).hexdigest()[:12]

    def _render_template(self, template_name: str, **vars: Any) -> str:
        env = jinja2.Environment(
            loader=jinja2.FileSystemLoader(str(_agent_dir() / "templates")),
            autoescape=False,
            trim_blocks=True,
            lstrip_blocks=True,
        )
        return env.get_template(template_name).render(**vars)


# Module-level `agent` singleton is exposed automatically by `@agent`.
