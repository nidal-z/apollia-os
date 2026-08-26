//! MCP servers, STT configuration, audit, approvals and resilience.

use crate::client::{encode_path_segment, extract_error, ClientError, RuntimeClient};

impl RuntimeClient {
    // ─── MCP Servers CRUD ─────────────────────────────────────────────────────

    /// Add an MCP server to the runtime via `POST /api/v1/mcp/servers`.
    pub async fn add_mcp_server(
        &self,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, ClientError> {
        let resp = self.post("/api/v1/mcp/servers", Some(body)).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Get MCP server details via `GET /api/v1/mcp/servers/{name}`.
    pub async fn get_mcp_server_detail(
        &self,
        name: &str,
    ) -> Result<serde_json::Value, ClientError> {
        let resp = self.get(&format!("/api/v1/mcp/servers/{name}")).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Remove an MCP server from the runtime via `DELETE /api/v1/mcp/servers/{name}`.
    pub async fn remove_mcp_server(&self, name: &str) -> Result<serde_json::Value, ClientError> {
        let resp = self.delete(&format!("/api/v1/mcp/servers/{name}")).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Test an MCP server connection via `POST /api/v1/mcp/servers/test`.
    pub async fn test_mcp_connection(
        &self,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, ClientError> {
        let resp = self.post("/api/v1/mcp/servers/test", Some(body)).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Test an already-installed MCP server by name via
    /// `POST /api/v1/mcp/servers/{name}/test`. Returns `404` for an unknown
    /// server (the name-based route is existence-aware).
    pub async fn test_live_mcp_server(&self, name: &str) -> Result<serde_json::Value, ClientError> {
        let body = serde_json::json!({});
        let resp = self
            .post(&format!("/api/v1/mcp/servers/{name}/test"), Some(&body))
            .await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Restart an MCP server via `POST /api/v1/mcp/servers/{name}/restart`.
    pub async fn restart_mcp_server(&self, name: &str) -> Result<serde_json::Value, ClientError> {
        let resp = self
            .post(&format!("/api/v1/mcp/servers/{name}/restart"), None)
            .await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Update an MCP server configuration via `PUT /api/v1/mcp/servers/{name}/config`.
    pub async fn update_mcp_server_config(
        &self,
        name: &str,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, ClientError> {
        let uri = format!("/api/v1/mcp/servers/{name}/config");
        let resp = self.put(&uri, Some(body)).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    // ─── STT Config ───────────────────────────────────────────────────────────

    /// Get the STT configuration via `GET /api/v1/stt/config`.
    pub async fn get_stt_config(&self) -> Result<serde_json::Value, ClientError> {
        let resp = self.get("/api/v1/stt/config").await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Update the STT configuration via `PUT /api/v1/stt/config`.
    pub async fn update_stt_config(
        &self,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, ClientError> {
        let resp = self.put("/api/v1/stt/config", Some(body)).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Delete a transcription by ID via `DELETE /api/v1/stt/transcriptions/{id}`.
    pub async fn delete_transcription(&self, id: &str) -> Result<serde_json::Value, ClientError> {
        let resp = self
            .delete(&format!("/api/v1/stt/transcriptions/{id}"))
            .await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        // The runtime returns 204 No Content (empty body) on success (delete is
        // idempotent), so an empty body is success, not malformed JSON.
        if resp.body.trim().is_empty() {
            return Ok(serde_json::json!({ "deleted": id }));
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    // ─── Audit ────────────────────────────────────────────────────────────────

    /// Get audit statistics via `GET /api/v1/audit/stats`.
    pub async fn get_audit_stats(&self) -> Result<serde_json::Value, ClientError> {
        let resp = self.get("/api/v1/audit/stats").await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    // ─── Approvals ────────────────────────────────────────────────────────────

    /// List resolved HITL approvals via `GET /api/v1/approvals/resolved`.
    pub async fn list_resolved_approvals(&self) -> Result<serde_json::Value, ClientError> {
        let resp = self.get("/api/v1/approvals/resolved").await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    // ─── Agent Logs ───────────────────────────────────────────────────────────

    /// Fetch recent log lines for an agent via `GET /api/v1/agents/{id}/logs?last={n}`.
    ///
    /// Returns a JSON value with a `"logs"` array of log line strings.
    pub async fn get_agent_logs(
        &self,
        agent_id: &str,
        last: u32,
    ) -> Result<serde_json::Value, ClientError> {
        let resp = self
            .get(&format!("/api/v1/agents/{agent_id}/logs?last={last}"))
            .await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    // ─── Resilience ───────────────────────────────────────────────────────────

    /// List all circuit breakers via `GET /api/v1/resilience/status`.
    ///
    /// Returns `{ "circuit_breakers": [ { tool_name, state, failure_count,
    /// cooldown_remaining_secs } ] }`.
    pub async fn resilience_list(&self) -> Result<serde_json::Value, ClientError> {
        let resp = self.get("/api/v1/resilience/status").await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Show the circuit breaker state for one tool via `GET /api/v1/resilience/status/{tool}`.
    ///
    /// Returns 404 when the tool has no recorded circuit breaker yet.
    pub async fn resilience_show(&self, tool_name: &str) -> Result<serde_json::Value, ClientError> {
        let resp = self
            .get(&format!(
                "/api/v1/resilience/status/{}",
                encode_path_segment(tool_name)
            ))
            .await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Reset a circuit breaker to CLOSED via `POST /api/v1/resilience/reset/{tool}`.
    ///
    /// Returns 404 when the tool has no recorded circuit breaker yet.
    pub async fn resilience_reset(
        &self,
        tool_name: &str,
    ) -> Result<serde_json::Value, ClientError> {
        let resp = self
            .post(
                &format!(
                    "/api/v1/resilience/reset/{}",
                    encode_path_segment(tool_name)
                ),
                None,
            )
            .await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Add a permission prefix rule via `POST /api/v1/permissions/prefix`.
    pub async fn add_permission_prefix_rule(
        &self,
        prefix: &str,
        action: &str,
    ) -> Result<serde_json::Value, ClientError> {
        let body = serde_json::json!({ "prefix": prefix, "action": action });
        let resp = self.post("/api/v1/permissions/prefix", Some(&body)).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }
}
