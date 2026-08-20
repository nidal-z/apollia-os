//! TLS material the API server tests hand to rustls, and nothing else.
//!
//! It lives in a file of its own for one reason: `detect-private-key` refuses
//! every commit that touches a file carrying a private key, whatever the diff.
//! With the pair inlined in `server.rs`, that hook blocked any change to a
//! 1400-line module, and the only way through was `--no-verify`, which turns
//! off all sixteen hooks at once.
//!
//! The `.pre-commit-config.yaml` exemption is anchored on this exact path, on
//! the entry of that single hook, so every other hook still judges this file
//! and every other file still faces the private-key detector. What no guard
//! covers is the content of this file: a real secret added here would be seen
//! by nothing. Nothing but test material goes in.
//!
//! The key is a self-signed EC pair generated for `localhost`, valid to 2036,
//! and it protects nothing.

// Self-signed EC certificate + key for the TLS handshake test. Generated
// once with `openssl req -x509 -newkey ec` for CN/SAN localhost. Test-only.
pub(crate) const TEST_TLS_CERT_PEM: &str = "-----BEGIN CERTIFICATE-----\n\
MIIBmjCCAT+gAwIBAgIUQqKKeHUOBMBghGEEvDXjapDcIiQwCgYIKoZIzj0EAwIw\n\
FDESMBAGA1UEAwwJbG9jYWxob3N0MB4XDTI2MDcxNDA1MzQyOFoXDTM2MDcxMTA1\n\
MzQyOFowFDESMBAGA1UEAwwJbG9jYWxob3N0MFkwEwYHKoZIzj0CAQYIKoZIzj0D\n\
AQcDQgAEWZ0RPXZo8vEdvtyHUAAe/R0TryJmnh2fT5wTVuUMZrJVIGRVTTbfenOz\n\
XFC25yp0escLNTMuNprp7qchbrmjIaNvMG0wHQYDVR0OBBYEFCLotXp0e5i8B2vA\n\
mlHBnwgVvxn1MB8GA1UdIwQYMBaAFCLotXp0e5i8B2vAmlHBnwgVvxn1MA8GA1Ud\n\
EwEB/wQFMAMBAf8wGgYDVR0RBBMwEYcEfwAAAYIJbG9jYWxob3N0MAoGCCqGSM49\n\
BAMCA0kAMEYCIQC9y01nmYoSlWnK+uX1tqHfjMn0a+HWhRiaSN55QML6LAIhAPKM\n\
Htplqy9lO4oMS0FJXRsjbD93wxQgJHL/4YHl+Ne0\n\
-----END CERTIFICATE-----\n";

pub(crate) const TEST_TLS_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\n\
MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQgBUl6bU7cNDQoy04z\n\
6fv3u4wCglZ2i1wK/BnAmFhnqo2hRANCAARZnRE9dmjy8R2+3IdQAB79HROvImae\n\
HZ9PnBNW5QxmslUgZFVNNt96c7NcULbnKnR6xws1My42munupyFuuaMh\n\
-----END PRIVATE KEY-----\n";
