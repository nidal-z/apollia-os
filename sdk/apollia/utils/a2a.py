"""Deprecated public re-export of A2A task helpers.

These helpers used to live here as part of the public surface, but the
new public API relies on the ``@skill`` decorator + the dispatch glue in
:mod:`apollia._internal.dispatch` to route tasks transparently. Agent
authors should not import this module anymore.

For workers exposing multiple skills, the canonical pattern is to
declare one ``@skill(...)`` per operation and let the dispatch boundary
call the right handler. If you genuinely need the helpers (e.g. inside
a low-level test fixture), import them from the private module:

    from apollia._internal.a2a_utils import extract_a2a_payload, extract_skill_id

This module re-exports the same functions and emits a
:class:`DeprecationWarning` on import. It will be removed in v0.6.0
once the legacy agents under ``apollia.agents`` are retired.
"""

from __future__ import annotations

import warnings

from apollia._internal.a2a_utils import extract_a2a_payload, extract_skill_id

warnings.warn(
    "apollia.utils.a2a is deprecated and will be removed in v0.6.0. "
    "Use apollia._internal.a2a_utils (private) or rely on the @skill "
    "decorator which handles dispatch automatically.",
    DeprecationWarning,
    stacklevel=2,
)

__all__ = ["extract_a2a_payload", "extract_skill_id"]
