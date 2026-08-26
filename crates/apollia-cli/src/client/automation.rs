//! Triggers, notification channels, pipelines and task cancellation.

use crate::client::{extract_error, ChannelTestResult, ClientError, RawResponse, RuntimeClient};

impl RuntimeClient {
    /// List all triggers via `GET /api/v1/triggers`.
    pub async fn list_triggers(&self) -> Result<serde_json::Value, ClientError> {
        let resp = self.get("/api/v1/triggers").await?;
        if resp.status != 200 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: resp.body,
            });
        }
        let json: serde_json::Value = serde_json::from_str(&resp.body)?;
        Ok(json)
    }

    /// Get trigger detail via `GET /api/v1/triggers/{id}`.
    pub async fn get_trigger(&self, id: &str) -> Result<serde_json::Value, ClientError> {
        let resp = self.get(&format!("/api/v1/triggers/{id}")).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Fire a trigger immediately via `POST /api/v1/triggers/{id}/fire`.
    pub async fn fire_trigger(&self, id: &str) -> Result<serde_json::Value, ClientError> {
        let resp = self
            .post(&format!("/api/v1/triggers/{id}/fire"), None)
            .await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Enable a trigger via `POST /api/v1/triggers/{id}/enable`.
    pub async fn enable_trigger(&self, id: &str) -> Result<serde_json::Value, ClientError> {
        let resp = self
            .post(&format!("/api/v1/triggers/{id}/enable"), None)
            .await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Disable a trigger via `POST /api/v1/triggers/{id}/disable`.
    pub async fn disable_trigger(&self, id: &str) -> Result<serde_json::Value, ClientError> {
        let resp = self
            .post(&format!("/api/v1/triggers/{id}/disable"), None)
            .await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Get trigger logs via `GET /api/v1/triggers/{id}/logs?last={last}`.
    pub async fn get_trigger_logs(
        &self,
        id: &str,
        last: usize,
    ) -> Result<serde_json::Value, ClientError> {
        let resp = self
            .get(&format!("/api/v1/triggers/{id}/logs?last={last}"))
            .await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Hot-reload triggers via `POST /api/v1/triggers/reload`.
    ///
    /// Returns the JSON response on success (`{ "reloaded": <count> }`).
    pub async fn reload_triggers(&self) -> Result<serde_json::Value, ClientError> {
        let resp = self.post("/api/v1/triggers/reload", None).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Resume a HITL task via `POST /api/v1/tasks/{id}/resume`.
    ///
    /// Returns the raw response so the caller can handle HTTP 409
    /// (task not in `input_required` state) distinctly from other errors.
    pub async fn resume_task(
        &self,
        task_id: &str,
        approved: bool,
        reason: Option<String>,
    ) -> Result<RawResponse, ClientError> {
        let mut body = serde_json::json!({ "approved": approved });
        if let Some(r) = reason {
            body["reason"] = serde_json::Value::String(r);
        }
        self.post(&format!("/api/v1/tasks/{task_id}/resume"), Some(&body))
            .await
    }

    /// Send a test notification through all active channels via `POST /api/v1/notifications/test`.
    ///
    /// Returns the per-channel results (status, latency, error).
    pub async fn test_notifications(&self) -> Result<Vec<ChannelTestResult>, ClientError> {
        let resp = self.post("/api/v1/notifications/test", None).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: resp.body,
            });
        }
        let json: serde_json::Value = serde_json::from_str(&resp.body)?;
        let results_val = json
            .get("results")
            .cloned()
            .unwrap_or(serde_json::Value::Array(vec![]));
        let results: Vec<ChannelTestResult> = serde_json::from_value(results_val)?;
        Ok(results)
    }

    /// List configured notification channels via `GET /api/v1/notifications/channels`.
    ///
    /// Returns the raw JSON value from the `"channels"` array.
    pub async fn list_notification_channels(&self) -> Result<serde_json::Value, ClientError> {
        let resp = self.get("/api/v1/notifications/channels").await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: resp.body,
            });
        }
        let json: serde_json::Value = serde_json::from_str(&resp.body)?;
        Ok(json)
    }

    /// Fetch notification log history via `GET /api/v1/notifications/logs?last={last}`.
    ///
    /// Returns the raw JSON value from the `"entries"` array.
    pub async fn notification_logs(&self, last: usize) -> Result<serde_json::Value, ClientError> {
        let resp = self
            .get(&format!("/api/v1/notifications/logs?last={last}"))
            .await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: resp.body,
            });
        }
        let json: serde_json::Value = serde_json::from_str(&resp.body)?;
        Ok(json)
    }

    /// List all registered pipelines via `GET /api/v1/pipelines`.
    ///
    /// Returns a JSON value with a `"pipelines"` array, each item having `"id"` and
    /// `"description"`.
    pub async fn list_pipelines(&self) -> Result<serde_json::Value, ClientError> {
        let resp = self.get("/api/v1/pipelines").await?;
        if resp.status != 200 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: resp.body,
            });
        }
        let json: serde_json::Value = serde_json::from_str(&resp.body)?;
        Ok(json)
    }

    /// Trigger a new pipeline run via `POST /api/v1/pipelines/{id}/run`.
    ///
    /// Returns the run summary including `run_id` on success.
    pub async fn run_pipeline(
        &self,
        id: &str,
        input: Option<&str>,
    ) -> Result<serde_json::Value, ClientError> {
        let body = match input {
            Some(i) => serde_json::json!({ "input": i }),
            None => serde_json::json!({}),
        };
        let resp = self
            .post(&format!("/api/v1/pipelines/{id}/run"), Some(&body))
            .await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// List recent runs for a pipeline via `GET /api/v1/pipelines/{id}/runs`.
    ///
    /// The server returns at most 20 runs. `limit` is a client-side cap applied
    /// on top of the server response.
    pub async fn list_pipeline_runs(
        &self,
        pipeline_id: &str,
        limit: u32,
    ) -> Result<serde_json::Value, ClientError> {
        let resp = self
            .get(&format!("/api/v1/pipelines/{pipeline_id}/runs"))
            .await?;
        if resp.status != 200 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: resp.body,
            });
        }
        let mut json: serde_json::Value = serde_json::from_str(&resp.body)?;
        // Apply client-side limit to the array.
        if let Some(arr) = json.as_array_mut() {
            arr.truncate(limit as usize);
        }
        Ok(json)
    }

    /// Get the detailed status of a pipeline run via `GET /api/v1/runs/{run_id}`.
    ///
    /// This endpoint does not require the pipeline id in the URL.
    pub async fn get_pipeline_run(&self, run_id: &str) -> Result<serde_json::Value, ClientError> {
        let resp = self.get(&format!("/api/v1/runs/{run_id}")).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }

    /// Cancel a task via `DELETE /api/v1/tasks/{id}`.
    pub async fn cancel_task(&self, task_id: &str) -> Result<serde_json::Value, ClientError> {
        let resp = self.delete(&format!("/api/v1/tasks/{task_id}")).await?;
        if resp.status >= 400 {
            return Err(ClientError::ServerError {
                status: resp.status,
                body: extract_error(&resp.body, resp.status),
            });
        }
        Ok(serde_json::from_str(&resp.body)?)
    }
}
