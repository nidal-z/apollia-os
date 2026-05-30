//! Google Sheets API client, non-sensitive scope (`spreadsheets`).
//!
//! Operates on sheets the agent creates OR that the user opens with
//! Apollia via Google Picker (same per-resource gating as `drive.file`).
//! No CASA audit needed.

use reqwest::Method;
use serde::{Deserialize, Serialize};

use crate::{
    error::ConnectorError,
    http::{HttpClient, JsonRequest},
};

const BASE: &str = "https://sheets.googleapis.com/v4/spreadsheets";

/// Minimal `Spreadsheet` shape returned by the Sheets v4 API.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Spreadsheet {
    pub spreadsheet_id: String,
    #[serde(default)]
    pub spreadsheet_url: Option<String>,
    pub properties: SpreadsheetProperties,
    #[serde(default)]
    pub sheets: Vec<SheetView>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpreadsheetProperties {
    pub title: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SheetView {
    pub properties: SheetProperties,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SheetProperties {
    pub sheet_id: u64,
    pub title: String,
    #[serde(default)]
    pub index: u32,
}

/// Range-of-values response.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValueRange {
    #[serde(default)]
    pub range: Option<String>,
    #[serde(default)]
    pub major_dimension: Option<String>,
    #[serde(default)]
    pub values: Vec<Vec<serde_json::Value>>,
}

/// Target range plus the rows to write, for
/// [`SheetsClient::append_values`] and [`SheetsClient::update_values`].
pub struct ValueWrite<'a> {
    /// Spreadsheet id.
    pub spreadsheet_id: &'a str,
    /// A1 range expression (e.g. `"Sheet1!A1:C10"`).
    pub range: &'a str,
    /// Rows of cell values.
    pub values: &'a [Vec<serde_json::Value>],
}

#[derive(Clone)]
pub struct SheetsClient {
    http: HttpClient,
}

impl SheetsClient {
    pub fn new(http: HttpClient) -> Self {
        Self { http }
    }

    /// Create a new spreadsheet with the given title.
    pub async fn create<F, Fut>(
        &self,
        title: &str,
        bearer: &str,
        refresh: F,
    ) -> Result<Spreadsheet, ConnectorError>
    where
        F: FnOnce() -> Fut + Send,
        Fut: std::future::Future<Output = Result<String, ConnectorError>> + Send,
    {
        let body = serde_json::json!({
            "properties": { "title": title },
        });
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

    /// List the tabs (sheets) inside a spreadsheet with their titles and ids.
    ///
    /// The Sheets API distinguishes the **spreadsheet title** (the overall
    /// document name) from the **sheet title** (each tab's name, defaults
    /// to `Sheet1` in English locales, `Feuille 1` in French, etc.). Range
    /// expressions use the *sheet* title, not the spreadsheet title; call
    /// this method before constructing an A1 range when you only know the
    /// spreadsheet id.
    pub async fn list_sheets<F, Fut>(
        &self,
        spreadsheet_id: &str,
        bearer: &str,
        refresh: F,
    ) -> Result<Vec<SheetProperties>, ConnectorError>
    where
        F: FnOnce() -> Fut + Send,
        Fut: std::future::Future<Output = Result<String, ConnectorError>> + Send,
    {
        // The `fields` param must include every key the `Spreadsheet` struct
        // marks as required (spreadsheetId, properties.title); otherwise the
        // deserializer chokes on missing keys.
        let url = format!(
            "{BASE}/{spreadsheet_id}?fields=spreadsheetId,properties.title,sheets.properties"
        );
        let resp: Spreadsheet = self.http.get_json(&url, bearer, refresh).await?;
        Ok(resp.sheets.into_iter().map(|s| s.properties).collect())
    }

    /// Read values from a cell range using A1 notation (e.g. `"Sheet1!A1:C10"`).
    pub async fn read_values<F, Fut>(
        &self,
        spreadsheet_id: &str,
        range: &str,
        bearer: &str,
        refresh: F,
    ) -> Result<ValueRange, ConnectorError>
    where
        F: FnOnce() -> Fut + Send,
        Fut: std::future::Future<Output = Result<String, ConnectorError>> + Send,
    {
        let url = format!("{BASE}/{spreadsheet_id}/values/{}", urlencode(range));
        self.http.get_json(&url, bearer, refresh).await
    }

    /// Append rows at the bottom of a range (Sheets auto-extends).
    pub async fn append_values<F, Fut>(
        &self,
        write: ValueWrite<'_>,
        bearer: &str,
        refresh: F,
    ) -> Result<serde_json::Value, ConnectorError>
    where
        F: FnOnce() -> Fut + Send,
        Fut: std::future::Future<Output = Result<String, ConnectorError>> + Send,
    {
        let spreadsheet_id = write.spreadsheet_id;
        let url = format!(
            "{BASE}/{spreadsheet_id}/values/{}:append?valueInputOption=USER_ENTERED",
            urlencode(write.range)
        );
        let body = serde_json::json!({ "values": write.values });
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

    /// Update values inside an existing range (overwrites existing cells).
    pub async fn update_values<F, Fut>(
        &self,
        write: ValueWrite<'_>,
        bearer: &str,
        refresh: F,
    ) -> Result<serde_json::Value, ConnectorError>
    where
        F: FnOnce() -> Fut + Send,
        Fut: std::future::Future<Output = Result<String, ConnectorError>> + Send,
    {
        let spreadsheet_id = write.spreadsheet_id;
        let url = format!(
            "{BASE}/{spreadsheet_id}/values/{}?valueInputOption=USER_ENTERED",
            urlencode(write.range)
        );
        let body = serde_json::json!({ "values": write.values });
        self.http
            .json_request(
                JsonRequest {
                    method: Method::PUT,
                    url: &url,
                    body: &body,
                },
                bearer,
                refresh,
            )
            .await
    }
}

fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~') {
            out.push(c);
        } else {
            for b in c.to_string().as_bytes() {
                out.push_str(&format!("%{b:02X}"));
            }
        }
    }
    out
}
