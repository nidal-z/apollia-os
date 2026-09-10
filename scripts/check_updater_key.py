#!/usr/bin/env python3
"""The updater signature must come from the key the bundle trusts.

Why this exists as a check and not as care. `plugins.updater.pubkey` in
tauri.conf.json and the private key held in the repository secrets are two
values nothing ties together. A rotation moves one and leaves the other, in
silence, and the result is a bundle that refuses every update it is ever
offered. It cannot be repaired remotely: the channel that would carry the
repair is the one that is broken.

It happened twice on 2026-09-10 alone, once because a rotation on 2026-09-01
never reached this file, once because the key was regenerated the same day. Both
were caught by reading the key id out of a published signature by hand, which
nobody does by habit.

Usage: check_updater_key.py <tauri.conf.json> <signature> [<signature> ...]

A signature is a Tauri updater `.sig` file. Exit 0 when every one of them was
produced by the key the configuration declares, 1 otherwise.
"""

import base64
import json
import sys
from pathlib import Path


def key_id_from_pubkey(raw: str) -> str:
    """The key id a Tauri `pubkey` value names, as minisign writes it."""
    text = base64.b64decode(raw).decode("utf-8", "replace")
    comment = text.splitlines()[0]
    return comment.rsplit(": ", 1)[-1].strip().upper()


def key_id_from_signature(raw: str) -> str:
    """The key id that produced a Tauri `.sig`.

    The file is base64 of a minisign signature file, whose second line is the
    base64 signature itself: two bytes of algorithm, then the key id in little
    endian.
    """
    inner = base64.b64decode(raw)
    signature = base64.b64decode(inner.split(b"\n")[1])
    return signature[2:10][::-1].hex().upper()


def main() -> int:
    if len(sys.argv) < 3:
        print(__doc__, file=sys.stderr)
        return 2

    config = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
    declared = key_id_from_pubkey(config["plugins"]["updater"]["pubkey"])

    failures = []
    for path in sys.argv[2:]:
        try:
            produced = key_id_from_signature(Path(path).read_text(encoding="utf-8"))
        except (ValueError, IndexError) as exc:
            failures.append(f"{path}: unreadable as a signature ({exc})")
            continue
        verdict = "ok" if produced == declared else "MISMATCH"
        print(f"  {verdict:8} {produced}  {path}")
        if produced != declared:
            failures.append(f"{path}: signed by {produced}, bundle trusts {declared}")

    print(f"bundle trusts {declared}, {len(sys.argv) - 2} signature(s) read")
    if failures:
        print(
            "\nThe bundle would refuse every update it is offered, and the failure\n"
            "cannot be repaired remotely. Align plugins.updater.pubkey with the\n"
            "key the repository secret holds, or set the secret to the key the\n"
            "configuration declares.",
            file=sys.stderr,
        )
        for line in failures:
            print(f"  {line}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
