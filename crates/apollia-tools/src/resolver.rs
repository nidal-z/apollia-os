//! ToolResolver: validates tool availability in the INITIALIZING state.
//!
//! This function is called exclusively by the runtime during the
//! CREATED -> INITIALIZING transition. It produces no side effect: read-only
//! access to the ToolRegistry, emitting `tracing` warnings for absent optional
//! tools.
//!
//! Fail fast: the first missing required tool triggers an immediate failure.
//! One actor, one responsibility: the ToolResolver validates, it does not execute.

use apollia_core::AgentManifest;
use thiserror::Error;

use crate::registry::{ToolRegistryError, ToolRegistryHandle};

/// Prefix reserved for A2A dependencies (inter-agent skills).
/// These entries are not in the `ToolRegistry`; they are resolved at invocation
/// time via the ToolProxy + A2AInvoker (see `apollia-runtime::chat::a2a_tools`).
const A2A_DEPENDENCY_PREFIX: &str = "a2a:";

/// Resolution report for an agent's tools.
///
/// Returned by [`resolve`] on success. Forwarded by the runtime to
/// [`AgentRegistry`][apollia_runtime] to determine the transition to `ACTIVE`
/// (AllResolved) or `DEGRADED` (Degraded).
pub struct ResolutionReport {
    /// Overall resolution status.
    pub status: ResolutionStatus,
    /// Names of the required tools resolved successfully.
    pub resolved: Vec<String>,
    /// Warnings: absent optional tools, etc.
    pub warnings: Vec<String>,
}

/// Overall status of a tool resolution.
#[derive(Debug, PartialEq, Eq)]
pub enum ResolutionStatus {
    /// All required tools are available in the catalogue.
    AllResolved,
    /// Some optional tools are absent; the agent moves to DEGRADED.
    Degraded,
}

