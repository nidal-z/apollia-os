"""Entry point for ``python -m apollia``."""

import sys

from apollia.cli.scaffold import main


def _entry() -> None:
    """Console-scripts entry point."""
    main(sys.argv[1:])


if __name__ == "__main__":
    _entry()
