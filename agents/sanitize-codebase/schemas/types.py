"""Pilier 1, Typed payloads.

TypedDict canonicals for the A2A skill inputs/outputs. We intentionally
avoid `from __future__ import annotations` (PEP 563 breaks
`TypedDict.__required_keys__` which the Apollia SDK introspects when it
builds the JSON Schema descriptor for LLM-facing tool catalogs).
"""

from typing import Any, Literal, TypedDict


# ---------------------------------------------------------------------------
# Manifest entry (persisted in ctx.memory under key "file:{path}")
# ---------------------------------------------------------------------------


class FileEntry(TypedDict, total=False):
    """Per-file progress record kept in the SQLite memory namespace."""

    path: str                          # repo-relative path
    language: Literal["rust", "python", "typescript", "svelte", "markdown"]
    crate: str                          # for cargo -p / pnpm filter
    state: Literal[
        "pending", "code_clean", "comments_clean", "done", "residue"
    ]
    sha_before: str                    # file SHA at last touch (12 hex)
    sha_after: str
    sonar_issues_before: int
    sonar_issues_after: int
    last_attempt_ts: str               # ISO-8601
    attempts: int
    last_error: str                    # populated when state == "residue"
    last_reason: str                   # human-readable cause (e.g. "harness_red")


# ---------------------------------------------------------------------------
# SonarQube issue (subset of MCP search_issues output we rely on)
# ---------------------------------------------------------------------------


class SonarIssue(TypedDict, total=False):
    key: str
    rule: str           # e.g. "rust:S3776"
    component: str
    line: int
    message: str
    severity: str       # "INFO" / "MINOR" / "MAJOR" / "CRITICAL" / "BLOCKER"
    type: str           # "BUG" / "CODE_SMELL" / "VULNERABILITY"
    status: str


# ---------------------------------------------------------------------------
# Harness report (cargo build / ruff / pnpm)
# ---------------------------------------------------------------------------


class HarnessReport(TypedDict):
    command: str
    exit_code: int
    stdout_tail: str
    stderr_tail: str
    elapsed_secs: float
    green: bool


# ---------------------------------------------------------------------------
# A2A skill IO, sanitize.clean_file
# ---------------------------------------------------------------------------


class CleanFileInput(TypedDict, total=False):
    path: str
    language: Literal["rust", "python", "typescript", "svelte"]
    crate: str
    max_issues: int


class CleanFileResult(TypedDict, total=False):
    path: str
    state: Literal["code_clean", "residue"]
    issues_before: int
    issues_after: int
    issues_fixed: list[dict[str, Any]]
    harness: HarnessReport
    error: str


# ---------------------------------------------------------------------------
# A2A skill IO, sanitize.translate_comments
# ---------------------------------------------------------------------------


class TranslateCommentsInput(TypedDict, total=False):
    path: str
    language: Literal["rust", "python", "typescript", "svelte"]
    dry_run: bool


class TranslateCommentsResult(TypedDict, total=False):
    path: str
    state: Literal["comments_clean", "residue"]
    comments_seen: int
    comments_rewritten: int
    em_dash_removed: int
    internal_refs_neutralized: int
    diff_only_touches_comments: bool
    error: str
