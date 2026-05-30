//! Outlook Calendar client (Microsoft Graph `/me/events`).
//!
//! Free-tier surface (calendar.readwrite scope): list, get, create, update,
//! delete events. Microsoft has no CASA equivalent, so v0.1.0 covers the
//! full Calendar API surface.

use chrono::{DateTime, Utc};
use reqwest::Method;
use serde::{Deserialize, Serialize};

use crate::{
    error::ConnectorError,
    http::{HttpClient, JsonRequest, RawRequest},
};

const GRAPH: &str = "https://graph.microsoft.com/v1.0/me";

// ─── Domain types ────────────────────────────────────────────────────────────

/// Outlook calendar event.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutlookEvent {
    /// Server-assigned event id.
    pub id: String,
    /// Subject (title).
    #[serde(default)]
    pub subject: Option<String>,
    /// Optional body / description (HTML by default).
    #[serde(default)]
    pub body: Option<EventBody>,
    /// Start time payload.
    pub start: EventDateTime,
    /// End time payload.
    pub end: EventDateTime,
    /// Location wrapper.
    #[serde(default)]
    pub location: Option<EventLocation>,
    /// Attendee list.
    #[serde(default)]
    pub attendees: Vec<EventAttendee>,
    /// Web link to view the event.
    #[serde(default)]
    pub web_link: Option<String>,
    /// Whether this is an all-day event.
    #[serde(default)]
    pub is_all_day: bool,
}

/// Body wrapper for events.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EventBody {
    /// `"text"` or `"html"`.
    #[serde(rename = "contentType")]
    pub content_type: String,
    /// Raw content.
    pub content: String,
}

/// Time + timezone payload as used by Graph.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventDateTime {
    /// ISO 8601 date-time. Note: Graph expects a naive form when paired with `time_zone`.
    pub date_time: String,
    /// IANA timezone (e.g. `Europe/Paris`) or `UTC`.
    pub time_zone: String,
}

impl EventDateTime {
    /// Build an [`EventDateTime`] from an [`Utc`] instant (`time_zone = "UTC"`).
    pub fn from_utc(instant: DateTime<Utc>) -> Self {
        Self {
            date_time: instant.format("%Y-%m-%dT%H:%M:%S").to_string(),
            time_zone: "UTC".into(),
        }
    }
}

/// Optional location payload.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventLocation {
    /// Display name for the location.
    pub display_name: String,
}

/// Attendee envelope.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventAttendee {
    /// Email address payload (same shape as mail recipients).
    pub email_address: super::mail::EmailAddress,
    /// `required`, `optional`, or `resource`.
    #[serde(default, rename = "type")]
    pub attendee_type: Option<String>,
}

/// Body of `create_event` / `update_event`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventDraft {
    /// Subject (title).
    pub subject: String,
    /// Optional body.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<EventBody>,
    /// Start time payload.
    pub start: EventDateTime,
    /// End time payload.
    pub end: EventDateTime,
    /// Optional location.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<EventLocation>,
    /// Optional attendees.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attendees: Vec<EventAttendee>,
    /// All-day toggle.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_all_day: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EventList {
    value: Vec<OutlookEvent>,
}

/// Filter parameters for `list_events`.
#[derive(Debug, Default, Clone)]
pub struct ListEventsFilter {
    /// Lower bound (UTC).
    pub start_after: Option<DateTime<Utc>>,
    /// Upper bound (UTC).
    pub end_before: Option<DateTime<Utc>>,
    /// Maximum number of events.
    pub top: Option<u32>,
}

// ─── Client ──────────────────────────────────────────────────────────────────

/// Outlook Calendar client.
#[derive(Clone)]
pub struct OutlookCalendarClient {
    http: HttpClient,
}

impl OutlookCalendarClient {
    /// Build a new client.
    pub fn new(http: HttpClient) -> Self {
        Self { http }
    }

    /// List events filtered by `filter`. Uses the `/calendarview` endpoint
    /// when bounds are provided (handles recurring expansion server-side),
    /// otherwise falls back to `/events`.
    pub async fn list_events<F, Fut>(
        &self,
        filter: &ListEventsFilter,
        bearer: &str,
        refresh: F,
    ) -> Result<Vec<OutlookEvent>, ConnectorError>
    where
        F: FnOnce() -> Fut + Send,
        Fut: std::future::Future<Output = Result<String, ConnectorError>> + Send,
    {
        let url = build_list_url(filter);
        let resp: EventList = self.http.get_json(&url, bearer, refresh).await?;
        Ok(resp.value)
    }

    /// Get a single event by id.
    pub async fn get_event<F, Fut>(
        &self,
        event_id: &str,
        bearer: &str,
        refresh: F,
    ) -> Result<OutlookEvent, ConnectorError>
    where
        F: FnOnce() -> Fut + Send,
        Fut: std::future::Future<Output = Result<String, ConnectorError>> + Send,
    {
        let url = format!("{GRAPH}/events/{event_id}");
        self.http.get_json(&url, bearer, refresh).await
    }

