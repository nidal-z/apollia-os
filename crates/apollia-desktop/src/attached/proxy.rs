//! The runtime's HTTP API reached through the seam, whichever arm runs it.
//!
//! A command whose whole body is a call to a runtime route serves both modes
//! by calling these methods: embedded, the request goes to the listener of the
//! runtime in this process; attached, to the daemon's. The answer is the
//! route's JSON in both cases, and an error is the text the interface shows.

use serde_json::Value;

use super::RuntimeLink;
use crate::commands::{
    http_delete_json, http_get_json, http_patch_json, http_post_json, http_put_json,
};

impl RuntimeLink {
    /// `GET path` on the runtime this application drives.
    pub(crate) async fn get_json(&self, path: &str) -> Result<Value, String> {
        match self {
            Self::Embedded(runtime) => http_get_json(runtime.api_port, path).await,
            Self::Attached(remote) => Ok(remote.get(path).await?),
        }
    }

    /// `POST path` with a JSON body on the runtime this application drives.
    pub(crate) async fn post_json(&self, path: &str, body: &Value) -> Result<Value, String> {
        match self {
            Self::Embedded(runtime) => http_post_json(runtime.api_port, path, body).await,
            Self::Attached(remote) => Ok(remote.post(path, body).await?),
        }
    }

    /// [`Self::post_json`] for a request whose work takes longer than the
    /// usual deadline of the attached arm; `deadline` replaces it. The
    /// embedded arm sets none.
    pub(crate) async fn post_json_within(
        &self,
        path: &str,
        body: &Value,
        deadline: std::time::Duration,
    ) -> Result<Value, String> {
        match self {
            Self::Embedded(runtime) => http_post_json(runtime.api_port, path, body).await,
            Self::Attached(remote) => Ok(remote.post_within(path, body, deadline).await?),
        }
    }

    /// `PUT path` with a JSON body on the runtime this application drives.
    pub(crate) async fn put_json(&self, path: &str, body: &Value) -> Result<Value, String> {
        match self {
            Self::Embedded(runtime) => http_put_json(runtime.api_port, path, body).await,
            Self::Attached(remote) => Ok(remote.put(path, body).await?),
        }
    }

    /// `PATCH path` with a JSON body on the runtime this application drives.
    pub(crate) async fn patch_json(&self, path: &str, body: &Value) -> Result<Value, String> {
        match self {
            Self::Embedded(runtime) => http_patch_json(runtime.api_port, path, body).await,
            Self::Attached(remote) => Ok(remote.patch(path, body).await?),
        }
    }

    /// `DELETE path` on the runtime this application drives.
    pub(crate) async fn delete_json(&self, path: &str) -> Result<Value, String> {
        match self {
            Self::Embedded(runtime) => http_delete_json(runtime.api_port, path).await,
            Self::Attached(remote) => Ok(remote.delete(path).await?),
        }
    }
}
