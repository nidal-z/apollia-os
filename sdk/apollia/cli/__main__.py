"""Entry point for ``python -m apollia`` and the ``apollia`` console script.

This dispatcher wires every sub-command (``new``, ``inspect``, …) onto a
single :class:`argparse.ArgumentParser`. Each sub-command lives in its
own module and registers itself via ``build_parser``.
"""

from __future__ import annotations

import argparse
import sys
from typing import Sequence

from apollia.cli import inspect as inspect_cmd
from apollia.cli.scaffold import VALID_AGENT_TYPES, _handle_new


def _build_parser() -> argparse.ArgumentParser:
    """Build the top-level ``apollia`` parser with every sub-command wired."""
    parser = argparse.ArgumentParser(
        prog="apollia",
        description="Apollia SDK - agent development toolkit",
    )
    subparsers = parser.add_subparsers(dest="command")

    # `apollia new` - agent scaffolding (delegates to scaffold.py).
    new_parser = subparsers.add_parser(
        "new",
        help="Generate a new agent from a template",
    )
    new_parser.add_argument(
        "name",
        help="Agent name in kebab-case (e.g. 'my-agent')",
    )
    new_parser.add_argument(
        "--type",
        dest="agent_type",
        choices=VALID_AGENT_TYPES,
        default="react",
        help=(
            "Agent type (default: react). Use 'worker' for "
            "domain-specialized Worker Agents."
        ),
    )
    new_parser.add_argument(
        "--output-dir",
        default=".",
        help="Output directory (default: current directory)",
    )
    def _run_new(ns: argparse.Namespace) -> int:
        _handle_new(ns)
        return 0

    new_parser.set_defaults(func=_run_new)

    # `apollia inspect` - read-only manifest inspection.
    inspect_cmd.build_parser(subparsers)

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
