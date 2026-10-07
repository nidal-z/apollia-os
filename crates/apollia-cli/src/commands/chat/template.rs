//! `apollia-os chat template <session-id>`: the prompt template the model of
//! a session receives.
//!
//! Reads `GET /api/v1/sessions/{id}/prompt-template`: the template of the
//! preset the session chose when it carries one, else the one the local
//! engine reports for the loaded model. `--json` prints the answer of the
//! runtime as is.

use std::path::PathBuf;

use crate::client::{default_socket_path, RuntimeClient};
use crate::exit_codes;

/// Run `chat template`.
pub(super) async fn run(session_id: &str, socket: Option<PathBuf>, json: bool) -> i32 {
    let client = RuntimeClient::new(socket.unwrap_or_else(default_socket_path));
    match client.get_session_prompt_template(session_id).await {
        Ok(body) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&body).unwrap_or_default()
                );
            } else {
                for line in human_lines(&body) {
                    println!("{line}");
                }
            }
            exit_codes::SUCCESS
        }
        Err(e) => crate::output::emit_client_error(json, &e),
    }
}

/// Where the template comes from, then its text when there is one.
fn human_lines(body: &serde_json::Value) -> Vec<String> {
    let source = match body["source"].as_str() {
        Some("preset") => "the preset of the session",
        Some("engine") => "the engine serving the model",
        _ => "built into the model, not readable here",
    };
    let mut lines = vec![format!("Source:  {source}")];
    if let Some(template) = body["template"].as_str() {
        lines.push(String::new());
        lines.push(template.to_owned());
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::human_lines;

    #[test]
    fn a_template_read_from_the_engine_follows_its_source() {
        // GIVEN the answer for a loaded model whose engine reports a template
        let body = serde_json::json!({
            "source": "engine",
            "template": "{%- for m in messages %}{{ m.content }}{%- endfor %}"
        });

        // WHEN it is rendered for a person
        let lines = human_lines(&body);

        // THEN the source comes first, then the text as it is
        assert_eq!(
            lines,
            [
                "Source:  the engine serving the model",
                "",
                "{%- for m in messages %}{{ m.content }}{%- endfor %}",
            ]
        );
    }

    #[test]
    fn a_model_that_is_not_loaded_reads_as_not_readable() {
        // GIVEN the answer for a model that is not loaded
        let body = serde_json::json!({ "source": "unavailable", "template": null });

        // WHEN it is rendered for a person
        let lines = human_lines(&body);

        // THEN one line says so and no text follows
        assert_eq!(lines, ["Source:  built into the model, not readable here"]);
    }
}
