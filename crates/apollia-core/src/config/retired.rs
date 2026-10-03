//! Keys of `apollia.toml` that the product no longer reads.
//!
//! A file written for an earlier version may still carry one. Refusing it
//! would stop the daemon over a setting that no longer does anything, and
//! dropping it in silence would leave the operator believing it still works.
//! The list is the withdrawn entries of the key registry
//! ([`keys`](super::keys)); [`validate`](super::validate) sets them aside and
//! calls [`warn_retired_keys`], which emits one `config.key.retired` warning
//! per key found.

use super::keys::{withdrawn_keys, ConfigKey};
use super::scan::{covers, scan, LineKind};

/// Returns the withdrawn keys present in `content`, in registry order.
///
/// Reads table headers (`[llm.vertex]`, `[llm.vertex.extra]`), dotted keys
/// (`vertex.enabled = true` under `[llm]`) and inline tables
/// (`vertex = { ... }` under `[llm]`). The scan is line-based and never fails:
/// a malformed file is reported by the deserializer.
pub(crate) fn retired_keys_in(content: &str) -> Vec<&'static ConfigKey> {
    let paths: Vec<String> = scan(content)
        .into_iter()
        .filter(|line| line.kind != LineKind::Other)
        .map(|line| line.path)
        .collect();
    withdrawn_keys()
        .filter(|retired| paths.iter().any(|p| covers(retired.path, p)))
        .collect()
}

/// Emits a `config.key.retired` warning for each withdrawn key in `content`
/// and returns their paths.
pub(crate) fn warn_retired_keys(content: &str) -> Vec<&'static str> {
    retired_keys_in(content)
        .into_iter()
        .map(|retired| {
            tracing::warn!(
                key = %retired.path,
                detail = %retired.withdrawn.unwrap_or_default(),
                "config.key.retired"
            );
            retired.path
        })
        .collect()
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
