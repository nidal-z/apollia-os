//! Keys of `apollia.toml` that the product no longer reads.
//!
//! A file written for an earlier version may still carry one. Refusing it
//! would stop the daemon over a setting that no longer does anything, and
//! dropping it in silence would leave the operator believing it still works.
//! Each loader therefore accepts it and calls [`warn_retired_keys`], which
//! emits one `config.key.retired` warning per key found.

/// A key path that a loader accepts and ignores, with what replaces it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetiredKey {
    /// Dotted path of the key or table, from the root of the file
    /// (`"llm.vertex"` stands for the `[llm.vertex]` table and every key under
    /// it).
    pub path: &'static str,
    /// What the operator does instead, shown in the warning.
    pub detail: &'static str,
}

/// Every retired key, in the order a warning is emitted.
pub const RETIRED_KEYS: &[RetiredKey] = &[RetiredKey {
    path: "llm.vertex",
    detail: "the Google Vertex AI client was removed; declare an OpenAI-compatible server instead",
}];

/// Returns the retired keys present in `content`, in [`RETIRED_KEYS`] order.
///
/// Reads table headers (`[llm.vertex]`, `[llm.vertex.extra]`), dotted keys
/// (`vertex.enabled = true` under `[llm]`) and inline tables
/// (`vertex = { ... }` under `[llm]`). The scan is line-based and never fails:
/// a malformed file is reported by the loader that deserializes it.
pub fn retired_keys_in(content: &str) -> Vec<&'static RetiredKey> {
    let paths = key_paths(content);
    RETIRED_KEYS
        .iter()
        .filter(|retired| paths.iter().any(|p| covers(retired.path, p)))
        .collect()
}

/// Emits a `config.key.retired` warning for each retired key in `content`
/// and returns their paths.
pub fn warn_retired_keys(content: &str) -> Vec<&'static str> {
    retired_keys_in(content)
        .into_iter()
        .map(|retired| {
            tracing::warn!(
                key = %retired.path,
                detail = %retired.detail,
                "config.key.retired"
            );
            retired.path
        })
        .collect()
}

/// `true` when `path` is `retired` itself or lies under it.
fn covers(retired: &str, path: &str) -> bool {
    path == retired
        || path
            .strip_prefix(retired)
            .is_some_and(|rest| rest.starts_with('.'))
}

/// Every table header and every key of `content`, as dotted paths from the root.
fn key_paths(content: &str) -> Vec<String> {
    let mut paths = Vec::new();
    let mut table = String::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(header) = line.strip_prefix('[') {
            let header = header.trim_start_matches('[');
            let Some(end) = header.find(']') else {
                continue;
            };
            table = normalize(&header[..end]);
            paths.push(table.clone());
            continue;
        }
        let Some(eq) = line.find('=') else {
            continue;
        };
        let key = normalize(&line[..eq]);
        if key.is_empty() {
            continue;
        }
        if table.is_empty() {
            paths.push(key);
        } else {
            paths.push(format!("{table}.{key}"));
        }
    }
    paths
}

/// Strips spaces and quotes around each segment of a dotted key.
fn normalize(raw: &str) -> String {
    raw.split('.')
        .map(|segment| segment.trim().trim_matches('"').trim_matches('\''))
        .collect::<Vec<_>>()
        .join(".")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tracing::field::{Field, Visit};
    use tracing::span::{Attributes, Id, Record};
    use tracing::{Event, Level, Metadata, Subscriber};

    /// One event as the recorder saw it: its level and its fields, rendered.
    type Seen = (Level, Vec<(String, String)>);

    /// The smallest subscriber that keeps every event it receives.
    struct Recorder(Arc<Mutex<Vec<Seen>>>);

    struct Fields(Vec<(String, String)>);

    impl Visit for Fields {
        fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
            self.0.push((field.name().to_owned(), format!("{value:?}")));
        }
    }

    impl Subscriber for Recorder {
        fn enabled(&self, _: &Metadata<'_>) -> bool {
            true
        }
        fn new_span(&self, _: &Attributes<'_>) -> Id {
            Id::from_u64(1)
        }
        fn record(&self, _: &Id, _: &Record<'_>) {}
        fn record_follows_from(&self, _: &Id, _: &Id) {}
        fn event(&self, event: &Event<'_>) {
            let mut fields = Fields(Vec::new());
            event.record(&mut fields);
            self.0
                .lock()
                .unwrap()
                .push((*event.metadata().level(), fields.0));
        }
        fn enter(&self, _: &Id) {}
        fn exit(&self, _: &Id) {}
    }

    fn field<'a>(fields: &'a [(String, String)], name: &str) -> Option<&'a str> {
        fields
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }

    const WITH_VERTEX: &str = r#"
[llm]
default = "local"

[[llm.backends]]
name = "local"
type = "api"
provider = "openai"
model = "qwen"
api_url = "http://localhost:8080/v1"
api_key_env = "LOCAL_SERVER_KEY"

[llm.vertex]
enabled    = true
project_id = "my-gcp-project"
location   = "us-east5"
model_id   = "claude-sonnet-4-6@20251001"
"#;

    #[test]
    fn a_file_still_carrying_llm_vertex_loads_and_warns() {
        // GIVEN an apollia.toml written for an earlier version, with [llm.vertex]
        let events = Arc::new(Mutex::new(Vec::new()));
        let recorder = Recorder(Arc::clone(&events));

        // WHEN the file is parsed and its retired keys are checked
        let parsed = toml::from_str::<toml::Table>(WITH_VERTEX);
        let found = tracing::subscriber::with_default(recorder, || warn_retired_keys(WITH_VERTEX));

        // THEN the file loads, the key is reported once, as a warning naming it
        assert!(parsed.is_ok(), "the file must still parse: {parsed:?}");
        assert_eq!(found, vec!["llm.vertex"]);
        let events = events.lock().unwrap();
        assert_eq!(events.len(), 1, "one warning expected, got {events:?}");
        let (level, fields) = &events[0];
        assert_eq!(*level, Level::WARN);
        assert_eq!(field(fields, "message"), Some("config.key.retired"));
        assert_eq!(field(fields, "key"), Some("llm.vertex"));
    }

    #[test]
    fn a_file_without_retired_keys_emits_nothing() {
        // GIVEN a file with an [llm] section and no retired key, and a
        //       neighbour whose name only starts like one
        let content = "[llm]\ndefault = \"local\"\nvertexish = 1\n";
        let events = Arc::new(Mutex::new(Vec::new()));
        let recorder = Recorder(Arc::clone(&events));

        // WHEN its retired keys are checked
        let found = tracing::subscriber::with_default(recorder, || warn_retired_keys(content));

        // THEN nothing is reported
        assert!(found.is_empty());
        assert!(events.lock().unwrap().is_empty());
    }

    #[test]
    fn dotted_and_inline_forms_are_recognised() {
        // GIVEN the ways TOML can spell the same retired table
        let header = "[llm.vertex.extra]\nx = 1\n";
        let dotted = "[llm]\nvertex.enabled = true\n";
        let inline = "[llm]\nvertex = { enabled = true }\n";
        let quoted = "[\"llm\" . 'vertex']\nenabled = true\n";

        // WHEN each is scanned
        let results: Vec<_> = [header, dotted, inline, quoted]
            .iter()
            .map(|c| {
                retired_keys_in(c)
                    .iter()
                    .map(|k| k.path)
                    .collect::<Vec<_>>()
            })
            .collect();

        // THEN every form reports llm.vertex
        for found in results {
            assert_eq!(found, vec!["llm.vertex"]);
        }
    }
}
