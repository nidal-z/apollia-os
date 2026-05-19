"""Entry point for ``python -m apollia``.

Delegates to :func:`apollia.cli.__main__.main`, which dispatches every
sub-command (``new``, ``inspect``, …) onto a single parser.
"""

from __future__ import annotations

import sys

from apollia.cli.__main__ import main

if __name__ == "__main__":
    sys.exit(main())
