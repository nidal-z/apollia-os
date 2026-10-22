//! The OpenAPI schemas of the state of a connection.
//!
//! The state types live in `apollia-core`, which carries no `utoipa`, so the
//! fields that hold them describe themselves here. The names listed are the
//! serde names of those types; a test holds the two together.

use apollia_core::{ConnectionReasonKey, ConnectionStatus};
use utoipa::openapi::schema::{ObjectBuilder, Schema, Type};
use utoipa::openapi::RefOr;

/// Every state, in its wire name.
pub(super) const STATUSES: [ConnectionStatus; 5] = [
    ConnectionStatus::Working,
    ConnectionStatus::NeedsRepair,
    ConnectionStatus::Paused,
    ConnectionStatus::Blocked,
    ConnectionStatus::NotChecked,
];

/// Every reason key, in its wire name.
pub(super) const REASON_KEYS: [ConnectionReasonKey; 7] = [
    ConnectionReasonKey::NotChecked,
    ConnectionReasonKey::SignInAgain,
    ConnectionReasonKey::PasswordRefused,
    ConnectionReasonKey::FolderNotFound,
    ConnectionReasonKey::Unreachable,
    ConnectionReasonKey::Failing,
    ConnectionReasonKey::ForbiddenByPolicy,
];

/// The serde name of `value`, a unit variant.
pub(super) fn wire_name<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

fn names<T: serde::Serialize>(values: &[T]) -> Vec<String> {
    values.iter().map(wire_name).collect()
}

/// Schema of `Connection.status`.
pub(super) fn status_schema() -> impl Into<RefOr<Schema>> {
    ObjectBuilder::new()
        .schema_type(Type::String)
        .enum_values(Some(names(&STATUSES)))
        .description(Some(
            "What its user reads: working, needs repair, paused, blocked by the organisation, or not checked yet (nothing has found it in any state).",
        ))
}

/// Schema of `Connection.reason`.
pub(super) fn reason_schema() -> impl Into<RefOr<Schema>> {
    ObjectBuilder::new()
        .schema_type(utoipa::openapi::schema::SchemaType::from_iter([
            Type::Object,
            Type::Null,
        ]))
        .description(Some(
            "Why it is not working, absent when it works: a key the interface translates, and the technical detail.",
        ))
        .property(
            "key",
            ObjectBuilder::new()
                .schema_type(Type::String)
                .enum_values(Some(names(&REASON_KEYS))),
        )
        .required("key")
        .property(
            "detail",
            ObjectBuilder::new().schema_type(utoipa::openapi::schema::SchemaType::from_iter([
                Type::String,
                Type::Null,
            ])),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_the_schemas_list_every_wire_name_of_the_state_types() {
        // GIVEN every state and every reason key, through an exhaustive match
        // that stops compiling when a variant is added and not listed
        let listed_status = |s: ConnectionStatus| match s {
            ConnectionStatus::Working
            | ConnectionStatus::NeedsRepair
            | ConnectionStatus::Paused
            | ConnectionStatus::Blocked
            | ConnectionStatus::NotChecked => STATUSES.contains(&s),
        };
        let listed_key = |k: ConnectionReasonKey| match k {
            ConnectionReasonKey::NotChecked
            | ConnectionReasonKey::SignInAgain
            | ConnectionReasonKey::PasswordRefused
            | ConnectionReasonKey::FolderNotFound
            | ConnectionReasonKey::Unreachable
            | ConnectionReasonKey::Failing
            | ConnectionReasonKey::ForbiddenByPolicy => REASON_KEYS.contains(&k),
        };

        // WHEN the schemas are built
        let status = serde_json::to_value(status_schema().into()).unwrap();
        let reason = serde_json::to_value(reason_schema().into()).unwrap();

        // THEN they carry exactly the serde names
        assert!(STATUSES.iter().all(|s| listed_status(*s)));
        assert!(REASON_KEYS.iter().all(|k| listed_key(*k)));
        assert_eq!(
            status["enum"],
            serde_json::json!([
                "working",
                "needs_repair",
                "paused",
                "blocked",
                "not_checked"
            ])
        );
        assert_eq!(
            reason["properties"]["key"]["enum"],
            serde_json::json!(names(&REASON_KEYS))
        );
        assert_eq!(
            wire_name(&ConnectionReasonKey::SignInAgain),
            "sign_in_again"
        );
    }
}
