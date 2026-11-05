//! `engine remove <engine> <tag> <variant> --confirm`: remove one installed
//! build through `DELETE /api/v1/engines/{engine}/{tag}/{variant}`.
//!
//! The build is first asked for without `confirm=true`. The runtime then
//! refuses the build `engines.gguf` chooses (`chosen`) and says, in `after`,
//! what the setting comes down to once it is gone; the CLI asks again with
//! `confirm=true` and says it. A build in service, the embedded engine, a
//! build the organisation policy fixes and a build not installed are refused
//! with exit 1, the message of the runtime with them.

use serde_json::{json, Value};

use super::{print_document, runtime_message, transport};
use crate::client::{encode_path_segment, RawResponse, RuntimeClient};
use crate::exit_codes;

/// The route of one build, with or without the confirmation.
pub(crate) fn removal_route(build: [&str; 3], confirm: bool) -> String {
    let [engine, tag, variant] = build.map(encode_path_segment);
    let route = format!("/api/v1/engines/{engine}/{tag}/{variant}");
    if confirm {
        format!("{route}?confirm=true")
    } else {
        route
    }
}

/// The code and `after` of a refusal of the removal route.
fn refusal_of(resp: &RawResponse) -> (Option<String>, Option<String>) {
    let body: Value = serde_json::from_str(&resp.body).unwrap_or(Value::Null);
    let text = |key: &str| body.get(key).and_then(Value::as_str).map(str::to_owned);
    (text("code"), text("after"))
}

/// A refusal of the runtime: exit 1 when it refuses this build, 2 when it
/// failed to remove it.
fn refused(json: bool, resp: &RawResponse) -> i32 {
    let code = if resp.status < 500 {
        exit_codes::GENERAL_ERROR
    } else {
        exit_codes::RUNTIME_ERROR
    };
    crate::output::emit_error(json, code, &runtime_message(resp))
}

/// What `after` says of `engines.gguf`, in words.
pub(crate) fn after_text(after: &str) -> String {
    if after == "auto" {
        "engines.gguf is back to auto: the next model load takes the automatic choice".to_owned()
    } else {
        format!(
            "engines.gguf keeps its variant: the build {after} takes the place of the one removed"
        )
    }
}

/// `engine remove <engine> <tag> <variant> [--confirm]`.
pub(crate) async fn remove(
    client: &RuntimeClient,
    build: [&String; 3],
    confirm: bool,
    json: bool,
) -> i32 {
    let build = build.map(String::as_str);
    let name = build.join(" ");
    if let Some(code) =
        crate::output::require_confirmation(confirm, json, &format!("remove {name}"))
    {
        return code;
    }
    let mut resp = match client.delete(&removal_route(build, false)).await {
        Ok(resp) => resp,
        Err(e) => return transport(json, &e),
    };
    let mut after = None;
    if resp.status == 409 {
        if let (Some(code), Some(then)) = refusal_of(&resp) {
            if code == "chosen" {
                after = Some(then);
                resp = match client.delete(&removal_route(build, true)).await {
                    Ok(resp) => resp,
                    Err(e) => return transport(json, &e),
                };
            }
        }
    }
    if resp.status >= 400 {
        return refused(json, &resp);
    }
    if json {
        print_document(&json!({
            "removed": { "engine": build[0], "tag": build[1], "variant": build[2] },
            "gguf_after": after,
        }));
        return exit_codes::SUCCESS;
    }
    crate::note!("Removed {name}.");
    if let Some(after) = &after {
        crate::note!("{}.", after_text(after));
    }
    exit_codes::SUCCESS
}
