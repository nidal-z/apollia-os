//! The Apollia agent: the built-in chat, an agent like the others.
//!
//! Its settings are those of any agent: overrides of its definition in
//! `agents.db` (`mission`, `access`) and its model in `system.db`. The
//! settings panel of the chat and `apollia-os chat config` still show them
//! in their earlier shape, a system prompt, a list of tools and a backend;
//! [`ChatConfigView`] is that shape, read from the resolved agent, and
//! [`changes`] the route calls that turn one view into another. Both callers
//! run them through the runtime's routes, so every write is journaled.
//!
//! [`carry_over`] moves the settings the chat kept before it was an agent.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub mod carry_over;

pub use apollia_tools::APOLLIA_AGENT_ID;

/// The settings of the chat in the shape its panel and command show them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatConfigView {
    /// The mission the user gave Apollia; empty when none.
    #[serde(default)]
    pub system_prompt: String,
    /// The accesses the user gave Apollia; empty when none.
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    /// The model assigned to Apollia; `None` for the default.
    #[serde(default)]
    pub llm_backend: Option<String>,
}

impl ChatConfigView {
    /// Reads the view from the resolved agent the route answers
    /// (`GET /api/v1/agent-definitions/{name}`) and the model assigned to it.
    ///
    /// Only the user's own values show: a field the user never set reads as
    /// empty, whatever the definition says.
    pub fn from_resolved(resolved: &Value) -> Self {
        let overridden = |field: &str| {
            resolved
                .get("overridden")
                .and_then(Value::as_array)
                .is_some_and(|fields| fields.iter().any(|f| f.as_str() == Some(field)))
        };
        let system_prompt = if overridden("mission") {
            resolved
                .get("mission")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        } else {
            String::new()
        };
        let allowed_tools = if overridden("access") {
            resolved
                .get("access")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|v| v.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let llm_backend = resolved
            .get("model")
            .and_then(|m| m.get("model_id"))
            .and_then(Value::as_str)
            .map(str::to_owned);
        Self {
            system_prompt,
            allowed_tools,
            llm_backend,
        }
    }

    /// The view with its values trimmed, empty tools dropped, tools
    /// deduplicated in order, and a blank backend read as none.
    fn normalized(&self) -> Self {
        let mut tools: Vec<String> = Vec::new();
        for tool in &self.allowed_tools {
            let tool = tool.trim();
            if !tool.is_empty() && !tools.iter().any(|t| t == tool) {
                tools.push(tool.to_owned());
            }
        }
        Self {
            system_prompt: self.system_prompt.trim().to_owned(),
            allowed_tools: tools,
            llm_backend: self
                .llm_backend
                .as_deref()
                .map(str::trim)
                .filter(|b| !b.is_empty())
                .map(str::to_owned),
        }
    }
}

/// One route call that changes a setting of Apollia.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigWrite {
    /// Set the mission override.
    SetMission(String),
    /// Remove the mission override.
    ClearMission,
    /// Set the access override.
    SetAccess(Vec<String>),
    /// Remove the access override.
    ClearAccess,
    /// Assign a model; its source comes from [`model_body`].
    SetModel(String),
    /// Remove the model assignment.
    ClearModel,
}

impl ConfigWrite {
    /// The route the write goes to.
    pub fn path(&self) -> String {
        match self {
            Self::SetMission(_) | Self::ClearMission => override_path("mission"),
            Self::SetAccess(_) | Self::ClearAccess => override_path("access"),
            Self::SetModel(_) | Self::ClearModel => model_path(),
        }
    }
}

/// `GET` route of the resolved Apollia agent.
pub fn definition_path() -> String {
    format!("/api/v1/agent-definitions/{APOLLIA_AGENT_ID}")
}

/// Route of the model assigned to Apollia (`GET`, `PUT`, `DELETE`).
pub fn model_path() -> String {
    format!("/api/v1/llm/agents/{APOLLIA_AGENT_ID}/model")
}

fn override_path(field: &str) -> String {
    format!("/api/v1/agent-definitions/{APOLLIA_AGENT_ID}/overrides/{field}")
}

/// The body that assigns `model_id` to Apollia. `provider` is the provider of
/// the backend of that name (`GET /api/v1/llm/backends/{name}`), `None` when
/// no backend has it: the local engine serves a `llama-cpp` row, a server any
/// other; a name no backend holds is a server model when it reads
/// `<server>/<model>`, a local model otherwise.
pub fn model_body(model_id: &str, provider: Option<&str>) -> Value {
    let source = match provider {
        Some("llama-cpp") => "local",
        Some(_) => "server",
        None if model_id.contains('/') => "server",
        None => "local",
    };
    json!({ "model_source": source, "model_id": model_id })
}