/// Blocking errors returned by [`resolve`].
///
/// These errors move the agent to `STOPPED` with a clear error message.
#[derive(Debug, Error)]
pub enum ToolResolutionError {
    /// A required tool is absent from the catalogue.
    #[error("required tool not found in registry: {0}")]
    RequiredToolMissing(String),
    /// A tool marked `dangerous=true` is requested but the manifest does not set
    /// `dangerous_tools_allowed = true`.
    #[error("tool '{0}' is dangerous but manifest does not set dangerous_tools_allowed=true")]
    DangerousToolNotAllowed(String),
    /// Communication error with the ToolRegistry actor.
    #[error("tool registry error: {0}")]
    RegistryError(#[from] ToolRegistryError),
}

/// Validates the availability of every tool declared in an [`AgentManifest`].
///
/// Called exclusively during the `INITIALIZING` transition. Returns a
/// [`ResolutionReport`] on success, or [`ToolResolutionError`] if a required
/// tool is missing or a dangerous tool is not authorized.
///
/// # Behaviour
///
/// 1. For each tool in `tools_required` (in order):
///    - Absent from the catalogue: immediate `Err(RequiredToolMissing)` (fail fast).
///    - `dangerous=true` without `dangerous_tools_allowed`: `Err(DangerousToolNotAllowed)`.
///    - Present and safe: added to `resolved`.
/// 2. For each tool in `tools_optional`:
///    - Absent: `tracing::warn!` plus a message added to `warnings`, continue.
///    - `dangerous=true` without `dangerous_tools_allowed`: `tracing::warn!`, continue.
///    - Present and safe: added to `resolved`.
/// 3. If `warnings` is empty, `ResolutionStatus::AllResolved`; otherwise
///    `ResolutionStatus::Degraded`.
/// 4. Dependencies prefixed with `a2a:` are resolved unconditionally (the real
///    resolution happens at invocation via ToolProxy/A2AInvoker), without
///    querying the `ToolRegistry`.
///
/// # Errors
///
/// - [`ToolResolutionError::RequiredToolMissing`]: required tool not found.
/// - [`ToolResolutionError::DangerousToolNotAllowed`]: required dangerous tool not authorized.
/// - [`ToolResolutionError::RegistryError`]: the registry actor is unreachable.
pub async fn resolve(
    manifest: &AgentManifest,
    registry: &ToolRegistryHandle,
) -> Result<ResolutionReport, ToolResolutionError> {
    let mut resolved: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    // Pass 1, required tools: immediate failure on the first problem (fail fast).
    for tool_name in &manifest.tools_required {
        if tool_name.starts_with(A2A_DEPENDENCY_PREFIX) {
            tracing::info!(
                agent = %manifest.name,
                dep = %tool_name,
                "ToolResolver: A2A dependency declared (resolved at invocation, not at boot)"
            );
            resolved.push(tool_name.clone());
            continue;
        }
        match registry.get(tool_name).await? {
            None => {
                tracing::error!(
                    agent = %manifest.name,
                    tool = %tool_name,
                    "ToolResolver: required tool missing"
                );
                return Err(ToolResolutionError::RequiredToolMissing(tool_name.clone()));
            }
            Some(descriptor) if descriptor.dangerous && !manifest.dangerous_tools_allowed => {
                tracing::error!(
                    agent = %manifest.name,
                    tool = %tool_name,
                    "ToolResolver: dangerous tool not allowed by manifest"
                );
                return Err(ToolResolutionError::DangerousToolNotAllowed(
                    tool_name.clone(),
                ));
            }
            Some(_) => {
                tracing::info!(
                    agent = %manifest.name,
                    tool = %tool_name,
                    "ToolResolver: required tool resolved"
                );
                resolved.push(tool_name.clone());
            }
        }
    }

    // Pass 2, optional tools: degradation, no fatal error.
    for tool_name in &manifest.tools_optional {
        if tool_name.starts_with(A2A_DEPENDENCY_PREFIX) {
            tracing::info!(
                agent = %manifest.name,
                dep = %tool_name,
                "ToolResolver: A2A dependency declared (resolved at invocation, not at boot)"
            );
            resolved.push(tool_name.clone());
            continue;
        }
        match registry.get(tool_name).await? {
            None => {
                let warning = format!("{tool_name} not found");
                tracing::warn!(
                    agent = %manifest.name,
                    tool = %tool_name,
                    "ToolResolver: optional tool missing - agent will run in DEGRADED mode"
                );
                warnings.push(warning);
            }
            Some(descriptor) if descriptor.dangerous && !manifest.dangerous_tools_allowed => {
                let warning = format!("{tool_name} is dangerous but dangerous_tools_allowed=false");
                tracing::warn!(
                    agent = %manifest.name,
                    tool = %tool_name,
                    "ToolResolver: dangerous optional tool not allowed - skipping"
                );
                warnings.push(warning);
            }
            Some(_) => {
                tracing::info!(
                    agent = %manifest.name,
                    tool = %tool_name,
                    "ToolResolver: optional tool resolved"
                );
                resolved.push(tool_name.clone());
            }
        }
    }

    let status = if warnings.is_empty() {
        ResolutionStatus::AllResolved
    } else {
        ResolutionStatus::Degraded
    };

    Ok(ResolutionReport {
        status,
        resolved,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::{AgentManifest, SandboxProfile};
    use serde_json::json;

    use crate::descriptor::{ToolDescriptor, ToolKind};
    use crate::registry::ToolRegistryHandle;

    fn minimal_manifest() -> AgentManifest {
        AgentManifest {
            name: "test-agent".to_string(),
            version: "1.0.0".to_string(),
            description: "test".to_string(),
            tools_required: vec![],
            tools_optional: vec![],
            supports_streaming: false,
            supports_a2a: false,
            memory_namespace: None,
            shared_memory_namespaces: vec![],
            max_concurrent_tasks: 1,
            step_budget: None,
            network_allowlist: None,
            dangerous_tools_allowed: false,
            tags: vec![],
            skills: vec![],
            execution_mode: "auto".to_string(),
            system_prompt: None,
            tools_requiring_approval: vec![],
            llm_backend: None,
            packages: vec![],
            memory_config: None,
            agent_type: None,
            examples: vec![],
            limitations: vec![],
            setup_notes: None,
            agent_class: None,
            user_memory_write: false,
            datasources: vec![],
            templates: vec![],
            secrets: vec![],
            check_commands: vec![],
        }
    }

    async fn registry_with_tools(names: &[&str]) -> ToolRegistryHandle {
        let registry = ToolRegistryHandle::start();
        for name in names {
            let descriptor = ToolDescriptor {
                name: name.to_string(),
                version: "1.0.0".to_string(),
                description: format!("{name} tool"),
                kind: ToolKind::Native,
                input_schema: json!({ "type": "object" }),
                output_schema: None,
                sandbox_profile: SandboxProfile::FileSystem,
                tags: vec![],
                dangerous: false,
                is_read_only: false,
                risk_score: 0,
                approval_risk_level: None,
                impact_description: None,
                reject_reason_required: false,
            };
            registry.register(descriptor).await.unwrap();
        }
        registry
    }

    async fn registry_with_dangerous_tool(name: &str) -> ToolRegistryHandle {
        let registry = ToolRegistryHandle::start();
        let descriptor = ToolDescriptor {
            name: name.to_string(),
            version: "1.0.0".to_string(),
            description: format!("{name} dangerous tool"),
            kind: ToolKind::Native,
            input_schema: json!({ "type": "object" }),
            output_schema: None,
            sandbox_profile: SandboxProfile::Full,
            tags: vec![],
            dangerous: true,
            is_read_only: false,
            risk_score: 10,
            approval_risk_level: None,
            impact_description: None,
            reject_reason_required: false,
        };
        registry.register(descriptor).await.unwrap();
        registry
    }

    #[tokio::test]
    async fn test_all_required_tools_present_succeeds() {
        // GIVEN
        let registry = registry_with_tools(&["bash_executor", "file_read"]).await;
        let mut manifest = minimal_manifest();
        manifest.tools_required = vec!["bash_executor".to_string(), "file_read".to_string()];
        // WHEN
        let result = resolve(&manifest, &registry).await;
        // THEN
        let report = result.expect("resolution should succeed");
        assert!(matches!(report.status, ResolutionStatus::AllResolved));
        assert!(report.warnings.is_empty());
        assert_eq!(report.resolved.len(), 2);
        registry.shutdown().await;
    }

    #[tokio::test]
    async fn test_missing_required_tool_returns_error() {
        // GIVEN: empty registry
        let registry = ToolRegistryHandle::start();
        let mut manifest = minimal_manifest();
        manifest.tools_required = vec!["bash_executor".to_string()];
        // WHEN
        let result = resolve(&manifest, &registry).await;
        // THEN
        assert!(
            matches!(result, Err(ToolResolutionError::RequiredToolMissing(_))),
            "expected RequiredToolMissing, got: {:?}",
            result.err()
        );
        registry.shutdown().await;
    }

    #[tokio::test]
    async fn test_missing_optional_tool_is_warning_only() {
        // GIVEN: registry without "mcp_erp"
        let registry = ToolRegistryHandle::start();
        let mut manifest = minimal_manifest();
        manifest.tools_optional = vec!["mcp_erp".to_string()];
        // WHEN
        let result = resolve(&manifest, &registry).await;
        // THEN
        let report = result.expect("resolution should succeed despite missing optional");
        assert!(matches!(report.status, ResolutionStatus::Degraded));
        assert!(!report.warnings.is_empty());
        assert!(
            report.warnings[0].contains("mcp_erp"),
            "warning should mention the tool name"
        );
        registry.shutdown().await;
    }

    #[tokio::test]
    async fn test_dangerous_required_tool_without_flag_blocked() {
        // GIVEN: registry with a dangerous tool, manifest without dangerous_tools_allowed
        let registry = registry_with_dangerous_tool("risky_tool").await;
        let mut manifest = minimal_manifest();
        manifest.tools_required = vec!["risky_tool".to_string()];
        manifest.dangerous_tools_allowed = false;
        // WHEN
        let result = resolve(&manifest, &registry).await;
        // THEN
        assert!(
            matches!(result, Err(ToolResolutionError::DangerousToolNotAllowed(_))),
            "expected DangerousToolNotAllowed, got: {:?}",
            result.err()
        );
        registry.shutdown().await;
    }

    #[tokio::test]
    async fn test_dangerous_tool_allowed_when_flag_set() {
        // GIVEN: registry with a dangerous tool, manifest with dangerous_tools_allowed=true
        let registry = registry_with_dangerous_tool("risky_tool").await;
        let mut manifest = minimal_manifest();
        manifest.tools_required = vec!["risky_tool".to_string()];
        manifest.dangerous_tools_allowed = true;
        // WHEN
        let result = resolve(&manifest, &registry).await;
        // THEN
        let report = result.expect("dangerous tool should be allowed when flag is set");
        assert!(matches!(report.status, ResolutionStatus::AllResolved));
        assert_eq!(report.resolved, vec!["risky_tool"]);
        registry.shutdown().await;
    }

    #[tokio::test]
    async fn test_empty_manifest_resolves_immediately() {
        // GIVEN: empty registry, empty manifest
        let registry = ToolRegistryHandle::start();
        let manifest = minimal_manifest();
        // WHEN
        let result = resolve(&manifest, &registry).await;
        // THEN
        let report = result.expect("empty manifest should always succeed");
        assert!(matches!(report.status, ResolutionStatus::AllResolved));
        assert!(report.warnings.is_empty());
        assert!(report.resolved.is_empty());
        registry.shutdown().await;
    }

    #[tokio::test]
    async fn test_fail_fast_stops_at_first_missing_required() {
        // GIVEN: only "file_read" present, manifest requires ["bash_executor", "file_read"]
        let registry = registry_with_tools(&["file_read"]).await;
        let mut manifest = minimal_manifest();
        manifest.tools_required = vec!["bash_executor".to_string(), "file_read".to_string()];
        // WHEN
        let result = resolve(&manifest, &registry).await;
        // THEN: fails on first missing, not second
        assert!(matches!(
            result,
            Err(ToolResolutionError::RequiredToolMissing(ref name)) if name == "bash_executor"
        ));
        registry.shutdown().await;
    }

    #[tokio::test]
    async fn test_a2a_dependency_in_optional_does_not_degrade() {
        // GIVEN: empty registry, manifest declares only A2A optional deps
        let registry = ToolRegistryHandle::start();
        let mut manifest = minimal_manifest();
        manifest.tools_optional = vec!["a2a:foo".to_string(), "a2a:bar".to_string()];
        // WHEN
        let result = resolve(&manifest, &registry).await;
        // THEN: A2A deps are resolved without registry lookup, no warnings
        let report = result.expect("A2A optional deps should resolve without registry lookup");
        assert!(matches!(report.status, ResolutionStatus::AllResolved));
        assert_eq!(report.resolved, vec!["a2a:foo", "a2a:bar"]);
        assert!(report.warnings.is_empty());
        registry.shutdown().await;
    }

    #[tokio::test]
    async fn test_a2a_dependency_in_required_passes_without_registry_lookup() {
        // GIVEN: empty registry, manifest declares an A2A required dep
        let registry = ToolRegistryHandle::start();
        let mut manifest = minimal_manifest();
        manifest.tools_required = vec!["a2a:critical-skill".to_string()];
        // WHEN
        let result = resolve(&manifest, &registry).await;
        // THEN: no RequiredToolMissing, AllResolved
        let report = result.expect("A2A required dep should resolve without registry lookup");
        assert!(matches!(report.status, ResolutionStatus::AllResolved));
        assert_eq!(report.resolved, vec!["a2a:critical-skill"]);
        assert!(report.warnings.is_empty());
        registry.shutdown().await;
    }

    #[tokio::test]
    async fn test_mixed_a2a_and_native_optional() {
        // GIVEN: registry contains "file_read" only; manifest mixes native + A2A + missing native
        let registry = registry_with_tools(&["file_read"]).await;
        let mut manifest = minimal_manifest();
        manifest.tools_optional = vec![
            "file_read".to_string(),
            "a2a:foo".to_string(),
            "missing-native".to_string(),
        ];
        // WHEN
        let result = resolve(&manifest, &registry).await;
        // THEN: Degraded only because of missing native; A2A resolved silently
        let report = result.expect("should succeed");
        assert!(matches!(report.status, ResolutionStatus::Degraded));
        assert!(report.resolved.contains(&"file_read".to_string()));
        assert!(report.resolved.contains(&"a2a:foo".to_string()));
        assert_eq!(report.warnings.len(), 1);
        assert!(report.warnings[0].contains("missing-native"));
        registry.shutdown().await;
    }

    #[tokio::test]
    async fn test_a2a_prefix_is_exact_match() {
        // GIVEN: empty registry, manifest declares two entries that are NOT exact A2A matches
        let registry = ToolRegistryHandle::start();
        let mut manifest = minimal_manifest();
        manifest.tools_optional = vec!["a2asomething".to_string(), "not-a2a:foo".to_string()];
        // WHEN
        let result = resolve(&manifest, &registry).await;
        // THEN: both go through registry lookup path, both missing, Degraded with 2 warnings
        let report = result.expect("should succeed");
        assert!(matches!(report.status, ResolutionStatus::Degraded));
        assert_eq!(report.warnings.len(), 2);
        assert!(report.resolved.is_empty());
        registry.shutdown().await;
    }

    #[tokio::test]
    async fn test_optional_present_and_missing_mix() {
        // GIVEN: "file_read" present, "mcp_erp" absent
        let registry = registry_with_tools(&["file_read"]).await;
        let mut manifest = minimal_manifest();
        manifest.tools_optional = vec!["file_read".to_string(), "mcp_erp".to_string()];
        // WHEN
        let result = resolve(&manifest, &registry).await;
        // THEN: Degraded because of missing optional, but resolved contains file_read
        let report = result.expect("should succeed");
        assert!(matches!(report.status, ResolutionStatus::Degraded));
        assert_eq!(report.resolved, vec!["file_read"]);
        assert_eq!(report.warnings.len(), 1);
        registry.shutdown().await;
    }
}
