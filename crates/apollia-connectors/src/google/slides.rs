//! Google Slides API client, non-sensitive scope (`presentations`).

use reqwest::Method;
use serde::{Deserialize, Serialize};

use crate::{
    error::ConnectorError,
    http::{HttpClient, JsonRequest},
};

const BASE: &str = "https://slides.googleapis.com/v1/presentations";

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationMeta {
    pub presentation_id: String,
    pub title: String,
    #[serde(default)]
    pub revision_id: Option<String>,
}

#[derive(Clone)]
pub struct SlidesClient {
    http: HttpClient,
}

impl SlidesClient {
    pub fn new(http: HttpClient) -> Self {
        Self { http }
    }

    /// Create a new presentation. v0.1.x ships only this; agents can
    /// chain Drive operations to share/move the deck once created.
    pub async fn create<F, Fut>(
        &self,
        title: &str,
        bearer: &str,
        refresh: F,
    ) -> Result<PresentationMeta, ConnectorError>
    where
        F: FnOnce() -> Fut + Send,
        Fut: std::future::Future<Output = Result<String, ConnectorError>> + Send,
    {
        let body = serde_json::json!({ "title": title });
        self.http
            .json_request(
                JsonRequest {
                    method: Method::POST,
                    url: BASE,
                    body: &body,
                },
                bearer,
                refresh,
            )
            .await
    }

    /// Append a slide containing a single text body to an existing deck.
    /// Uses `batchUpdate` with two requests: createSlide + insertText.
    pub async fn append_slide_with_text<F, Fut>(
        &self,
        presentation_id: &str,
        text: &str,
        bearer: &str,
        refresh: F,
    ) -> Result<serde_json::Value, ConnectorError>
    where
        F: FnOnce() -> Fut + Send,
        Fut: std::future::Future<Output = Result<String, ConnectorError>> + Send,
    {
        let object_id = format!("slide-{}", uuid_short());
        let text_box_id = format!("txt-{}", uuid_short());
        let url = format!("{BASE}/{presentation_id}:batchUpdate");
        let body = serde_json::json!({
            "requests": [
                {
                    "createSlide": {
                        "objectId": object_id,
                        "slideLayoutReference": { "predefinedLayout": "BLANK" }
                    }
                },
                {
                    "createShape": {
                        "objectId": text_box_id,
                        "shapeType": "TEXT_BOX",
                        "elementProperties": {
                            "pageObjectId": object_id,
                            "size": {
                                "width": { "magnitude": 6_000_000, "unit": "EMU" },
                                "height": { "magnitude": 3_000_000, "unit": "EMU" }
                            },
                            "transform": {
                                "scaleX": 1.0,
                                "scaleY": 1.0,
                                "translateX": 1_000_000,
                                "translateY": 1_000_000,
                                "unit": "EMU"
                            }
                        }
                    }
                },
                {
                    "insertText": {
                        "objectId": text_box_id,
                        "text": text
                    }
                }
            ]
        });
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
}

fn uuid_short() -> String {
    let id = uuid::Uuid::new_v4().to_string();
    id.split('-').next().unwrap_or("xxx").to_string()
}
