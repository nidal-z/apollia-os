//! Reviews, LLM backends, trigger and notification writes, pipelines.

use crate::client::{extract_error, ClientError, RuntimeClient};

impl RuntimeClient {
    /// Request a code review for an existing task via `POST /api/v1/tasks/{id}/review`.
    ///
    /// Blocks until the `apollia-review` agent completes (up to 120 s on the server side).
    /// Returns the raw JSON `ReviewReport` value on success.
    pub async fn post_task_review(&self, task_id: &str) -> Result<serde_json::Value, ClientError> {
        let resp = self
            .post(&format!("/api/v1/tasks/{task_id}/review"), None)
            .await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Submit a review directly to the `apollia-review` agent via `POST /api/v1/tasks`.
    ///
    /// Used when the caller supplies a PR number or diff file path rather than an
    /// existing task ID.  The `inputs` value is forwarded as a `data` part in the
    /// AIP input so the agent can extract `pr_number` / `diff_file`.
    pub async fn submit_review(
        &self,
        inputs: serde_json::Value,
    ) -> Result<serde_json::Value, ClientError> {
        let body = serde_json::json!({
            "agent_id": "apollia-review",
            "input": {
                "parts": [{ "type": "data", "data": inputs }]
            }
        });
        let resp = self.post("/api/v1/tasks", Some(&body)).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    // ─── LLM Backends CRUD ────────────────────────────────────────────────────

    /// List all configured LLM backends via `GET /api/v1/llm/backends`.
    pub async fn list_llm_backends(&self) -> Result<serde_json::Value, ClientError> {
        let resp = self.get("/api/v1/llm/backends").await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Fetch a single LLM backend via `GET /api/v1/llm/backends/{name}`.
    ///
    /// Returns the full configuration object including `provider`, `model`,
    /// `config_json`, `enabled`, and `is_default`. Used by `llm backends show`
    /// and by the CLI update path to merge partial flags with the existing
    /// state before re-submitting a full body to the route (`PUT` is replace,
    /// not patch).
    pub async fn get_llm_backend(&self, name: &str) -> Result<serde_json::Value, ClientError> {
        let resp = self.get(&format!("/api/v1/llm/backends/{name}")).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Create a new LLM backend via `POST /api/v1/llm/backends`.
    pub async fn create_llm_backend(
        &self,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, ClientError> {
        let resp = self.post("/api/v1/llm/backends", Some(body)).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Update an existing LLM backend via `PUT /api/v1/llm/backends/{name}`.
    pub async fn update_llm_backend(
        &self,
        name: &str,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, ClientError> {
        let uri = format!("/api/v1/llm/backends/{name}");
        let resp = self.put(&uri, Some(body)).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Delete an LLM backend via `DELETE /api/v1/llm/backends/{name}`.
    pub async fn delete_llm_backend(&self, name: &str) -> Result<serde_json::Value, ClientError> {
        let resp = self.delete(&format!("/api/v1/llm/backends/{name}")).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Rebuild the in-memory `LlmRouter` from `system.db` via
    /// `POST /api/v1/llm/reload`.
    ///
    /// Returns the list of backends that came up in the new router so the
    /// CLI can confirm the swap without a follow-up `status` call.
    pub async fn reload_llm_router(&self) -> Result<serde_json::Value, ClientError> {
        let resp = self.post("/api/v1/llm/reload", None).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Set a backend as the default LLM backend via `POST /api/v1/llm/backends/{name}/set-default`.
    pub async fn set_default_llm_backend(
        &self,
        name: &str,
    ) -> Result<serde_json::Value, ClientError> {
        let resp = self
            .post(&format!("/api/v1/llm/backends/{name}/set-default"), None)
            .await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Get aggregated LLM usage and costs via `GET /api/v1/llm/costs`.
    pub async fn get_llm_costs(&self) -> Result<serde_json::Value, ClientError> {
        let resp = self.get("/api/v1/llm/costs").await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    // ─── Triggers CRUD ────────────────────────────────────────────────────────

    /// Create a new trigger via `POST /api/v1/triggers`.
    pub async fn create_trigger(
        &self,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, ClientError> {
        let resp = self.post("/api/v1/triggers", Some(body)).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Update an existing trigger via `PUT /api/v1/triggers/{id}`.
    pub async fn update_trigger(
        &self,
        id: &str,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, ClientError> {
        let uri = format!("/api/v1/triggers/{id}");
        let resp = self.put(&uri, Some(body)).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Delete a trigger via `DELETE /api/v1/triggers/{id}`.
    pub async fn delete_trigger(&self, id: &str) -> Result<serde_json::Value, ClientError> {
        let resp = self.delete(&format!("/api/v1/triggers/{id}")).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    // ─── Notifications CRUD ───────────────────────────────────────────────────

    /// Create a notification channel via `POST /api/v1/notifications/channels`.
    pub async fn create_notification_channel(
        &self,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, ClientError> {
        let resp = self
            .post("/api/v1/notifications/channels", Some(body))
            .await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Update a notification channel via `PUT /api/v1/notifications/channels/{id}`.
    pub async fn update_notification_channel(
        &self,
        id: &str,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, ClientError> {
        let uri = format!("/api/v1/notifications/channels/{id}");
        let resp = self.put(&uri, Some(body)).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Delete a notification channel via `DELETE /api/v1/notifications/channels/{id}`.
    pub async fn delete_notification_channel(
        &self,
        id: &str,
    ) -> Result<serde_json::Value, ClientError> {
        let resp = self
            .delete(&format!("/api/v1/notifications/channels/{id}"))
            .await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Get notification event types configuration via `GET /api/v1/notifications/events`.
    pub async fn get_notification_events(&self) -> Result<serde_json::Value, ClientError> {
        let resp = self.get("/api/v1/notifications/events").await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Update notification event types via `PUT /api/v1/notifications/events`.
    pub async fn set_notification_events(
        &self,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, ClientError> {
        let resp = self.put("/api/v1/notifications/events", Some(body)).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    // ─── Pipelines CRUD ───────────────────────────────────────────────────────

    /// Create a new pipeline via `POST /api/v1/pipelines`.
    pub async fn create_pipeline(
        &self,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, ClientError> {
        let resp = self.post("/api/v1/pipelines", Some(body)).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Get pipeline details via `GET /api/v1/pipelines/{id}`.
    pub async fn get_pipeline(&self, id: &str) -> Result<serde_json::Value, ClientError> {
        let resp = self.get(&format!("/api/v1/pipelines/{id}")).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Update a pipeline via `PUT /api/v1/pipelines/{id}`.
    pub async fn update_pipeline(
        &self,
        id: &str,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, ClientError> {
        let uri = format!("/api/v1/pipelines/{id}");
        let resp = self.put(&uri, Some(body)).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Delete a pipeline via `DELETE /api/v1/pipelines/{id}`.
    pub async fn delete_pipeline(&self, id: &str) -> Result<serde_json::Value, ClientError> {
        let resp = self.delete(&format!("/api/v1/pipelines/{id}")).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }
}
