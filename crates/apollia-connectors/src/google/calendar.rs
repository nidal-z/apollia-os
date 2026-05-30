//! Google Calendar client: list, get, create, update, delete events.
//!
//! Uses non-restricted scopes `calendar.readonly` (read) and `calendar.events`
//! (write). Both are free-tier eligible (no CASA audit required).
//! Calendar id defaults to `primary`; explicit calendars can be addressed
//! by passing their id.

use chrono::{DateTime, Utc};
use reqwest::Method;
use serde::{Deserialize, Serialize};

use crate::{
    error::ConnectorError,
    http::{HttpClient, JsonRequest, RawRequest},
};

const BASE: &str = "https://www.googleapis.com/calendar/v3/calendars";

// ─── Domain types ────────────────────────────────────────────────────────────

/// Calendar event surface exposed to agents.
///
/// Mirrors the Google Calendar API shape but trims fields agents rarely use.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    /// Server-assigned event id.
    pub id: String,
    /// Summary (title) of the event.
    #[serde(default)]
    pub summary: Option<String>,
    /// Optional event description.
    #[serde(default)]
    pub description: Option<String>,
    /// Optional location string.
    #[serde(default)]
    pub location: Option<String>,
    /// Start time wrapper.
    pub start: EventTime,
    /// End time wrapper.
    pub end: EventTime,
    /// Attendees (optional).
    #[serde(default)]
    pub attendees: Vec<Attendee>,
    /// HTML link to view the event.
    #[serde(default)]
    pub html_link: Option<String>,
}

/// Time wrapper used by the Google Calendar API.
///
/// Either `date` (all-day) or `date_time` (timed) is populated. Apollia tools
/// surface both; agents pass whichever is appropriate when creating events.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventTime {
    /// RFC 3339 date-time (timed events).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date_time: Option<DateTime<Utc>>,
    /// All-day date (YYYY-MM-DD).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    /// IANA time zone id (e.g. `Europe/Paris`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_zone: Option<String>,
}

/// Calendar event attendee.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Attendee {
    /// Email address of the attendee.
    pub email: String,
    /// Optional display name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// Optional response status (`needsAction`, `accepted`, `declined`, …).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_status: Option<String>,
}

/// Filter parameters for [`CalendarClient::list_events`].
#[derive(Debug, Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListEventsFilter {
    /// Lower bound for `start.date_time` (inclusive).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_min: Option<DateTime<Utc>>,
    /// Upper bound for `start.date_time` (exclusive).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_max: Option<DateTime<Utc>>,
    /// Maximum number of events to return (default 100, capped at 2500 by the API).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_results: Option<u32>,
    /// Optional free-text filter (matches summary, description, location, …).
    #[serde(rename = "q", skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
}

/// Identifies the event to update plus the new content, for
/// [`CalendarClient::update_event`].
pub struct EventUpdate<'a> {
    /// Calendar id (`"primary"` for the user's primary calendar).
    pub calendar_id: &'a str,
    /// Server-assigned id of the event to replace.
    pub event_id: &'a str,
    /// New event content (full PUT semantics).
    pub draft: &'a EventDraft,
}

/// Body of [`CalendarClient::create_event`].
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventDraft {
    /// Title.
    pub summary: String,
    /// Optional description (markdown not supported by Google, plain text).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Optional location.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    /// Start.
    pub start: EventTime,
    /// End.
    pub end: EventTime,
    /// Attendees.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attendees: Vec<Attendee>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EventList {
    #[serde(default)]
    items: Vec<Event>,
    #[serde(default)]
    #[allow(dead_code)] // exposed in a future paginated API
    next_page_token: Option<String>,
}

// ─── Client ──────────────────────────────────────────────────────────────────

/// Google Calendar API client.
#[derive(Clone)]
pub struct CalendarClient {
    http: HttpClient,
}

impl CalendarClient {
    /// Build a calendar client.
    pub fn new(http: HttpClient) -> Self {
        Self { http }
    }

    /// List events on a calendar, filtered by `filter`.
    ///
    /// Pass `"primary"` as `calendar_id` for the user's primary calendar.
    pub async fn list_events<F, Fut>(
        &self,
        calendar_id: &str,
        filter: &ListEventsFilter,
        bearer: &str,
        refresh: F,
    ) -> Result<Vec<Event>, ConnectorError>
    where
        F: FnOnce() -> Fut + Send,
        Fut: std::future::Future<Output = Result<String, ConnectorError>> + Send,
    {
        let url = build_list_url(calendar_id, filter);
        let resp: EventList = self.http.get_json(&url, bearer, refresh).await?;
        Ok(resp.items)
    }

    /// Get a single event by id.
    pub async fn get_event<F, Fut>(
        &self,
        calendar_id: &str,
        event_id: &str,
        bearer: &str,
        refresh: F,
    ) -> Result<Event, ConnectorError>
    where
        F: FnOnce() -> Fut + Send,
        Fut: std::future::Future<Output = Result<String, ConnectorError>> + Send,
    {
        let cal = urlencode(calendar_id);
        let url = format!("{BASE}/{cal}/events/{event_id}");
        self.http.get_json(&url, bearer, refresh).await
    }

