"""ctx.logger — structured logging via stdlib ``logging``.

The runtime configures the actual logger so that records are piped into
the Rust ``tracing`` subscriber.  Agents just use the standard
:class:`logging.Logger` API.
"""

from __future__ import annotations

import logging

#: Canonical type for ``ctx.logger``.
Logger = logging.Logger

__all__ = ["Logger"]
