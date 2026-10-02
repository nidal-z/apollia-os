//! Mail providers whose IMAP server is known from the address alone.
//!
//! An operator types an address; for these providers the server and port are
//! filled in, so the only thing left to give is the application password.

/// IMAP over implicit TLS, the port every listed provider serves.
pub const DEFAULT_IMAP_PORT: u16 = 993;

/// A provider whose IMAP server is known.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KnownProvider {
    /// Name shown to the operator.
    pub name: &'static str,
    /// Address domains the provider serves, lowercase.
    pub domains: &'static [&'static str],
    /// IMAP server.
    pub host: &'static str,
    /// IMAP port.
    pub port: u16,
}

/// Every provider prefilled from the address.
pub const KNOWN_PROVIDERS: &[KnownProvider] = &[
    KnownProvider {
        name: "Gmail",
        domains: &["gmail.com", "googlemail.com"],
        host: "imap.gmail.com",
        port: DEFAULT_IMAP_PORT,
    },
    KnownProvider {
        name: "iCloud",
        domains: &["icloud.com", "me.com", "mac.com"],
        host: "imap.mail.me.com",
        port: DEFAULT_IMAP_PORT,
    },
    KnownProvider {
        name: "Yahoo",
        domains: &["yahoo.com", "yahoo.fr", "ymail.com"],
        host: "imap.mail.yahoo.com",
        port: DEFAULT_IMAP_PORT,
    },
    KnownProvider {
        name: "Orange",
        domains: &["orange.fr", "wanadoo.fr"],
        host: "imap.orange.fr",
        port: DEFAULT_IMAP_PORT,
    },
    KnownProvider {
        name: "Free",
        domains: &["free.fr"],
        host: "imap.free.fr",
        port: DEFAULT_IMAP_PORT,
    },
    KnownProvider {
        name: "La Poste",
        domains: &["laposte.net"],
        host: "imap.laposte.net",
        port: DEFAULT_IMAP_PORT,
    },
];

/// Domain of an address, lowercase, or `None` when it is not one.
#[must_use]
pub fn email_domain(email: &str) -> Option<String> {
    let (local, domain) = email.trim().rsplit_once('@')?;
    if local.is_empty() || domain.is_empty() {
        return None;
    }
    Some(domain.trim_end_matches('.').to_ascii_lowercase())
}

/// The provider serving `email`, when it is a known one.
#[must_use]
pub fn provider_for_email(email: &str) -> Option<&'static KnownProvider> {
    let domain = email_domain(email)?;
    KNOWN_PROVIDERS
        .iter()
        .find(|p| p.domains.contains(&domain.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_each_required_provider_is_prefilled_from_its_domain() {
        // GIVEN one address on each provider the product promises to prefill
        let cases = [
            ("ana@gmail.com", "imap.gmail.com"),
            ("ana@icloud.com", "imap.mail.me.com"),
            ("ana@yahoo.fr", "imap.mail.yahoo.com"),
            ("ana@orange.fr", "imap.orange.fr"),
            ("ana@free.fr", "imap.free.fr"),
            ("ana@laposte.net", "imap.laposte.net"),
        ];
        for (email, host) in cases {
            // WHEN its provider is looked up
            let provider = provider_for_email(email);
            // THEN the server is known, on the implicit TLS port
            let provider = provider.unwrap_or_else(|| panic!("{email} has no provider"));
            assert_eq!(provider.host, host, "{email}");
            assert_eq!(provider.port, 993, "{email}");
        }
    }

    #[test]
    fn test_domain_lookup_ignores_case_and_trailing_dot() {
        // GIVEN an address typed in capitals with a fully qualified domain
        let email = "Ana@GMail.COM.";
        // WHEN its provider is looked up
        let provider = provider_for_email(email);
        // THEN it is still Gmail
        assert_eq!(provider.map(|p| p.name), Some("Gmail"));
    }

    #[test]
    fn test_unknown_domain_has_no_provider() {
        // GIVEN an address on a domain no provider is listed for
        // WHEN its provider is looked up
        // THEN none is answered, and the operator gives the server
        assert!(provider_for_email("ana@example.org").is_none());
    }

    #[test]
    fn test_text_without_at_sign_has_no_domain() {
        // GIVEN strings that are not addresses
        // WHEN their domain is read
        // THEN none is answered
        assert_eq!(email_domain("ana"), None);
        assert_eq!(email_domain("@gmail.com"), None);
        assert_eq!(email_domain("ana@"), None);
    }

    #[test]
    fn test_no_domain_is_listed_twice() {
        // GIVEN the provider table
        let mut seen = std::collections::HashSet::new();
        // WHEN every domain is collected
        // THEN each appears once, so a lookup cannot be ambiguous
        for provider in KNOWN_PROVIDERS {
            for domain in provider.domains {
                assert!(seen.insert(*domain), "{domain} listed twice");
                assert_eq!(*domain, domain.to_ascii_lowercase());
            }
        }
    }
}