    /// Create a new event on `calendar_id`.
    pub async fn create_event<F, Fut>(
        &self,
        calendar_id: &str,
        draft: &EventDraft,
        bearer: &str,
        refresh: F,
    ) -> Result<Event, ConnectorError>
    where
        F: FnOnce() -> Fut + Send,
        Fut: std::future::Future<Output = Result<String, ConnectorError>> + Send,
    {
        let cal = urlencode(calendar_id);
        let url = format!("{BASE}/{cal}/events");
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

    /// Update an existing event (full PUT semantics).
    pub async fn update_event<F, Fut>(
        &self,
        update: EventUpdate<'_>,
        bearer: &str,
        refresh: F,
    ) -> Result<Event, ConnectorError>
    where
        F: FnOnce() -> Fut + Send,
        Fut: std::future::Future<Output = Result<String, ConnectorError>> + Send,
    {
        let cal = urlencode(update.calendar_id);
        let event_id = update.event_id;
        let url = format!("{BASE}/{cal}/events/{event_id}");
        self.http
            .json_request(
                JsonRequest {
                    method: Method::PUT,
                    url: &url,
                    body: update.draft,
                },
                bearer,
                refresh,
            )
            .await
    }

    /// Delete an event by id. Returns success even if the event was already gone.
    pub async fn delete_event<F, Fut>(
        &self,
        calendar_id: &str,
        event_id: &str,
        bearer: &str,
        refresh: F,
    ) -> Result<(), ConnectorError>
    where
        F: FnOnce() -> Fut + Send,
        Fut: std::future::Future<Output = Result<String, ConnectorError>> + Send,
    {
        let cal = urlencode(calendar_id);
        let url = format!("{BASE}/{cal}/events/{event_id}");
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
    // The encoding rules for Google's calendar id are mild: `/` and `:` need
    // escaping for ids that look like emails (e.g. `user@example.com`).
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~' | '@') {
            out.push(c);
        } else {
            for b in c.to_string().as_bytes() {
                out.push_str(&format!("%{b:02X}"));
            }
        }
    }
    out
}

fn build_list_url(calendar_id: &str, filter: &ListEventsFilter) -> String {
    let cal = urlencode(calendar_id);
    let mut url = format!("{BASE}/{cal}/events?singleEvents=true&orderBy=startTime");
    if let Some(tmin) = filter.time_min {
        url.push_str(&format!("&timeMin={}", urlencode(&tmin.to_rfc3339())));
    }
    if let Some(tmax) = filter.time_max {
        url.push_str(&format!("&timeMax={}", urlencode(&tmax.to_rfc3339())));
    }
    if let Some(max) = filter.max_results {
        url.push_str(&format!("&maxResults={max}"));
    }
    if let Some(q) = &filter.query {
        url.push_str(&format!("&q={}", urlencode(q)));
    }
    url
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn test_urlencode_keeps_safe_chars() {
        assert_eq!(urlencode("primary"), "primary");
        assert_eq!(urlencode("foo@example.com"), "foo@example.com");
    }

    #[test]
    fn test_urlencode_escapes_unsafe_chars() {
        assert_eq!(urlencode("a b"), "a%20b");
        assert_eq!(urlencode("?key=val&x=y"), "%3Fkey%3Dval%26x%3Dy");
    }

    #[test]
    fn test_build_list_url_default_includes_single_events_and_order_by() {
        let url = build_list_url("primary", &ListEventsFilter::default());
        assert!(url.contains("singleEvents=true"));
        assert!(url.contains("orderBy=startTime"));
        assert!(!url.contains("timeMin"));
        assert!(!url.contains("timeMax"));
    }

    #[test]
    fn test_build_list_url_includes_time_bounds_when_set() {
        let tmin = Utc.with_ymd_and_hms(2026, 5, 1, 0, 0, 0).unwrap();
        let tmax = Utc.with_ymd_and_hms(2026, 5, 31, 23, 59, 59).unwrap();
        let url = build_list_url(
            "primary",
            &ListEventsFilter {
                time_min: Some(tmin),
                time_max: Some(tmax),
                max_results: Some(50),
                query: Some("standup".into()),
            },
        );
        assert!(url.contains("timeMin="));
        assert!(url.contains("timeMax="));
        assert!(url.contains("maxResults=50"));
        assert!(url.contains("q=standup"));
    }

    #[test]
    fn test_event_time_serializes_with_camel_case() {
        let t = EventTime {
            date_time: Some(Utc.with_ymd_and_hms(2026, 5, 12, 9, 0, 0).unwrap()),
            date: None,
            time_zone: Some("Europe/Paris".into()),
        };
        let json = serde_json::to_value(&t).expect("serialize");
        assert!(json.get("dateTime").is_some());
        assert!(json.get("timeZone").is_some());
        // Field with None is skipped
        assert!(json.get("date").is_none());
    }

    #[test]
    fn test_event_time_skips_none_fields() {
        let t = EventTime::default();
        let json = serde_json::to_string(&t).expect("serialize");
        assert_eq!(json, "{}");
    }

    #[test]
    fn test_event_draft_serializes_with_camel_case() {
        let draft = EventDraft {
            summary: "Standup".into(),
            description: Some("daily".into()),
            location: None,
            start: EventTime {
                date_time: Some(Utc.with_ymd_and_hms(2026, 5, 12, 9, 0, 0).unwrap()),
                date: None,
                time_zone: Some("Europe/Paris".into()),
            },
            end: EventTime {
                date_time: Some(Utc.with_ymd_and_hms(2026, 5, 12, 9, 15, 0).unwrap()),
                date: None,
                time_zone: Some("Europe/Paris".into()),
            },
            attendees: vec![Attendee {
                email: "a@example.com".into(),
                display_name: Some("Alice".into()),
                response_status: None,
            }],
        };
        let json = serde_json::to_value(&draft).expect("serialize");
        assert_eq!(json["summary"], "Standup");
        assert!(json["start"]["dateTime"].is_string());
        assert_eq!(json["attendees"][0]["displayName"], "Alice");
    }
}
