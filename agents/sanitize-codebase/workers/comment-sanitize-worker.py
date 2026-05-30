"""comment-sanitize-worker v0.1.0, Translate / normalize comment blocks.

Skill: sanitize.translate_comments (path, language, dry_run).

The worker:
1. Reads the file.
2. Parses comment blocks per language (line and block forms).
3. For each non-empty comment block, asks the local LLM to rewrite it
   following the constraints in templates/comment-translate-prompt.md.j2
   (FR -> EN, em-dash stripping, internal refs neutralization, preserve
   the why).
4. Reassembles the file. Validates that the diff touches only comment
   lines: the set of non-comment lines must be byte-identical to the
   input. Otherwise -> residue with `diff_only_touches_comments=False`.
5. Writes the new content via file_write.

Stdlib-only parser per language. No tree-sitter dependency.
"""

import json
import re
import sys
from pathlib import Path
from typing import Any

import jinja2
import yaml

from apollia import DomainError, agent, skill
from apollia.types import Ctx


def _capture_agent_dir() -> Path:
    if sys.path:
        candidate = Path(sys.path[0])
        if candidate.is_absolute() and candidate.exists():
            return candidate
    return Path(__file__).resolve().parent


_WORKER_DIR: Path = _capture_agent_dir()


def _agent_root() -> Path:
    current = _WORKER_DIR
    for _ in range(4):
        if (current / "manifest.toml").exists():
            return current
        current = current.parent
    return _WORKER_DIR.parent


_AGENT_ROOT: Path = _agent_root()


@agent(
    name="comment-sanitize-worker",
    version="0.1.0",
    description=(
        "Worker A2A. Rewrites comments per file: FR -> EN, em-dash strip, "
        "internal refs neutralization, preserve the why. Code tokens untouched."
    ),
    tags=("worker", "comments", "i18n", "fr-en"),
    agent_type="worker",
    tools_required=("file_read", "file_write"),
    packages=("jinja2", "pyyaml"),
    step_budget={"max_steps": 20, "max_tool_calls": 40, "wall_clock_secs": 600},
)
class CommentSanitizeWorker:
    """A2A worker that rewrites comments and validates code-only invariance."""

    @skill(
        "sanitize.translate_comments",
        description=(
            "Rewrite comments in English, strip em-dash, neutralize internal "
            "references. Touches only comment lines."
        ),
        examples=[
            {
                "path": "sdk/apollia/types.py",
                "language": "python",
                "dry_run": False,
            }
        ],
    )
    async def translate_comments(
        self,
        path: str,
        language: str,
        dry_run: bool = False,
        ctx: Ctx = None,
    ) -> dict[str, Any]:
        if ctx is None:
            raise DomainError("NO_CTX", "Runtime context required")
        if ctx.llm is None:
            raise DomainError("NO_LLM", "Local LLM backend required")

        exclusions = _load_yaml(_AGENT_ROOT / "datasources" / "exclusions.yaml")
        forbidden = [item["char"] for item in exclusions.get("forbidden_chars", [])]
        internal_refs = exclusions.get("internal_refs", [])

        # 1. Read the file.
        try:
            file_read = await ctx.tools.call("file_read", {"path": path})
            original = file_read.get("content", "")
        except Exception as e:
            return {
                "path": path,
                "state": "residue",
                "error": f"file_read_failed: {e}",
            }
        if not original:
            return {
                "path": path,
                "state": "residue",
                "error": "empty_file",
                "comments_seen": 0,
                "comments_rewritten": 0,
            }

        lines = original.splitlines(keepends=True)
        blocks = _extract_comment_blocks(lines, language)
        comments_seen = len(blocks)
        if comments_seen == 0:
            return {
                "path": path,
                "state": "comments_clean",
                "comments_seen": 0,
                "comments_rewritten": 0,
                "em_dash_removed": 0,
                "internal_refs_neutralized": 0,
                "diff_only_touches_comments": True,
            }

        # 2. Rewrite each block.
        rewritten_blocks: list[tuple[int, int, list[str]]] = []
        em_dash_total = 0
        refs_total = 0
        rewritten_count = 0

        for start, end, body in blocks:
            block_text = "".join(body)
            if not _block_needs_rewrite(block_text, forbidden, internal_refs):
                continue
            prompt = self._render_template(
                "comment-translate-prompt.md.j2",
                path=path,
                language=language,
                comment_block=block_text,
                forbidden_chars=forbidden,
                internal_refs=internal_refs,
            )
            resp = await ctx.llm.chat(prompt, block_text[:4000])
            payload = _extract_json(self._llm_text(resp)) or {}
            new_block = payload.get("rewritten")
            if not isinstance(new_block, str) or not new_block.strip():
                # Apply mechanical fallback.
                new_block = _mechanical_rewrite(block_text, forbidden, internal_refs)
            actions = payload.get("actions") or {}
            em_dash_total += int(actions.get("em_dash_removed", 0))
            refs_total += int(actions.get("internal_refs_neutralized", 0))
            # Preserve final newline shape.
            new_body = new_block.splitlines(keepends=True)
            if body and body[-1].endswith("\n") and new_body and not new_body[-1].endswith("\n"):
                new_body[-1] = new_body[-1] + "\n"
            rewritten_blocks.append((start, end, new_body))
            rewritten_count += 1

        # 3. Reassemble.
        new_lines = lines[:]
        for start, end, new_body in sorted(rewritten_blocks, key=lambda b: b[0], reverse=True):
            new_lines[start:end] = new_body
        new_content = "".join(new_lines)

        # 4. Code-only invariance: strip comment lines on both sides and
        # compare. If the non-comment skeleton diverges, refuse to write.
        before_code = _strip_comment_lines("".join(lines), language)
        after_code = _strip_comment_lines(new_content, language)
        diff_clean = before_code == after_code

        if not diff_clean:
            return {
                "path": path,
                "state": "residue",
                "error": "code_lines_changed",
                "comments_seen": comments_seen,
                "comments_rewritten": rewritten_count,
                "em_dash_removed": em_dash_total,
                "internal_refs_neutralized": refs_total,
                "diff_only_touches_comments": False,
            }

        # 5. Write.
        if not dry_run and new_content != original:
            await ctx.tools.call("file_write", {"path": path, "content": new_content})

        return {
            "path": path,
            "state": "comments_clean",
            "comments_seen": comments_seen,
            "comments_rewritten": rewritten_count,
            "em_dash_removed": em_dash_total,
            "internal_refs_neutralized": refs_total,
            "diff_only_touches_comments": True,
        }

    @staticmethod
    def _llm_text(resp: Any) -> str:
        if resp is None:
            return ""
        for attr in ("content", "text", "message", "output"):
            v = getattr(resp, attr, None)
            if isinstance(v, str):
                return v
        s = str(resp)
        if s.startswith("<") and "object at 0x" in s:
            return ""
        return s

    def _render_template(self, template_name: str, **vars: Any) -> str:
        env = jinja2.Environment(
            loader=jinja2.FileSystemLoader(str(_AGENT_ROOT / "templates")),
            autoescape=False,
            trim_blocks=True,
            lstrip_blocks=True,
        )
        return env.get_template(template_name).render(**vars)


