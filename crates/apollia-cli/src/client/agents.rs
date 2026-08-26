//! Runtime health, the agent inventory and the task submission verbs.

use crate::client::{extract_error, ClientError, RuntimeClient};

impl RuntimeClient {
    /// Check if the runtime is healthy by calling `GET /api/v1/health`.
    pub async fn health(&self) -> Result<serde_json::Value, ClientError> {
        let resp = self.get("/api/v1/health").await?;
        if resp.status != 200 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: resp.body,
            });
        }
        let json: serde_json::Value = serde_json::from_str(&resp.body)?;
        Ok(json)
    }

    /// Request shutdown via `POST /api/v1/shutdown`.
    pub async fn shutdown(&self) -> Result<serde_json::Value, ClientError> {
        let resp = self.post("/api/v1/shutdown", None).await?;
        let json: serde_json::Value = serde_json::from_str(&resp.body)?;
        Ok(json)
    }

    /// List all agents via `GET /api/v1/agents`.
    pub async fn list_agents(&self) -> Result<serde_json::Value, ClientError> {
        let resp = self.get("/api/v1/agents").await?;
        if resp.status != 200 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: resp.body,
            });
        }
        let json: serde_json::Value = serde_json::from_str(&resp.body)?;
        Ok(json)
    }

    /// List A2A-capable agents via `GET /api/v1/agents?supports_a2a=true`.
    ///
    /// Returns all agents that declare `supports_a2a = true` in their manifest,
    /// including their skill descriptors and version.
    pub async fn list_a2a_agents(&self) -> Result<serde_json::Value, ClientError> {
        let resp = self.get("/api/v1/agents?supports_a2a=true").await?;
        if resp.status != 200 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: resp.body,
            });
        }
        let json: serde_json::Value = serde_json::from_str(&resp.body)?;
        Ok(json)
    }

    /// Start (register) a new agent via `POST /api/v1/agents`.
    pub async fn start_agent(&self, agent_path: &str) -> Result<serde_json::Value, ClientError> {
        let body = serde_json::json!({ "agent_path": agent_path });
        let resp = self.post("/api/v1/agents", Some(&body)).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Get agent detail via `GET /api/v1/agents/{id}`.
    pub async fn get_agent(&self, agent_id: &str) -> Result<serde_json::Value, ClientError> {
        let resp = self.get(&format!("/api/v1/agents/{agent_id}")).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// List in-memory A2A messages for an agent via
    /// `GET /api/v1/agents/{id}/messages?limit=N`.
    ///
    /// Returns the full JSON response body (`{ "messages": [...] }`) so the
    /// caller can render either tabular or JSON output without re-parsing.
    pub async fn list_agent_messages(
        &self,
        agent_id: &str,
        limit: Option<u32>,
    ) -> Result<serde_json::Value, ClientError> {
        let uri = match limit {
            Some(n) => format!("/api/v1/agents/{agent_id}/messages?limit={n}"),
            None => format!("/api/v1/agents/{agent_id}/messages"),
        };
        let resp = self.get(&uri).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Stop an agent via `DELETE /api/v1/agents/{id}`.
    pub async fn stop_agent(&self, agent_id: &str) -> Result<serde_json::Value, ClientError> {
        let resp = self.delete(&format!("/api/v1/agents/{agent_id}")).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Submit a task via `POST /api/v1/tasks`.
    pub async fn submit_task(
        &self,
        agent_id: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, ClientError> {
        self.submit_task_with_options(agent_id, input, serde_json::Value::Null)
            .await
    }

    /// Submit a task with per-run control options (`run_options`).
    ///
    /// `run_options` is forwarded verbatim under the `run_options` key so the
    /// runtime can honour CLI flags (`--plan`, `--autonomy`). Pass
    /// [`serde_json::Value::Null`] to send no options.
    pub async fn submit_task_with_options(
        &self,
        agent_id: &str,
        input: serde_json::Value,
        run_options: serde_json::Value,
    ) -> Result<serde_json::Value, ClientError> {
        let mut body = serde_json::json!({
            "agent_id": agent_id,
            "input": input,
        });
        if !run_options.is_null() {
            body["run_options"] = run_options;
        }
        let resp = self.post("/api/v1/tasks", Some(&body)).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// List A2A skills via `GET /api/v1/a2a/skills`.
    ///
    /// Returns a flat list of every skill exposed by active Worker Agents,
    /// each carrying `skill_id`, `agent_name`, and the input schema.
    pub async fn list_a2a_skills(&self) -> Result<serde_json::Value, ClientError> {
        let resp = self.get("/api/v1/a2a/skills").await?;
        if resp.status != 200 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Invoke an A2A Worker Agent skill via `POST /api/v1/a2a/invoke`.
    ///
    /// Routes by `skill_id` (not agent id) so the caller does not need to know
    /// which worker provides it. `timeout_secs = None` falls back to the
    /// runtime default (120 s).
    pub async fn invoke_a2a_skill(
        &self,
        skill_id: &str,
        input: serde_json::Value,
        timeout_secs: Option<u64>,
        caller: Option<&str>,
    ) -> Result<serde_json::Value, ClientError> {
        let mut body = serde_json::json!({
            "skill_id": skill_id,
            "input": input,
        });
        if let Some(t) = timeout_secs {
            body["timeout_secs"] = serde_json::Value::from(t);
        }
        if let Some(c) = caller {
            body["caller"] = serde_json::Value::String(c.to_string());
        }
        let resp = self.post("/api/v1/a2a/invoke", Some(&body)).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Get task status via `GET /api/v1/tasks/{id}`.
    pub async fn get_task(&self, task_id: &str) -> Result<serde_json::Value, ClientError> {
        let resp = self.get(&format!("/api/v1/tasks/{task_id}")).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }
}
