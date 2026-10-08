#!/usr/bin/env bash
# Stand-in for `cargo` under `cargo tauri dev --runner`, on macOS: builds the
# application, signs the binary with a stable identity, then runs it.
#
# The keychain ties each entry to the signature of the program that wrote it.
# An unsigned dev binary has a new signature after every build, so macOS asks
# for the keychain password again, once per entry read at startup. Signed with
# one identity under one identifier, every build is the same program to the
# keychain, and "Always allow" holds.
#
# APOLLIA_DEV_SIGN_IDENTITY names the identity, a code-signing certificate of
# the login keychain. Unset, or on a command other than `run`, this script is
# `cargo` and nothing else. A signature that fails is reported and the
# application still starts.
set -euo pipefail

identity="${APOLLIA_DEV_SIGN_IDENTITY:-}"
if [ "${1:-}" != "run" ] || [ -z "$identity" ] || [ "$(uname -s)" != "Darwin" ]; then
  exec cargo "$@"
fi
shift

build_args=()
while [ "$#" -gt 0 ] && [ "$1" != "--" ]; do
  build_args+=("$1")
  shift
done
if [ "${1:-}" = "--" ]; then
  shift
fi

# The path of the binary is read from cargo itself, on a second pass that
# builds nothing: the first one shows its progress as usual.
cargo build ${build_args[@]+"${build_args[@]}"}
bin="$(
  cargo build ${build_args[@]+"${build_args[@]}"} --message-format=json 2>/dev/null |
    python3 -c '
import json, sys
found = ""
for line in sys.stdin:
    try:
        message = json.loads(line)
    except ValueError:
        continue
    if message.get("reason") == "compiler-artifact" and message.get("executable"):
        found = message["executable"]
print(found)
'
)"
if [ -z "$bin" ] || [ ! -x "$bin" ]; then
  echo "dev-sign: cargo named no binary to run" >&2
  exit 1
fi

if ! codesign --force --sign "$identity" --identifier "${APOLLIA_DEV_SIGN_IDENTIFIER:-com.apollia.desktop.dev}" "$bin" 2>/dev/null; then
  echo "dev-sign: could not sign with the identity \"$identity\"; the keychain will ask again after each build" >&2
fi
exec "$bin" "$@"
