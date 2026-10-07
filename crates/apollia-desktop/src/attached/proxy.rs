//! The runtime's HTTP API reached through the seam, whichever arm runs it.
//!
//! A command whose whole body is a call to a runtime route serves both modes
//! by calling these methods: embedded, the request goes to the listener of the
//! runtime in this process; attached, to the daemon's. The answer is the
//! route's JSON in both cases, and an error is the text the interface shows.

use serde_json::Value;

use super::{AttachedError, RuntimeLink};
use crate::commands::{
    http_delete_json, http_get_for_file, http_get_json, http_open_stream, http_patch_json,
    http_post_for_file, http_post_json, http_post_json_refusable, http_put_json, HttpFailure,
};

impl RuntimeLink {
    /// Opens the open-ended SSE stream `path` of the runtime this application
    /// drives. The caller reads the body frame by frame.
    pub(crate) async fn open_stream(&self, path: &str) -> Result<reqwest::Response, String> {
        match self {
            Self::Embedded(runtime) => http_open_stream(runtime.api_port, path).await,
            Self::Attached(remote) => Ok(remote.open_stream(path).await?),
        }
    }

    /// `GET path` on the runtime this application drives.
    pub(crate) async fn get_json(&self, path: &str) -> Result<Value, String> {
        match self {
            Self::Embedded(runtime) => http_get_json(runtime.api_port, path).await,
            Self::Attached(remote) => Ok(remote.get(path).await?),
        }
    }

    /// [`Self::get_json`], `Ok(None)` when the route answers 404: a state the
    /// interface shows, not a failure.
    pub(crate) async fn get_json_or_none(&self, path: &str) -> Result<Option<Value>, String> {
        match self {
            Self::Embedded(runtime) => match http_get_json(runtime.api_port, path).await {
                Ok(value) => Ok(Some(value)),
                Err(e) if e.starts_with("API error (404)") => Ok(None),
                Err(e) => Err(e),
            },
            Self::Attached(remote) => match remote.get::<Value>(path).await {
                Ok(value) => Ok(Some(value)),
                Err(AttachedError::Refused { status: 404, .. }) => Ok(None),
                Err(e) => Err(e.into()),
            },
        }
    }

    /// `POST path` with a JSON body on the runtime this application drives.
    pub(crate) async fn post_json(&self, path: &str, body: &Value) -> Result<Value, String> {
        match self {
            Self::Embedded(runtime) => http_post_json(runtime.api_port, path, body).await,
            Self::Attached(remote) => Ok(remote.post(path, body).await?),
        }
    }

    /// [`Self::post_json`] for a caller that tells one refusal from another:
    /// the failure keeps the HTTP status the runtime answered, in both arms.
    pub(crate) async fn post_json_refusable(
        &self,
        path: &str,
        body: &Value,
    ) -> Result<Value, HttpFailure> {
        match self {
            Self::Embedded(runtime) => http_post_json_refusable(runtime.api_port, path, body).await,
            Self::Attached(remote) => remote.post(path, body).await.map_err(|error| {
                let status = match &error {
                    AttachedError::Refused { status, .. } => Some(*status),
                    _ => None,
                };
                HttpFailure {
                    status,
                    message: error.into(),
                }
            }),
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

    /// `POST path` with a JSON body on the runtime this application drives,
    /// for an answer that is a file: the value of its `report_header`, and
    /// its bytes.
    pub(crate) async fn post_for_file(
        &self,
        path: &str,
        body: &Value,
        report_header: &str,
    ) -> Result<(Option<String>, Vec<u8>), String> {
        match self {
            Self::Embedded(runtime) => {
                http_post_for_file(runtime.api_port, path, body, report_header).await
            }
            Self::Attached(remote) => Ok(remote.post_for_file(path, body, report_header).await?),
        }
    }

    /// `GET path` on the runtime this application drives, for an answer that
    /// is a file: its bytes.
    pub(crate) async fn get_for_file(&self, path: &str) -> Result<Vec<u8>, String> {
        match self {
            Self::Embedded(runtime) => http_get_for_file(runtime.api_port, path).await,
            Self::Attached(remote) => Ok(remote.get_for_file(path).await?),
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

/// Encodes one path segment of a daemon route.
pub(crate) fn segment(value: &str) -> std::borrow::Cow<'_, str> {
    urlencoding::encode(value)
}