# ---------------------------------------------------------------------------
# Comment parsing per language (line-level, stdlib only)
# ---------------------------------------------------------------------------


def _is_comment_line(line: str, language: str) -> bool:
    s = line.lstrip()
    if language == "rust":
        return s.startswith("//") or s.startswith("/*") or s.startswith("*")
    if language == "python":
        return s.startswith("#") or s.startswith('"""') or s.startswith("'''")
    if language in ("typescript", "svelte"):
        return s.startswith("//") or s.startswith("/*") or s.startswith("*") or s.startswith("<!--")
    return False


def _extract_comment_blocks(
    lines: list[str], language: str
) -> list[tuple[int, int, list[str]]]:
    """Return contiguous comment line ranges as (start, end_exclusive, body)."""
    blocks: list[tuple[int, int, list[str]]] = []
    n = len(lines)
    i = 0
    while i < n:
        if _is_comment_line(lines[i], language):
            start = i
            while i < n and _is_comment_line(lines[i], language):
                i += 1
            blocks.append((start, i, lines[start:i]))
        else:
            i += 1
    return blocks


def _strip_comment_lines(content: str, language: str) -> str:
    """Drop every full comment line. Used to verify code invariance."""
    out: list[str] = []
    for line in content.splitlines(keepends=True):
        if _is_comment_line(line, language):
            continue
        out.append(line)
    return "".join(out)


def _block_needs_rewrite(
    block_text: str,
    forbidden: list[str],
    internal_refs: list[dict[str, Any]],
) -> bool:
    if any(ch in block_text for ch in forbidden):
        return True
    for ref in internal_refs:
        try:
            if re.search(ref["pattern"], block_text):
                return True
        except re.error:
            continue
    # Naive FR detection, accented characters or common French words.
    if re.search(r"[éèàùçâîôû]", block_text):
        return True
    for word in (" est ", " une ", " des ", " pour ", " avec ", " sans "):
        if word in block_text:
            return True
    return False


def _mechanical_rewrite(
    block_text: str,
    forbidden: list[str],
    internal_refs: list[dict[str, Any]],
) -> str:
    """Fallback: do the deterministic replacements only (no translation)."""
    out = block_text
    # Forbidden chars from exclusions.yaml mapping.
    exclusions = _load_yaml(_AGENT_ROOT / "datasources" / "exclusions.yaml")
    for item in exclusions.get("forbidden_chars", []):
        out = out.replace(item["char"], item["replacement"])
    for ref in internal_refs:
        try:
            out = re.sub(ref["pattern"], ref["replacement"], out)
        except re.error:
            continue
    return out


def _load_yaml(path: Path) -> dict[str, Any]:
    if not path.exists():
        return {}
    try:
        return yaml.safe_load(path.read_text()) or {}
    except yaml.YAMLError:
        return {}


def _extract_json(text: str) -> dict[str, Any] | None:
    if not text:
        return None
    s = text.strip()
    s = re.sub(r"<think>.*?</think>", "", s, flags=re.DOTALL | re.IGNORECASE).strip()
    s = re.sub(r"^```(?:json)?\s*|\s*```$", "", s, flags=re.MULTILINE).strip()
    start = s.find("{")
    if start < 0:
        return None
    depth = 0
    in_string = False
    escape = False
    end = -1
    for i in range(start, len(s)):
        c = s[i]
        if escape:
            escape = False
            continue
        if c == "\\" and in_string:
            escape = True
            continue
        if c == '"':
            in_string = not in_string
            continue
        if in_string:
            continue
        if c == "{":
            depth += 1
        elif c == "}":
            depth -= 1
            if depth == 0:
                end = i
                break
    if end < 0:
        return None
    try:
        return json.loads(s[start : end + 1])
    except json.JSONDecodeError:
        return None
