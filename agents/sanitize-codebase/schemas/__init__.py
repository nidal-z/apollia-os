"""Schemas for sanitize-codebase agents.

Re-exports for convenient `from schemas import FileEntry, ...` imports.
Do not add `from __future__ import annotations` here (PEP 563 breaks
TypedDict.__required_keys__ which the SDK uses to build A2A descriptors).
"""

from schemas.state import SanitizeStep
from schemas.types import (
    CleanFileInput,
    CleanFileResult,
    FileEntry,
    HarnessReport,
    SonarIssue,
    TranslateCommentsInput,
    TranslateCommentsResult,
)

__all__ = [
    "SanitizeStep",
    "FileEntry",
    "SonarIssue",
    "HarnessReport",
    "CleanFileInput",
    "CleanFileResult",
    "TranslateCommentsInput",
    "TranslateCommentsResult",
]
