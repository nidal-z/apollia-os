#!/usr/bin/env bash
# The stable signature of the desktop in dev mode, on macOS. Sourced by the
# justfile; `dev-sign-cargo.sh` holds the reason and does the signing.

DEV_SIGN_DEFAULT_IDENTITY="Apollia Dev"

# The identity to sign the dev build with: APOLLIA_DEV_SIGN_IDENTITY when set,
# else the default one when the login keychain holds it. Empty elsewhere than
# on macOS and when no such identity exists.
dev_sign_identity() {
  [ "$(uname -s)" = "Darwin" ] || return 0
  if [ -n "${APOLLIA_DEV_SIGN_IDENTITY:-}" ]; then
    printf '%s' "$APOLLIA_DEV_SIGN_IDENTITY"
    return 0
  fi
  if security find-identity -p codesigning 2>/dev/null | grep -qF "\"$DEV_SIGN_DEFAULT_IDENTITY\""; then
    printf '%s' "$DEV_SIGN_DEFAULT_IDENTITY"
  fi
}

# `cargo tauri dev`, signed when an identity exists. Run from the desktop crate.
dev_sign_tauri_dev() {
  local lib identity
  lib="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
  identity="$(dev_sign_identity)"
  if [ -n "$identity" ]; then
    APOLLIA_DEV_SIGN_IDENTITY="$identity" cargo tauri dev --runner "$lib/dev-sign-cargo.sh" "$@"
  else
    cargo tauri dev "$@"
  fi
}

# Make the default identity: a self-signed code-signing certificate in the
# login keychain. macOS asks for the keychain password once, to store it.
dev_sign_create_identity() {
  if [ "$(uname -s)" != "Darwin" ]; then
    echo "dev-sign: only macOS ties its keychain to a signature" >&2
    return 1
  fi
  if [ -n "$(APOLLIA_DEV_SIGN_IDENTITY='' dev_sign_identity)" ]; then
    echo "dev-sign: the identity \"$DEV_SIGN_DEFAULT_IDENTITY\" already exists"
    return 0
  fi
  local tmp pass
  tmp="$(mktemp -d)"
  pass="$(/usr/bin/openssl rand -hex 16)"
  cat >"$tmp/req.cnf" <<CNF
[req]
distinguished_name = dn
x509_extensions = ext
prompt = no
[dn]
CN = $DEV_SIGN_DEFAULT_IDENTITY
[ext]
basicConstraints = critical, CA:false
keyUsage = critical, digitalSignature
extendedKeyUsage = critical, codeSigning
CNF
  /usr/bin/openssl req -x509 -newkey rsa:2048 -nodes -days 3650 \
    -config "$tmp/req.cnf" -keyout "$tmp/key.pem" -out "$tmp/cert.pem" 2>/dev/null
  /usr/bin/openssl pkcs12 -export -inkey "$tmp/key.pem" -in "$tmp/cert.pem" \
    -name "$DEV_SIGN_DEFAULT_IDENTITY" -out "$tmp/identity.p12" -passout "pass:$pass"
  security import "$tmp/identity.p12" -P "$pass" -T /usr/bin/codesign \
    -k "$HOME/Library/Keychains/login.keychain-db"
  rm -rf "$tmp"
  echo "dev-sign: the identity \"$DEV_SIGN_DEFAULT_IDENTITY\" is in the login keychain; \`just desktop-dev\` signs with it from now on"
}