    /// Create a new event.
    pub async fn create_event<F, Fut>(
        &self,
        draft: &EventDraft,
        bearer: &str,
        refresh: F,
    ) -> Result<OutlookEvent, ConnectorError>
    where
        F: FnOnce() -> Fut + Send,
        Fut: std::future::Future<Output = Result<String, ConnectorError>> + Send,
    {
        let url = format!("{GRAPH}/events");
        self.http
            .json_request(
                JsonRequest {
                    method: Method::POST,
                    url: &url,
                    body: draft,
                },
                bearer,
                refresh,
            )
            .await
    }

    /// Update an existing event (PATCH semantics: only provided fields are replaced).
    pub async fn update_event<F, Fut>(
        &self,
        event_id: &str,
        draft: &EventDraft,
        bearer: &str,
        refresh: F,
    ) -> Result<OutlookEvent, ConnectorError>
    where
        F: FnOnce() -> Fut + Send,
        Fut: std::future::Future<Output = Result<String, ConnectorError>> + Send,
    {
        let url = format!("{GRAPH}/events/{event_id}");
        self.http
            .json_request(
                JsonRequest {
                    method: Method::PATCH,
                    url: &url,
                    body: draft,
                },
                bearer,
                refresh,
            )
            .await
    }

    /// Delete an event by id.
    pub async fn delete_event<F, Fut>(
        &self,
        event_id: &str,
        bearer: &str,
        refresh: F,
    ) -> Result<(), ConnectorError>
    where
        F: FnOnce() -> Fut + Send,
        Fut: std::future::Future<Output = Result<String, ConnectorError>> + Send,
    {
        let url = format!("{GRAPH}/events/{event_id}");
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

// ─── URL helpers ─────────────────────────────────────────────────────────────

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

fn build_list_url(filter: &ListEventsFilter) -> String {
    if let (Some(start), Some(end)) = (filter.start_after, filter.end_before) {
        let mut url = format!(
            "{GRAPH}/calendarView?startDateTime={}&endDateTime={}",
            urlencode(&start.to_rfc3339()),
            urlencode(&end.to_rfc3339()),
        );
        if let Some(top) = filter.top {
            url.push_str(&format!("&$top={top}"));
        }
        url
    } else {
        let mut url = format!("{GRAPH}/events");
        if let Some(top) = filter.top {
            url.push_str(&format!("?$top={top}"));
        }
        url
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn test_event_datetime_from_utc_uses_utc_timezone() {
        let instant = Utc.with_ymd_and_hms(2026, 5, 12, 9, 0, 0).unwrap();
        let dt = EventDateTime::from_utc(instant);
        assert_eq!(dt.time_zone, "UTC");
        assert!(dt.date_time.starts_with("2026-05-12T09:00:00"));
    }

    #[test]
    fn test_build_list_url_without_filter_uses_events_endpoint() {
        let url = build_list_url(&ListEventsFilter::default());
        assert!(url.ends_with("/me/events"));
        assert!(!url.contains("calendarView"));
    }

    #[test]
    fn test_build_list_url_with_bounds_uses_calendarview() {
        let url = build_list_url(&ListEventsFilter {
            start_after: Some(Utc.with_ymd_and_hms(2026, 5, 1, 0, 0, 0).unwrap()),
            end_before: Some(Utc.with_ymd_and_hms(2026, 5, 31, 23, 59, 59).unwrap()),
            top: Some(50),
        });
        assert!(url.contains("calendarView"));
        assert!(url.contains("startDateTime="));
        assert!(url.contains("endDateTime="));
        assert!(url.contains("$top=50"));
    }

    #[test]
    fn test_event_draft_serializes_with_graph_field_names() {
        let draft = EventDraft {
            subject: "Standup".into(),
            body: Some(EventBody {
                content_type: "text".into(),
                content: "daily".into(),
            }),
            start: EventDateTime {
                date_time: "2026-05-12T09:00:00".into(),
                time_zone: "Europe/Paris".into(),
            },
            end: EventDateTime {
                date_time: "2026-05-12T09:15:00".into(),
                time_zone: "Europe/Paris".into(),
            },
            location: Some(EventLocation {
                display_name: "Meet".into(),
            }),
            attendees: vec![],
            is_all_day: Some(false),
        };
        let json = serde_json::to_value(&draft).expect("serialize");
        assert_eq!(json["subject"], "Standup");
        assert_eq!(json["start"]["timeZone"], "Europe/Paris");
        assert_eq!(json["location"]["displayName"], "Meet");
        assert_eq!(json["isAllDay"], false);
        // body.contentType is *not* camelCased because we explicitly rename
        // the field to match Graph's actual API.
        assert_eq!(json["body"]["contentType"], "text");
    }
}
