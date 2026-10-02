//! TLS material the IMAP tests hand to rustls, and nothing else.
//!
//! It lives in a file of its own because `detect-private-key` refuses every
//! commit that touches a file carrying a private key. The
//! `.pre-commit-config.yaml` exemption is anchored on this exact path, on the
//! entry of that single hook; nothing but test material goes in.
//!
//! An authority and a server certificate it signed: rustls refuses a
//! certificate authority presented as the server's own certificate, so the
//! fake server shows the leaf and the client trusts the authority. EC P-256,
//! generated with `openssl` for `localhost` and `127.0.0.1`, valid to 2126,
//! and they protect nothing.

/// The authority the test client trusts.
pub(crate) const TEST_CA_PEM: &str = "-----BEGIN CERTIFICATE-----\n\
MIIBszCCAVmgAwIBAgIUIRA+Cya+0czABunGbEm8+DRrc1YwCgYIKoZIzj0EAwIw\n\
JjEkMCIGA1UEAwwbQXBvbGxpYSBJTUFQIHRlc3QgYXV0aG9yaXR5MCAXDTI2MTAw\n\
MjA2NTgzOFoYDzIxMjYwOTA4MDY1ODM4WjAmMSQwIgYDVQQDDBtBcG9sbGlhIElN\n\
QVAgdGVzdCBhdXRob3JpdHkwWTATBgcqhkjOPQIBBggqhkjOPQMBBwNCAAQ9JqCQ\n\
AyqQl3B4h4MJvEfXmKCAo9T/UkpkT0U371XKGFpij/yfyKC0t1GEJVlPAZCWv0tV\n\
BD2/Tkcu6DLgwt5wo2MwYTAdBgNVHQ4EFgQUCAf/wduZ+6Zt/F2+1KYgjRUF62Ew\n\
HwYDVR0jBBgwFoAUCAf/wduZ+6Zt/F2+1KYgjRUF62EwDwYDVR0TAQH/BAUwAwEB\n\
/zAOBgNVHQ8BAf8EBAMCAQYwCgYIKoZIzj0EAwIDSAAwRQIhAMoBlZBgOoyiwTMl\n\
Ic5U41BbgONkATNUkfnvcaVIiMwoAiBfy2SXnYoACzkT9/sKsiBtXG4bLdHngWCY\n\
h1Hfi8bk4A==\n\
-----END CERTIFICATE-----\n";

/// The certificate the fake server presents, signed by [`TEST_CA_PEM`].
pub(crate) const TEST_SERVER_CERT_PEM: &str = "-----BEGIN CERTIFICATE-----\n\
MIIB0jCCAXegAwIBAgIUGpGDaEifKTRBcPkgscjQ7RtUsGswCgYIKoZIzj0EAwIw\n\
JjEkMCIGA1UEAwwbQXBvbGxpYSBJTUFQIHRlc3QgYXV0aG9yaXR5MCAXDTI2MTAw\n\
MjA2NTgzOFoYDzIxMjYwOTA4MDY1ODM4WjAUMRIwEAYDVQQDDAlsb2NhbGhvc3Qw\n\
WTATBgcqhkjOPQIBBggqhkjOPQMBBwNCAARxCQekWBkZS4oxtouAUbS74t3Q8nyl\n\
sBP1KkDoMjb6hkPRBup4b0KmIhZNrFlWSeV03AKYZDljpQJI74625gEQo4GSMIGP\n\
MAwGA1UdEwEB/wQCMAAwDgYDVR0PAQH/BAQDAgeAMBMGA1UdJQQMMAoGCCsGAQUF\n\
BwMBMBoGA1UdEQQTMBGCCWxvY2FsaG9zdIcEfwAAATAdBgNVHQ4EFgQUULN8DaYr\n\
TrcEAjJ369k5aRkDyvowHwYDVR0jBBgwFoAUCAf/wduZ+6Zt/F2+1KYgjRUF62Ew\n\
CgYIKoZIzj0EAwIDSQAwRgIhAPnUNIX9aBfKfH/lY3TDR3+p1DbA0mRAQ1woKFu1\n\
H6pyAiEAjCt8CmzOPvmFa4rnxxVZViUvsak+47zeFEtZho3aHs0=\n\
-----END CERTIFICATE-----\n";

/// The private key of [`TEST_SERVER_CERT_PEM`], PKCS#8.
pub(crate) const TEST_SERVER_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\n\
MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQgeSDFfG/T2fh9Q4JO\n\
MK1SJRljG3juw8aiP9p7dhRQDbyhRANCAARxCQekWBkZS4oxtouAUbS74t3Q8nyl\n\
sBP1KkDoMjb6hkPRBup4b0KmIhZNrFlWSeV03AKYZDljpQJI74625gEQ\n\
-----END PRIVATE KEY-----\n";

/// An authority that signed nothing the fake server shows.
pub(crate) const TEST_OTHER_CA_PEM: &str = "-----BEGIN CERTIFICATE-----\n\
MIIBrjCCAVOgAwIBAgIUI9uUVL6BcDym+7riFKmbVqV5Tz8wCgYIKoZIzj0EAwIw\n\
IzEhMB8GA1UEAwwYVW5yZWxhdGVkIHRlc3QgYXV0aG9yaXR5MCAXDTI2MTAwMjA3\n\
MDMzNFoYDzIxMjYwOTA4MDcwMzM0WjAjMSEwHwYDVQQDDBhVbnJlbGF0ZWQgdGVz\n\
dCBhdXRob3JpdHkwWTATBgcqhkjOPQIBBggqhkjOPQMBBwNCAATjSAdS56uGtd2Q\n\
z7fS7XC3/pHBMgJVKdgEUFA9WN1ivLBLIOGP7IE6PMuqWgMf0LZPb5vmTwyiU+gX\n\
UMpB+kF/o2MwYTAdBgNVHQ4EFgQUf73k31hSgRqCR2Q3XEITAgGT7a0wHwYDVR0j\n\
BBgwFoAUf73k31hSgRqCR2Q3XEITAgGT7a0wDwYDVR0TAQH/BAUwAwEB/zAOBgNV\n\
HQ8BAf8EBAMCAQYwCgYIKoZIzj0EAwIDSQAwRgIhAMHrqntmUDZN3SqF+XCR8++m\n\
oF+zjT2JwGxCOYVSFg22AiEAjIe3RP3kxfS8ZCO/iuCCuk1azLxtrSPyf9VHmDtE\n\
EIs=\n\
-----END CERTIFICATE-----\n";