/// The writes that turn `current` into `desired`, field by field; a field
/// left as it is writes nothing, so the journal records only what changed.
pub fn changes(current: &ChatConfigView, desired: &ChatConfigView) -> Vec<ConfigWrite> {
    let current = current.normalized();
    let desired = desired.normalized();
    let mut writes = Vec::new();
    if desired.system_prompt != current.system_prompt {
        writes.push(if desired.system_prompt.is_empty() {
            ConfigWrite::ClearMission
        } else {
            ConfigWrite::SetMission(desired.system_prompt)
        });
    }
    if desired.allowed_tools != current.allowed_tools {
        writes.push(if desired.allowed_tools.is_empty() {
            ConfigWrite::ClearAccess
        } else {
            ConfigWrite::SetAccess(desired.allowed_tools)
        });
    }
    if desired.llm_backend != current.llm_backend {
        writes.push(match desired.llm_backend {
            Some(backend) => ConfigWrite::SetModel(backend),
            None => ConfigWrite::ClearModel,
        });
    }
    writes
}

#[cfg(test)]
mod tests {
    use super::*;

    // GIVEN a resolved Apollia with a mission and an access of the user, and
    //   a model
    // WHEN the view is read
    // THEN the three settings come out in the shape of the panel
    #[test]
    fn the_view_reads_the_users_values() {
        let resolved = json!({
            "id": "apollia:chat",
            "mission": "Be brief.",
            "access": ["web_search", "file_read"],
            "overridden": ["access", "mission"],
            "model": {"agent": "apollia:chat", "model_source": "local", "model_id": "qwen"},
        });

        let view = ChatConfigView::from_resolved(&resolved);

        assert_eq!(
            view,
            ChatConfigView {
                system_prompt: "Be brief.".into(),
                allowed_tools: vec!["web_search".into(), "file_read".into()],
                llm_backend: Some("qwen".into()),
            }
        );
    }

    // GIVEN a resolved Apollia whose access comes from its definition only
    // WHEN the view is read
    // THEN nothing shows: only the user's own values are the panel's
    #[test]
    fn a_default_value_is_not_shown_as_the_users() {
        let resolved = json!({
            "mission": "",
            "access": ["web_search"],
            "overridden": [],
            "model": null,
        });

        let view = ChatConfigView::from_resolved(&resolved);

        assert_eq!(view, ChatConfigView::default());
    }

    // GIVEN a current view and a desired one that changes the prompt, empties
    //   the tools and keeps the backend
    // WHEN the writes are computed
    // THEN the mission is set, the access removed, the model left alone
    #[test]
    fn only_the_changed_fields_are_written() {
        let current = ChatConfigView {
            system_prompt: "Old.".into(),
            allowed_tools: vec!["web_search".into()],
            llm_backend: Some("local".into()),
        };
        let desired = ChatConfigView {
            system_prompt: " New. ".into(),
            allowed_tools: vec![" ".into()],
            llm_backend: Some("local".into()),
        };

        let writes = changes(&current, &desired);

        assert_eq!(
            writes,
            vec![
                ConfigWrite::SetMission("New.".into()),
                ConfigWrite::ClearAccess
            ]
        );
        assert_eq!(
            writes[0].path(),
            "/api/v1/agent-definitions/apollia:chat/overrides/mission"
        );
    }

    // GIVEN a backend set, then cleared
    // WHEN the writes are computed
    // THEN the model is assigned, then removed, on the model route
    #[test]
    fn the_backend_is_the_model_of_apollia() {
        let none = ChatConfigView::default();
        let some = ChatConfigView {
            llm_backend: Some("mistral".into()),
            ..ChatConfigView::default()
        };

        assert_eq!(
            changes(&none, &some),
            vec![ConfigWrite::SetModel("mistral".into())]
        );
        assert_eq!(changes(&some, &none), vec![ConfigWrite::ClearModel]);
        assert_eq!(
            ConfigWrite::ClearModel.path(),
            "/api/v1/llm/agents/apollia:chat/model"
        );
    }

    // GIVEN the backends a user may name
    // WHEN the model body is built
    // THEN a local engine row is local, any other row a server, and a name no
    //   row holds is read from its shape
    #[test]
    fn the_model_source_follows_the_backend() {
        assert_eq!(
            model_body("local", Some("llama-cpp"))["model_source"],
            "local"
        );
        assert_eq!(
            model_body("mistral", Some("mistral"))["model_source"],
            "server"
        );
        assert_eq!(model_body("lab/qwen", None)["model_source"], "server");
        assert_eq!(model_body("qwen", None)["model_source"], "local");
    }
}
