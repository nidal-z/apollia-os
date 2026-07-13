//! The [`HookRegistry`]: immutable index of lifecycle hook handlers.

use std::collections::HashMap;
use std::time::Duration;

use apollia_core::{HookEventKind, HookHandlerKind, HooksConfig};
use serde::{Deserialize, Serialize};

/// Resolved handler ready for invocation, built from
/// [`apollia_core::HookHandlerConfig`].
///
/// Separates the config-level representation (deserialized TOML) from the
/// runtime representation (pre-validated, timeout bound to a [`Duration`]).
#[derive(Debug, Clone)]
pub struct ResolvedHandler {
    /// Lifecycle events this handler subscribes to, in configuration order.
    pub events: Vec<HookEventKind>,
    /// Delivery mechanism, mirroring the config.
    pub kind: HookHandlerKind,
    /// Timeout bound for this specific handler.
    pub timeout: Duration,
}

/// Flat summary of one hook handler, used by the `GET /hooks` route and the
/// `apollia-os hooks list` CLI command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookHandlerSummary {
    /// Zero-based index in the configuration (declaration order).
    pub id: usize,
    /// Delivery mechanism: `"command"` or `"http"`.
    pub r#type: String,
    /// Lifecycle events this handler subscribes to (snake_case wire names).
    pub events: Vec<String>,
    /// Configured timeout in milliseconds.
    pub timeout_ms: u64,
    /// Command argv joined by spaces (command) or URL (http).
    pub target: String,
}

/// Registry of lifecycle hook handlers, indexed by [`HookEventKind`].
///
/// Built once from [`HooksConfig`] at runtime startup and shared as a read-only
/// reference with the execution loop. Immutable after construction: there is no
/// dynamic registration or hot-reload.
#[derive(Debug, Default)]
pub struct HookRegistry {
    /// Handlers in declaration order, each carrying its event subscriptions.
    /// Backs [`HookRegistry::list_all`].
    handlers: Vec<ResolvedHandler>,
    /// Handlers grouped by event, in declaration order, so
    /// [`HookRegistry::handlers_for`] can return a contiguous slice.
    by_event: HashMap<HookEventKind, Vec<ResolvedHandler>>,
}

impl HookRegistry {
    /// Builds a registry from the validated hooks configuration.
    ///
    /// Handlers are stored in declaration order. A handler subscribed to several
    /// events is indexed once per event.
    pub fn from_config(cfg: &HooksConfig) -> Self {
        let mut handlers = Vec::with_capacity(cfg.handlers.len());
        let mut by_event: HashMap<HookEventKind, Vec<ResolvedHandler>> = HashMap::new();
        for hc in &cfg.handlers {
            let resolved = ResolvedHandler {
                events: hc.events.clone(),
                kind: hc.kind.clone(),
                timeout: Duration::from_millis(hc.timeout_ms),
            };
            for event in &hc.events {
                by_event.entry(*event).or_default().push(resolved.clone());
            }
            handlers.push(resolved);
        }
        Self { handlers, by_event }
    }

    /// Returns the handlers subscribed to a given lifecycle event, in
    /// declaration order.
    ///
    /// Returns an empty slice when no handler is registered for the event.
    pub fn handlers_for(&self, event: HookEventKind) -> &[ResolvedHandler] {
        self.by_event.get(&event).map_or(&[], Vec::as_slice)
    }

    /// Returns `true` when no handler is registered for any event.
    pub fn is_empty(&self) -> bool {
        self.handlers.is_empty()
    }

    /// Returns a flat summary of every registered handler, in declaration order.
    pub fn list_all(&self) -> Vec<HookHandlerSummary> {
        self.handlers
            .iter()
            .enumerate()
            .map(|(id, h)| HookHandlerSummary {
                id,
                r#type: h.kind.type_str().to_string(),
                events: h.events.iter().map(|e| e.as_str().to_string()).collect(),
                timeout_ms: h.timeout.as_millis() as u64,
                target: h.kind.target(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::HookHandlerConfig;

    fn handler(events: Vec<HookEventKind>, kind: HookHandlerKind) -> HookHandlerConfig {
        HookHandlerConfig {
            events,
            kind,
            timeout_ms: 5_000,
        }
    }

    #[test]
    fn test_registry_from_config_indexes_by_event() {
        // GIVEN a config with one handler subscribed to PreToolUse and PostToolUse
        let cfg = HooksConfig {
            handlers: vec![handler(
                vec![HookEventKind::PreToolUse, HookEventKind::PostToolUse],
                HookHandlerKind::Http {
                    url: "http://127.0.0.1:9000/hook".to_string(),
                },
            )],
        };

        // WHEN the registry is built
        let registry = HookRegistry::from_config(&cfg);

        // THEN each subscribed event resolves the handler, others are empty
        assert_eq!(registry.handlers_for(HookEventKind::PreToolUse).len(), 1);
        assert_eq!(registry.handlers_for(HookEventKind::PostToolUse).len(), 1);
        assert!(registry.handlers_for(HookEventKind::PreCompact).is_empty());
    }

    #[test]
    fn test_registry_empty_when_no_handlers() {
        // GIVEN the default hooks config
        let registry = HookRegistry::from_config(&HooksConfig::default());

        // WHEN the registry is queried
        // THEN it is empty and every event yields an empty slice
        assert!(registry.is_empty());
        assert!(registry.handlers_for(HookEventKind::PreToolUse).is_empty());
        assert!(registry.list_all().is_empty());
    }

    #[test]
    fn test_registry_handler_order_preserved() {
        // GIVEN two handlers on the same event, declared in order A then B
        let cfg = HooksConfig {
            handlers: vec![
                handler(
                    vec![HookEventKind::PreToolUse],
                    HookHandlerKind::Command {
                        command: vec!["a".to_string()],
                    },
                ),
                handler(
                    vec![HookEventKind::PreToolUse],
                    HookHandlerKind::Command {
                        command: vec!["b".to_string()],
                    },
                ),
            ],
        };

        // WHEN handlers_for is called
        let registry = HookRegistry::from_config(&cfg);
        let resolved = registry.handlers_for(HookEventKind::PreToolUse);

        // THEN the slice preserves declaration order [A, B]
        assert_eq!(resolved.len(), 2);
        assert_eq!(resolved[0].kind.target(), "a");
        assert_eq!(resolved[1].kind.target(), "b");
    }

    #[test]
    fn test_list_all_reports_summary() {
        // GIVEN a command handler on two events
        let cfg = HooksConfig {
            handlers: vec![handler(
                vec![HookEventKind::PreToolUse, HookEventKind::PostToolUse],
                HookHandlerKind::Command {
                    command: vec!["/usr/bin/hook".to_string(), "--flag".to_string()],
                },
            )],
        };

        // WHEN list_all is called
        let summaries = HookRegistry::from_config(&cfg).list_all();

        // THEN the summary reflects type, events, timeout, and target
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].id, 0);
        assert_eq!(summaries[0].r#type, "command");
        assert_eq!(summaries[0].events, vec!["pre_tool_use", "post_tool_use"]);
        assert_eq!(summaries[0].timeout_ms, 5_000);
        assert_eq!(summaries[0].target, "/usr/bin/hook --flag");
    }
}
