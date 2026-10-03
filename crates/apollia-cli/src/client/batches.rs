//! Batches: launch an agent on every item of a source, read one, list them,
//! list the accesses a launch may grant, pause, resume, stop or retry one,
//! and export its result as a file.

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

    /// The sensitive accesses of an agent via `GET /api/v1/batches/accesses`.
    pub async fn batch_accesses(&self, agent: &str) -> Result<serde_json::Value, ClientError> {
        let agent = crate::commands::model::urlencode(agent);
        json_or_error(
            self.get(&format!("/api/v1/batches/accesses?agent={agent}"))
                .await?,
        )
    }

    /// Pause, resume, stop or retry a batch via
    /// `POST /api/v1/batches/{id}/<verb>`.
    pub async fn control_batch(
        &self,
        id: &str,
        verb: &str,
        body: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, ClientError> {
        json_or_error(
            self.post(&format!("/api/v1/batches/{id}/{verb}"), body)
                .await?,
        )
    }

    /// The result of a batch as a file via
    /// `GET /api/v1/batches/{id}/export?format=`: `xlsx`, `csv` or `json`.
    pub async fn export_batch(&self, id: &str, format: &str) -> Result<Vec<u8>, ClientError> {
        let id = crate::commands::model::urlencode(id);
        let format = crate::commands::model::urlencode(format);
        let (status, body) = self
            .get_bytes(&format!("/api/v1/batches/{id}/export?format={format}"))
            .await?;
        if status >= 400 {
            let text = String::from_utf8_lossy(&body);
            return Err(ClientError::ServerError {
                status,
                body: extract_error(&text, status),
            });
        }
        Ok(body.to_vec())
    }
}
