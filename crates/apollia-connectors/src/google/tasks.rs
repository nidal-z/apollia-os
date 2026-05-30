//! Google Tasks API client, non-sensitive scope (`tasks`).
//!
//! Note: unlike `drive.file`, the `tasks` scope has no per-resource
//! gating; it grants full access to all of the user's task lists.
//! Still non-sensitive per Google's policy.

use reqwest::Method;
use serde::{Deserialize, Serialize};

use crate::{
    error::ConnectorError,
    http::{HttpClient, JsonRequest, RawRequest},
};

const BASE: &str = "https://tasks.googleapis.com/tasks/v1";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TaskList {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub updated: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub notes: Option<String>,
    /// `needsAction` | `completed`.
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub due: Option<String>,
    #[serde(default)]
    pub completed: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct TaskListsResponse {
    #[serde(default)]
    items: Vec<TaskList>,
}

#[derive(Debug, Clone, Deserialize)]
struct TasksResponse {
    #[serde(default)]
    items: Vec<Task>,
}

/// Parameters for [`TasksClient::create_task`].
pub struct NewTask<'a> {
    /// Id of the list the task is created in (`@default` for the default list).
    pub task_list_id: &'a str,
    /// Task title.
    pub title: &'a str,
    /// Optional free-text notes.
    pub notes: Option<&'a str>,
    /// Optional due date (RFC 3339).
    pub due_rfc3339: Option<&'a str>,
}

#[derive(Clone)]
pub struct TasksClient {
    http: HttpClient,
}

impl TasksClient {
    pub fn new(http: HttpClient) -> Self {
        Self { http }
    }

    /// List the user's task lists.
    pub async fn list_lists<F, Fut>(
        &self,
        bearer: &str,
        refresh: F,
    ) -> Result<Vec<TaskList>, ConnectorError>
    where
        F: FnOnce() -> Fut + Send,
        Fut: std::future::Future<Output = Result<String, ConnectorError>> + Send,
    {
        let url = format!("{BASE}/users/@me/lists");
        let resp: TaskListsResponse = self.http.get_json(&url, bearer, refresh).await?;
        Ok(resp.items)
    }

    /// List tasks inside a given list (default list id = `@default`).
    pub async fn list_tasks<F, Fut>(
        &self,
        task_list_id: &str,
        bearer: &str,
        refresh: F,
    ) -> Result<Vec<Task>, ConnectorError>
    where
        F: FnOnce() -> Fut + Send,
        Fut: std::future::Future<Output = Result<String, ConnectorError>> + Send,
    {
        let url = format!("{BASE}/lists/{task_list_id}/tasks?showCompleted=true");
        let resp: TasksResponse = self.http.get_json(&url, bearer, refresh).await?;
        Ok(resp.items)
    }

    /// Create a new task inside a list.
    pub async fn create_task<F, Fut>(
        &self,
        task: NewTask<'_>,
        bearer: &str,
        refresh: F,
    ) -> Result<Task, ConnectorError>
    where
        F: FnOnce() -> Fut + Send,
        Fut: std::future::Future<Output = Result<String, ConnectorError>> + Send,
    {
        let mut body = serde_json::json!({ "title": task.title });
        if let Some(n) = task.notes {
            body["notes"] = serde_json::Value::String(n.to_string());
        }
        if let Some(d) = task.due_rfc3339 {
            body["due"] = serde_json::Value::String(d.to_string());
        }
        let task_list_id = task.task_list_id;
        let url = format!("{BASE}/lists/{task_list_id}/tasks");
        self.http
            .json_request(
                JsonRequest {
                    method: Method::POST,
                    url: &url,
                    body: &body,
                },
                bearer,
                refresh,
            )
            .await
    }

    /// Mark a task complete.
    pub async fn complete_task<F, Fut>(
        &self,
        task_list_id: &str,
        task_id: &str,
        bearer: &str,
        refresh: F,
    ) -> Result<Task, ConnectorError>
    where
        F: FnOnce() -> Fut + Send,
        Fut: std::future::Future<Output = Result<String, ConnectorError>> + Send,
    {
        let body = serde_json::json!({ "status": "completed" });
        let url = format!("{BASE}/lists/{task_list_id}/tasks/{task_id}");
        self.http
            .json_request(
                JsonRequest {
                    method: Method::PATCH,
                    url: &url,
                    body: &body,
                },
                bearer,
                refresh,
            )
            .await
    }

    /// Delete a task permanently.
    pub async fn delete_task<F, Fut>(
        &self,
        task_list_id: &str,
        task_id: &str,
        bearer: &str,
        refresh: F,
    ) -> Result<(), ConnectorError>
    where
        F: FnOnce() -> Fut + Send,
        Fut: std::future::Future<Output = Result<String, ConnectorError>> + Send,
    {
        let url = format!("{BASE}/lists/{task_list_id}/tasks/{task_id}");
        self.http
            .send(
                RawRequest {
                    method: Method::DELETE,
                    url: &url,
                    body: None,
                    content_type: None,
                },
                bearer,
                refresh,
            )
            .await?;
        Ok(())
    }
}
