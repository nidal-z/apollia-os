//! Workflows: read one, launch one, read its history, read a run, stop a run.

use crate::client::{extract_error, ClientError, RawResponse, RuntimeClient};

fn json_or_error(resp: RawResponse) -> Result<serde_json::Value, ClientError> {
    if resp.status >= 400 {
        return Err(ClientError::ServerError {
            status: resp.status,
            body: extract_error(&resp.body, resp.status),
        });
    }
    Ok(serde_json::from_str(&resp.body)?)
}

impl RuntimeClient {
    /// A workflow and the fields of its inputs via
    /// `GET /api/v1/workflows/{package}/{workflow}`.
    pub async fn get_workflow(
        &self,
        package: &str,
        workflow: &str,
    ) -> Result<serde_json::Value, ClientError> {
        json_or_error(
            self.get(&format!("/api/v1/workflows/{package}/{workflow}"))
                .await?,
        )
    }

    /// Launch a workflow via `POST /api/v1/workflows/{package}/{workflow}/runs`.
    pub async fn launch_workflow(
        &self,
        package: &str,
        workflow: &str,
        inputs: &serde_json::Value,
    ) -> Result<serde_json::Value, ClientError> {
        let body = serde_json::json!({ "inputs": inputs });
        json_or_error(
            self.post(
                &format!("/api/v1/workflows/{package}/{workflow}/runs"),
                Some(&body),
            )
            .await?,
        )
    }

    /// The runs of a workflow via `GET /api/v1/workflows/{package}/{workflow}/runs`.
    pub async fn list_workflow_runs(
        &self,
        package: &str,
        workflow: &str,
        limit: u32,
    ) -> Result<serde_json::Value, ClientError> {
        json_or_error(
            self.get(&format!(
                "/api/v1/workflows/{package}/{workflow}/runs?limit={limit}"
            ))
            .await?,
        )
    }

    /// A run and its steps via `GET /api/v1/workflow-runs/{id}`.
    pub async fn get_workflow_run(&self, id: &str) -> Result<serde_json::Value, ClientError> {
        json_or_error(self.get(&format!("/api/v1/workflow-runs/{id}")).await?)
    }

    /// Stop a run via `POST /api/v1/workflow-runs/{id}/cancel`.
    pub async fn cancel_workflow_run(&self, id: &str) -> Result<serde_json::Value, ClientError> {
        json_or_error(
            self.post(&format!("/api/v1/workflow-runs/{id}/cancel"), None)
                .await?,
        )
    }
}
