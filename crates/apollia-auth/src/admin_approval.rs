//! Microsoft's answer that an administrator of the organisation must approve
//! the application before its user can sign in, and the address of that
//! approval.
//!
//! In an organisation at its default setting, a user cannot consent alone to
//! `Mail.Read`, `Mail.ReadWrite` or `Calendars.*`: the identity platform
//! answers with an `AADSTS` code instead of a code to exchange, either on the
//! redirect or at the token endpoint. A personal account (Outlook.com) signs in
//! through the same `common` authority and consents for itself, so it never
//! receives one of these codes.
//!
//! What the user can do then is send their administrator the approval page of
//! the application, `adminconsent` under the `organizations` authority, where
//! an administrator signs in and approves it for the whole organisation.

/// The `AADSTS` codes that mean an administrator must approve the
/// application, and nothing the user can do alone will.
///
/// - `90094`: the grant requires an administrator's permission.
/// - `90095`: the same, with the approval request workflow turned on.
/// - `900941`: an administrator must approve, the application being judged
///   risky.
/// - `65001`: nobody has consented to the application, the code the token
///   endpoint answers when the consent never happened.
///
/// A user who declines (`65004`) or cancels is not among them: retrying is
/// theirs to do.
pub const ADMIN_APPROVAL_CODES: [u32; 4] = [90094, 90095, 900941, 65001];

/// Authority under which the approval page is opened: any organisation, the
/// administrator's own resolved when they sign in.
const APPROVAL_AUTHORITY: &str = "https://login.microsoftonline.com/organizations/adminconsent";

/// The `AADSTS` codes `text` names, in order, each read whole: `AADSTS900941`
/// is `900941`, never `90094`.
#[must_use]
pub fn aadsts_codes(text: &str) -> Vec<u32> {
    text.match_indices("AADSTS")
        .filter_map(|(at, marker)| {
            let digits: String = text[at + marker.len()..]
                .chars()
                .take_while(char::is_ascii_digit)
                .collect();
            digits.parse().ok()
        })
        .collect()
}

/// `true` when and only when `text`, an error answered by the Microsoft
/// identity platform (the `error` and `error_description` of a redirect, or
/// the message of a token exchange), names one of [`ADMIN_APPROVAL_CODES`].
#[must_use]
pub fn requires_admin_approval(text: &str) -> bool {
    aadsts_codes(text)
        .iter()
        .any(|code| ADMIN_APPROVAL_CODES.contains(code))
}

/// The approval page of the application `client_id`, the address the user
/// copies to their administrator.
#[must_use]
pub fn admin_consent_url(client_id: &str) -> String {
    format!(
        "{APPROVAL_AUTHORITY}?client_id={}",
        urlencoding::encode(client_id)
    )
}

/// The approval page of the Microsoft client this build resolves, `None` when
/// none resolves.
#[must_use]
pub fn resolved_admin_consent_url() -> Option<String> {
    crate::ConnectorProvider::Microsoft
        .resolve_client_id()
        .map(|id| admin_consent_url(&id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_an_administrator_approval_answer_is_recognised_on_the_redirect_and_the_token_endpoint()
    {
        // GIVEN the answers Microsoft gives when an administrator must approve
        let redirect = "consent_required: AADSTS90094: The grant requires admin permission.";
        let workflow = "access_denied: AADSTS90095: Admin consent is required.";
        let token = "invalid_grant: AADSTS65001: The user or administrator has not consented \
                     to use the application with ID 'c4f95bc5'.";
        let risky = "AADSTS900941: Administrator consent is required. App is considered risky.";

        // WHEN each is read
        // THEN every one requires an administrator
        for text in [redirect, workflow, token, risky] {
            assert!(requires_admin_approval(text), "{text}");
        }
    }

    #[test]
    fn test_a_refusal_the_user_can_retry_is_not_an_approval_answer() {
        // GIVEN a user who declined, a user who cancelled, a code that only
        // starts like an approval code, and an error with no code at all
        let declined = "access_denied: AADSTS65004: User declined to consent to access the app.";
        let cancelled = "access_denied";
        let longer = "AADSTS900944: something else";

        // WHEN each is read
        // THEN none of them requires an administrator
        for text in [declined, cancelled, longer] {
            assert!(!requires_admin_approval(text), "{text}");
        }
        assert_eq!(aadsts_codes(longer), vec![900944]);
    }

    #[test]
    fn test_the_approval_page_names_the_client_under_any_organisation() {
        // GIVEN the shipped client id
        let id = crate::connector_providers::MICROSOFT_DEFAULT_CLIENT_ID;

        // WHEN its approval page is built
        let url = admin_consent_url(id);

        // THEN it is the adminconsent page of any organisation, for that client
        assert_eq!(
            url,
            format!("https://login.microsoftonline.com/organizations/adminconsent?client_id={id}")
        );
    }

    #[test]
    fn test_a_client_id_is_encoded_into_the_approval_page() {
        // GIVEN a client id carrying characters a query must escape
        // WHEN its approval page is built
        let url = admin_consent_url("a b&c");

        // THEN they are escaped, so nothing can be appended to the query
        assert!(url.ends_with("?client_id=a%20b%26c"), "{url}");
    }
}
