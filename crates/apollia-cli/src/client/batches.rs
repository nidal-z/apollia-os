//! Batches: launch an agent on every item of a source, read one, list them.

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
    /// Launch a batch via `POST /api/v1/batches`.
    pub async fn launch_batch(
        &self,
        launch: &serde_json::Value,
    ) -> Result<serde_json::Value, ClientError> {
        json_or_error(self.post("/api/v1/batches", Some(launch)).await?)
    }

    /// Read a batch and its items via `GET /api/v1/batches/{id}`.
    pub async fn get_batch(&self, id: &str) -> Result<serde_json::Value, ClientError> {
        json_or_error(self.get(&format!("/api/v1/batches/{id}")).await?)
    }

    /// List the batches via `GET /api/v1/batches`.
    pub async fn list_batches(&self, limit: u32) -> Result<serde_json::Value, ClientError> {
        json_or_error(self.get(&format!("/api/v1/batches?limit={limit}")).await?)
    }
}
