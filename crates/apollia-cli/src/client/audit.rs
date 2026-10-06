//! The audit journal: export it as a file.

use crate::client::{extract_error, ClientError, RuntimeClient};

impl RuntimeClient {
    /// The filtered journal as a file via `GET /api/v1/audit/export`. `uri`
    /// is that path with its query: the format, the detail and the filters.
    pub async fn export_journal(&self, uri: &str) -> Result<Vec<u8>, ClientError> {
        let (status, body) = self.get_bytes(uri).await?;
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
