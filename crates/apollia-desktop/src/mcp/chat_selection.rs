//! The curated selection, as Apollia searches it in a conversation for a
//! connection a request needs.
//!
//! The desktop is the host that holds the selection; it hands the runtime the
//! entries the catalogue lists, each with the admission request the
//! organisation policy is asked about before it is added, so an entry the
//! policy forbids is never offered.

use std::collections::HashMap;

use apollia_runtime::chat::{ConnectionSelection, SelectionEntry};
use apollia_runtime::governance::{AdmissionRequest, ConnectionGovernance};

use crate::mcp::enrichments::ConnectorEnrichment;

/// Locale whose texts name and describe an entry; the others are keywords.
const PRIMARY_LOCALE: &str = "en";

/// The listed entries of `enrichments`, decided by `governance`.
#[must_use]
pub fn connection_selection(
    enrichments: &[ConnectorEnrichment],
    governance: ConnectionGovernance,
) -> ConnectionSelection {
    let entries = enrichments
        .iter()
        .filter(|e| e.is_listed())
        .map(selection_entry)
        .collect();
    ConnectionSelection::new(entries, governance)
}

/// One listed entry, as the chat searches it.
fn selection_entry(enrichment: &ConnectorEnrichment) -> SelectionEntry {
    let label = in_primary(&enrichment.operator_label)
        .unwrap_or(&enrichment.package_identifier)
        .to_owned();
    let description = enrichment
        .description
        .as_ref()
        .and_then(in_primary)
        .unwrap_or_default()
        .to_owned();
    let mut keywords: Vec<String> = Vec::new();
    keywords.extend(other_locales(&enrichment.operator_label));
    if let Some(described) = &enrichment.description {
        keywords.extend(other_locales(described));
    }
    keywords.push(enrichment.category.clone());
    keywords.extend(enrichment.registry_names.iter().cloned());
    for example in &enrichment.examples {
        keywords.extend(example.label.values().cloned());
    }
    SelectionEntry {
        id: enrichment.package_identifier.clone(),
        label,
        description,
        keywords,
        admission: admission_of(enrichment),
    }
}

/// What the policy is asked about the entry before it is added: a remote
/// entry by its address, a package entry by the npm package it runs.
fn admission_of(enrichment: &ConnectorEnrichment) -> AdmissionRequest {
    let (transport, url) = match &enrichment.remote_url {
        Some(url) => (
            enrichment
                .remote_transport
                .as_deref()
                .unwrap_or("streamable-http"),
            Some(url.as_str()),
        ),
        None => ("stdio", None),
    };
    let npm_package = (enrichment.remote_url.is_none()
        && enrichment.package_registry_type.as_deref() == Some("npm"))
    .then_some(enrichment.package_identifier.as_str());
    AdmissionRequest::for_selection_entry(
        &enrichment.package_identifier,
        transport,
        url,
        npm_package,
        enrichment.tool_families.values().copied(),
    )
}

/// The text of `texts` in the primary locale, or in any other.
fn in_primary(texts: &HashMap<String, String>) -> Option<&str> {
    texts
        .get(PRIMARY_LOCALE)
        .or_else(|| texts.values().next())
        .map(String::as_str)
}

/// The texts of `texts` in every locale but the primary one.
fn other_locales(texts: &HashMap<String, String>) -> impl Iterator<Item = String> + '_ {
    texts
        .iter()
        .filter(|(locale, _)| locale.as_str() != PRIMARY_LOCALE)
        .map(|(_, text)| text.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    use apollia_runtime::governance::{AdmissionDecision, ConnectionAdmission, PolicyReadError};

    /// A policy that refuses every remote connection.
    struct RefuseRemote;

    impl ConnectionAdmission for RefuseRemote {
        fn read_decision(
            &self,
            request: &AdmissionRequest,
        ) -> Result<AdmissionDecision, PolicyReadError> {
            Ok(if request.remote {
                AdmissionDecision::Refused {
                    reason: "remote connections are not allowed".into(),
                }
            } else {
                AdmissionDecision::Admitted
            })
        }
    }

    // GIVEN the selection compiled into the application
    // WHEN it is handed to the chat, without a policy
    // THEN every listed entry is in it and none other, and a search for
    //      Notion answers the entry the add flow opens
    #[test]
    fn test_the_chat_receives_every_listed_entry() {
        // GIVEN
        let enrichments = crate::mcp::enrichments::compiled_enrichments();
        let listed: Vec<&str> = enrichments
            .iter()
            .filter(|e| e.is_listed())
            .map(|e| e.package_identifier.as_str())
            .collect();
        assert!(!listed.is_empty(), "the compiled selection lists entries");

        // WHEN
        let selection = connection_selection(&enrichments, ConnectionGovernance::default());

        // THEN
        assert_eq!(selection.len(), listed.len());
        let notion =
            apollia_runtime::chat::connection_lookup::lookup(&selection, "notion", 20, &[], &[]);
        assert!(
            notion
                .iter()
                .any(|m| m.entry == "@notionhq/notion-mcp-server"),
            "{notion:?}"
        );
    }

    // GIVEN the compiled selection and a policy refusing remote connections
    // WHEN Apollia searches it for Notion, a remote service
    // THEN Notion is not offered
    #[test]
    fn test_a_remote_entry_is_not_offered_under_a_policy_refusing_remote() {
        // GIVEN
        let enrichments = crate::mcp::enrichments::compiled_enrichments();
        let governance = ConnectionGovernance::new(std::sync::Arc::new(RefuseRemote), None);

        // WHEN
        let selection = connection_selection(&enrichments, governance);
        let found =
            apollia_runtime::chat::connection_lookup::lookup(&selection, "notion", 20, &[], &[]);

        // THEN
        assert!(
            found
                .iter()
                .all(|m| m.entry != "@notionhq/notion-mcp-server"),
            "{found:?}"
        );
    }
}
