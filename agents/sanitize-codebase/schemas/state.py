"""Pilier 2, Steps fonctionnels.

State machine of the sanitize-codebase director. Each batch run walks
through a deterministic sequence and persists progress in the SQLite
namespace memory.

INIT
  -> LOAD_DATASOURCES        load rules.yaml, pool.yaml, exclusions.yaml
  -> LOAD_MANIFEST           reload per-file progress entries from memory
  -> PICK_BATCH              select the next N files in state "pending" or "residue"
  -> DISPATCH_SONAR          A2A sanitize.clean_file per file (or skip if already code_clean)
  -> DISPATCH_COMMENTS       A2A sanitize.translate_comments per file
  -> VERIFY_HARNESS          bash_executor: cargo build / ruff check / pnpm check per language
  -> PERSIST_PROGRESS        write file:{path} entries back to memory + episode + procedure
  -> NOTIFY                  desktop notification of batch summary
  -> DONE
"""

from __future__ import annotations

from enum import Enum


class SanitizeStep(str, Enum):
    """State machine steps for the sanitize-codebase director."""

    INIT = "INIT"
    LOAD_DATASOURCES = "LOAD_DATASOURCES"
    LOAD_MANIFEST = "LOAD_MANIFEST"
    PICK_BATCH = "PICK_BATCH"
    DISPATCH_SONAR = "DISPATCH_SONAR"
    DISPATCH_COMMENTS = "DISPATCH_COMMENTS"
    VERIFY_HARNESS = "VERIFY_HARNESS"
    PERSIST_PROGRESS = "PERSIST_PROGRESS"
    NOTIFY = "NOTIFY"
    DONE = "DONE"
