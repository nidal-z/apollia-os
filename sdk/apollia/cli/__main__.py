"""Entry point for ``python -m apollia`` and the ``apollia`` console script.

This dispatcher wires the sub-command ``describe`` onto a single
:class:`argparse.ArgumentParser`. A sub-command lives in its own module and
registers itself via ``build_parser``. A new package is written by
``apollia-os package new``, the command line of the runtime.
"""

from __future__ import annotations

import argparse
import sys
from typing import TYPE_CHECKING

from apollia.cli import describe as describe_cmd

if TYPE_CHECKING:
    from collections.abc import Sequence


def _build_parser() -> argparse.ArgumentParser:
    """Build the top-level ``apollia`` parser with every sub-command wired."""
    parser = argparse.ArgumentParser(
        prog="apollia",
        description="Apollia SDK - tools and workflows of a package",
    )
    subparsers = parser.add_subparsers(dest="command")

    # `apollia describe` - write the tool and workflow descriptors of a package.
    describe_cmd.build_parser(subparsers)

    return parser


def main(argv: Sequence[str] | None = None) -> int:
    """Top-level dispatcher. Returns the process exit code.

    Args:
        argv: Optional argument list. Uses ``sys.argv[1:]`` when ``None``.
    """
    parser = _build_parser()
    args = parser.parse_args(list(argv) if argv is not None else sys.argv[1:])

    if args.command is None:
        parser.print_help()
        return 0

    func = getattr(args, "func", None)
    if func is None:
        parser.print_help()
        return 0

    result = func(args)
    if isinstance(result, int):
        return result
    return 0


def _entry() -> None:
    """Console-scripts entry point declared in ``pyproject.toml``."""
    sys.exit(main(sys.argv[1:]))


if __name__ == "__main__":
    _entry()
