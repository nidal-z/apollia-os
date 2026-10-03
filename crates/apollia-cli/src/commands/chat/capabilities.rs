//! `apollia-os chat capabilities <session-id>`: what the model of a session
//! can do.
//!
//! Reads `GET /api/v1/sessions/{id}/model-capabilities`: the model the next
//! message of the session runs on, the reasoning setting it offers, and
//! whether it reads images. `--json` prints the answer of the runtime as is.

use std::path::PathBuf;

use crate::client::{default_socket_path, RuntimeClient};
use crate::exit_codes;

/// Run `chat capabilities`.
pub(super) async fn run(session_id: &str, socket: Option<PathBuf>, json: bool) -> i32 {
    let client = RuntimeClient::new(socket.unwrap_or_else(default_socket_path));
    match client.get_session_model_capabilities(session_id).await {
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

/// The three lines a person reads: the model, its reasoning setting, images.
fn human_lines(body: &serde_json::Value) -> Vec<String> {
    let model = match body["model"].as_str() {
        Some(name) if body["local"].as_bool() == Some(true) => format!("{name} (local)"),
        Some(name) => format!("{name} (server)"),
        None => "none configured".to_owned(),
    };
    let reasoning = match body["reasoning_profile"].as_str() {
        Some("toggle") => "on or off",
        Some("levels") => "low, medium or high",
        Some("none") => "no setting",
        _ => "unknown",
    };
    let formats: Vec<&str> = body["image_formats"]
        .as_array()
        .map(|a| a.iter().filter_map(serde_json::Value::as_str).collect())
        .unwrap_or_default();
    let vision = if body["vision"].as_bool() == Some(true) {
        if formats.is_empty() {
            "yes".to_owned()
        } else {
            format!("yes ({})", formats.join(", "))
        }
    } else {
        "no".to_owned()
    };
    vec![
        format!("Model:      {model}"),
        format!("Reasoning:  {reasoning}"),
        format!("Images:     {vision}"),
    ]
}

#[cfg(test)]
mod tests {
    use super::human_lines;

    #[test]
    fn a_local_switch_model_reading_images_reads_in_three_lines() {
        // GIVEN the answer for a local model with a reasoning switch and vision
        let body = serde_json::json!({
            "model": "local-qwen",
            "local": true,
            "reasoning_profile": "toggle",
            "vision": true,
            "image_formats": ["jpeg", "png"]
        });

        // WHEN it is rendered for a person
        let lines = human_lines(&body);

        // THEN each line names one fact
        assert_eq!(
            lines,
            [
                "Model:      local-qwen (local)",
                "Reasoning:  on or off",
                "Images:     yes (jpeg, png)",
            ]
        );
    }

    #[test]
    fn a_remote_model_reads_as_unknown_reasoning() {
        // GIVEN the answer for a model on a server, whose profile is unknown
        let body = serde_json::json!({
            "model": "srv/model",
            "local": false,
            "reasoning_profile": null,
            "vision": false,
            "image_formats": []
        });

        // WHEN it is rendered for a person
        let lines = human_lines(&body);

        // THEN the reasoning setting is unknown and no image is read
        assert_eq!(lines[0], "Model:      srv/model (server)");
        assert_eq!(lines[1], "Reasoning:  unknown");
        assert_eq!(lines[2], "Images:     no");
    }
}
