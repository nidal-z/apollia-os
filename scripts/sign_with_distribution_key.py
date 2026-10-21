#!/usr/bin/env python3
"""Sign files with the distribution key, each signature beside its file.

The distribution key signs what the application downloads after install and
accepts from no other key: the engine manifest, the model registry index and
the connection catalogue. The signer is `cargo tauri signer sign`, the tool
that signs the application updates, given this other key for one child
process: the key is read from `APOLLIA_DIST_SIGNING_KEY` and
`APOLLIA_DIST_SIGNING_KEY_PASSWORD` only, never from a file of the repository
and never on a command line. A run where the distribution key holds the same
value as the updater key is refused: the two are distinct on purpose.

Each `<file>` gets `<file>.minisig`, the name the application reads.

Usage:
    python3 scripts/sign_with_distribution_key.py FILE [FILE ...]
"""

import os
import pathlib
import subprocess
import sys

DIST_KEY = "APOLLIA_DIST_SIGNING_KEY"
DIST_KEY_PASSWORD = "APOLLIA_DIST_SIGNING_KEY_PASSWORD"
UPDATER_KEY = "TAURI_SIGNING_PRIVATE_KEY"
UPDATER_KEY_PATH = "TAURI_SIGNING_PRIVATE_KEY_PATH"
UPDATER_KEY_PASSWORD = "TAURI_SIGNING_PRIVATE_KEY_PASSWORD"


class Refused(Exception):
    """The file cannot be signed."""


def sign(path: pathlib.Path) -> pathlib.Path:
    key = os.environ.get(DIST_KEY, "").strip()
    if not key:
        raise Refused(f"{DIST_KEY} is not set: nothing can be signed")
    if key == os.environ.get(UPDATER_KEY, "").strip():
        raise Refused(f"{DIST_KEY} holds the same key as {UPDATER_KEY}; the distribution key is a dedicated one")
    if not path.is_file():
        raise Refused(f"{path} is not a file")
    env = {k: v for k, v in os.environ.items() if k not in (UPDATER_KEY_PATH, DIST_KEY, DIST_KEY_PASSWORD)}
    # The signer reads the key under these names, set for this one child
    # process, so the key never appears in a command line.
    env[UPDATER_KEY] = key
    env[UPDATER_KEY_PASSWORD] = os.environ.get(DIST_KEY_PASSWORD, "")
    try:
        done = subprocess.run(
            ["cargo", "tauri", "signer", "sign", str(path)],
            env=env,
            stdin=subprocess.DEVNULL,
            capture_output=True,
            text=True,
            check=False,
        )
    except OSError as err:
        raise Refused(f"`cargo tauri signer sign` could not be run: {err}") from err
    produced = path.with_name(path.name + ".sig")
    if done.returncode != 0 or not produced.is_file():
        detail = (done.stderr or done.stdout).strip().splitlines()[-1:] or ["no output"]
        raise Refused(f"`cargo tauri signer sign` failed on {path.name}: {detail[0]}")
    signature = path.with_name(path.name + ".minisig")
    produced.replace(signature)
    return signature


def main() -> int:
    files = [pathlib.Path(arg) for arg in sys.argv[1:]]
    if not files:
        sys.stderr.write("usage: sign_with_distribution_key.py FILE [FILE ...]\n")
        return 2
    try:
        for path in files:
            signature = sign(path)
            sys.stdout.write(f"signed {path.name} -> {signature.name}\n")
    except Refused as refused:
        sys.stderr.write(f"sign_with_distribution_key: refused: {refused}\n")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
