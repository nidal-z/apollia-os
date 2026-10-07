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

    /// Make the stored agent `agent` available via `POST /api/v1/agents`.
    pub async fn start_agent(&self, agent: &str) -> Result<serde_json::Value, ClientError> {
        let body = serde_json::json!({ "agent": agent });
        let resp = self.post("/api/v1/agents", Some(&body)).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Every stored agent, resolved, via `GET /api/v1/agent-definitions`.
    pub async fn list_agent_definitions(&self) -> Result<serde_json::Value, ClientError> {
        let resp = self.get("/api/v1/agent-definitions").await?;
        definition_body(resp)
    }

    /// One stored agent, resolved (its definition then the user's overrides),
    /// via `GET /api/v1/agent-definitions/{name}`.
    pub async fn get_agent_definition(&self, name: &str) -> Result<serde_json::Value, ClientError> {
        let resp = self
            .get(&format!("/api/v1/agent-definitions/{name}"))
            .await?;
        definition_body(resp)
    }

    /// Set the user's value of `field` for `name` via
    /// `PUT /api/v1/agent-definitions/{name}/overrides/{field}`.
    pub async fn set_agent_override(
        &self,
        name: &str,
        field: &str,
        value: serde_json::Value,
    ) -> Result<serde_json::Value, ClientError> {
        let body = serde_json::json!({ "value": value });
        let resp = self
            .put(
                &format!("/api/v1/agent-definitions/{name}/overrides/{field}"),
                Some(&body),
            )
            .await?;
        definition_body(resp)
    }

    /// Remove the user's value of `field` for `name` via
    /// `DELETE /api/v1/agent-definitions/{name}/overrides/{field}`.
    pub async fn remove_agent_override(
        &self,
        name: &str,
        field: &str,
    ) -> Result<serde_json::Value, ClientError> {
        let resp = self
            .delete(&format!(
                "/api/v1/agent-definitions/{name}/overrides/{field}"
            ))
            .await?;
        definition_body(resp)
    }

    /// Back to defaults for `name` via
    /// `DELETE /api/v1/agent-definitions/{name}/overrides`.
    pub async fn reset_agent_overrides(
        &self,
        name: &str,
    ) -> Result<serde_json::Value, ClientError> {
        let resp = self
            .delete(&format!("/api/v1/agent-definitions/{name}/overrides"))
            .await?;
        definition_body(resp)
    }

    /// Assign a model to `agent` via `PUT /api/v1/llm/agents/{agent}/model`;
    /// `body` carries `model_source` and `model_id`.
    pub async fn set_agent_model(
        &self,
        agent: &str,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, ClientError> {
        let resp = self
            .put(&format!("/api/v1/llm/agents/{agent}/model"), Some(body))
            .await?;
        definition_body(resp)
    }

    /// Remove the model assigned to `agent` via
    /// `DELETE /api/v1/llm/agents/{agent}/model`.
    pub async fn clear_agent_model(&self, agent: &str) -> Result<serde_json::Value, ClientError> {
        let resp = self
            .delete(&format!("/api/v1/llm/agents/{agent}/model"))
            .await?;
        definition_body(resp)
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

/// The JSON body of an agent-definition route, or its error.
/// An exported agent: the archive and the report the runtime gave with it.
#[derive(Debug, Clone)]
pub struct ExportedAgent {
    /// The bytes of the `.apo` archive.
    pub bytes: Vec<u8>,
    /// The report of the export (`file_name`, `publisher`, `signed`,
    /// `key_fingerprint`, `removed_access`, `required_connections`).
    pub report: serde_json::Value,
}

/// Header that carries the report of an export.
const EXPORT_REPORT_HEADER: &str = "x-apollia-export-report";

impl RuntimeClient {
    /// Export the agent `name` as a `.apo` archive via
    /// `POST /api/v1/agent-definitions/{name}/export`, signed when `sign`,
    /// under `publisher` when given (else the `author.name` setting).
    pub async fn export_agent(
        &self,
        name: &str,
        sign: bool,
        publisher: Option<&str>,
    ) -> Result<ExportedAgent, ClientError> {
        let body = serde_json::json!({ "sign": sign, "publisher": publisher });
        let (status, headers, bytes) = self
            .post_for_file(&format!("/api/v1/agent-definitions/{name}/export"), &body)
            .await?;
        if status >= 400 {
            let text = String::from_utf8_lossy(&bytes);
            return Err(ClientError::ServerError {
                status,
                body: extract_error(&text, status),
            });
        }
        let report = headers
            .get(EXPORT_REPORT_HEADER)
            .and_then(|v| v.to_str().ok())
            .map(serde_json::from_str)
            .transpose()?
            .unwrap_or(serde_json::Value::Null);
        Ok(ExportedAgent {
            bytes: bytes.to_vec(),
            report,
        })
    }
}

fn definition_body(resp: crate::client::RawResponse) -> Result<serde_json::Value, ClientError> {
    if resp.status >= 400 {
        return Err(ClientError::ServerError {
            status: resp.status,
            body: extract_error(&resp.body, resp.status),
        });
    }
    Ok(serde_json::from_str(&resp.body)?)
}
