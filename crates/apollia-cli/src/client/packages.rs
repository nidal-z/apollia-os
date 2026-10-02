//! `.apo` packages: preview, installation, trusted publisher keys.

use crate::client::{encode_path_segment, ClientError, RawResponse, RuntimeClient};

impl RuntimeClient {
    /// What the package at `path` contains and asks for, via
    /// `POST /api/v1/packages/preview`. Nothing is written.
    pub async fn preview_package(&self, path: &str) -> Result<serde_json::Value, ClientError> {
        let body = serde_json::json!({ "path": path });
        let resp = self.post("/api/v1/packages/preview", Some(&body)).await?;
        package_body(resp)
    }

    /// Installs or updates the package at `path` via
    /// `POST /api/v1/packages/install`.
    pub async fn install_package(
        &self,
        path: &str,
        deps_confirmed: bool,
        changes_confirmed: bool,
        allow_downgrade: bool,
    ) -> Result<serde_json::Value, ClientError> {
        let body = serde_json::json!({
            "path": path,
            "depsConfirmed": deps_confirmed,
            "changesConfirmed": changes_confirmed,
            "allowDowngrade": allow_downgrade,
        });
        let resp = self.post("/api/v1/packages/install", Some(&body)).await?;
        package_body(resp)
    }

    /// The trusted publisher keys via `GET /api/v1/packages/trusted-keys`.
    pub async fn list_trusted_keys(&self) -> Result<serde_json::Value, ClientError> {
        let resp = self.get("/api/v1/packages/trusted-keys").await?;
        package_body(resp)
    }

    /// Trusts `public_key` under `label` via
    /// `POST /api/v1/packages/trusted-keys`.
    pub async fn trust_key(
        &self,
        public_key: &str,
        label: &str,
    ) -> Result<serde_json::Value, ClientError> {
        let body = serde_json::json!({ "publicKey": public_key, "label": label });
        let resp = self
            .post("/api/v1/packages/trusted-keys", Some(&body))
            .await?;
        package_body(resp)
    }

    /// Stops trusting the key of `fingerprint` via
    /// `DELETE /api/v1/packages/trusted-keys/{fingerprint}`.
    pub async fn remove_trusted_key(
        &self,
        fingerprint: &str,
    ) -> Result<serde_json::Value, ClientError> {
        let resp = self
            .delete(&format!(
                "/api/v1/packages/trusted-keys/{}",
                encode_path_segment(fingerprint)
            ))
            .await?;
        package_body(resp)
    }
}

/// The JSON body of a success, or the runtime's message, completed with the
/// flag that answers it when the refusal is a missing confirmation.
fn package_body(resp: RawResponse) -> Result<serde_json::Value, ClientError> {
    if resp.status < 400 {
        return Ok(serde_json::from_str(&resp.body)?);
    }
    let parsed: Option<serde_json::Value> = serde_json::from_str(&resp.body).ok();
    let field = |key: &str| {
        parsed
            .as_ref()
            .and_then(|v| v.get(key))
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
    };
    let message = field("error").unwrap_or_else(|| resp.body.clone());
    let hint = match field("code").as_deref() {
        Some("deps_confirmation_required") => " (pass --confirm-deps)",
        Some("changes_confirmation_required") => {
            " (read `package preview`, then pass --confirm-changes)"
        }
        Some("downgrade") => " (pass --allow-downgrade)",
        _ => "",
    };
    Err(ClientError::ServerError {
        status: resp.status,
        body: format!("{message}{hint}"),
    })
}
