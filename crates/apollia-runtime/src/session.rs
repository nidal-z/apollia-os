//! Session-level configuration for tool access control.
//!
//! Holds the `--allowed-tools` and `--disallowed-tools` values supplied by the
//! operator on the CLI. These are forwarded to [`apollia_tools::SessionToolFilter`]
//! when the [`ToolDispatcher`] is constructed for a given execution.
//!
//! [`ToolDispatcher`]: apollia_tools::ToolDispatcher

use serde::{Deserialize, Serialize};

/// Per-session tool access configuration.
///
/// Created from CLI flags and included in task submission payloads so the runtime
/// can enforce tool restrictions without modifying the global config.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionConfig {
    /// Restrictive allow-list. `None` means all tools are permitted (subject to
    /// `disallowed_tools`). When `Some`, only the listed tool names can be invoked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_tools: Option<Vec<String>>,
    /// Tools that are always blocked in this session, regardless of `allowed_tools`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub disallowed_tools: Vec<String>,
}

impl SessionConfig {
    /// Returns `true` when no restrictions are active (no filter at all).
    pub fn is_unrestricted(&self) -> bool {
        self.allowed_tools.is_none() && self.disallowed_tools.is_empty()
    }

    /// Convert into a [`apollia_tools::SessionToolFilter`].
    ///
    /// Returns `None` when there are no restrictions so callers can skip
    /// building an unnecessary filter.
    pub fn into_tool_filter(self) -> Option<apollia_tools::SessionToolFilter> {
        if self.is_unrestricted() {
            return None;
        }
        Some(apollia_tools::SessionToolFilter::new(
            self.allowed_tools,
            self.disallowed_tools,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unrestricted_when_both_empty() {
        // GIVEN no restrictions
        let config = SessionConfig::default();
        // THEN is_unrestricted returns true
        assert!(config.is_unrestricted());
    }

    #[test]
    fn restricted_when_allowed_tools_set() {
        // GIVEN an allow-list
        let config = SessionConfig {
            allowed_tools: Some(vec!["file_read".to_string()]),
            disallowed_tools: vec![],
        };
        // THEN not unrestricted
        assert!(!config.is_unrestricted());
    }

    #[test]
    fn into_filter_returns_none_for_unrestricted() {
        // GIVEN no restrictions
        let config = SessionConfig::default();
        // THEN into_filter produces None
        assert!(config.into_tool_filter().is_none());
    }

    #[test]
    fn into_filter_returns_some_when_disallowed_set() {
        // GIVEN a deny-list
        let config = SessionConfig {
            allowed_tools: None,
            disallowed_tools: vec!["bash_executor".to_string()],
        };
        // THEN into_filter produces Some
        let filter = config.into_tool_filter();
        assert!(filter.is_some());
        let f = filter.expect("expected Some");
        assert!(!f.is_allowed("bash_executor"));
        assert!(f.is_allowed("file_read"));
    }

    #[test]
    fn serialization_roundtrip() {
        // GIVEN a config with both fields set
        let config = SessionConfig {
            allowed_tools: Some(vec!["file_read".to_string(), "file_grep".to_string()]),
            disallowed_tools: vec!["bash_executor".to_string()],
        };
        // WHEN serialized and deserialized
        let json = serde_json::to_string(&config).expect("serialize");
        let restored: SessionConfig = serde_json::from_str(&json).expect("deserialize");
        // THEN fields are preserved
        assert_eq!(restored.allowed_tools, config.allowed_tools);
        assert_eq!(restored.disallowed_tools, config.disallowed_tools);
    }
}
