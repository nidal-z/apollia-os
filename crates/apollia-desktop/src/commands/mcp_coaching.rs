//! Tauri IPC command — post-install coaching examples for a freshly added
//! MCP connection (US-SP42-052, finding C.I.14).
//!
//! Returns 2–3 ready-to-send prompts the operator can click to try the newly
//! connected service immediately. Implemented as a heuristic for now: the
//! skill contract (same `Vec<CoachingExample>` shape) matches what the
//! Meta-LLM `GenerateCapabilityCoaching` routine (US-SP42-057) will produce,
//! so the frontend will not need to change when the LLM path is wired.
//!
//! Cache is not implemented yet (US-SP42-057 owns the 7-day file cache keyed
//! by `integration_id + version`) — the heuristic is deterministic and cheap.
use serde::{Deserialize, Serialize};

/// One coaching example shown as a clickable card in the wizard's final step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoachingExample {
    /// Short actionable title (e.g. "Summarise my latest Notion page").
    pub title: String,
    /// One-line description shown under the title.
    pub description: String,
    /// Pre-filled chat message sent when the user clicks "Try".
    pub prompt: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoachingRequest {
    pub server_name: String,
    #[serde(default)]
    pub server_title: Option<String>,
}

/// Generate 2–3 post-install examples for a newly added MCP server.
///
/// Never returns `Err` — on empty match the frontend shows a generic empty
/// state. The heuristic matches against well-known connector names; unknown
/// servers get a single generic "Explore the tools" card.
#[tauri::command]
pub fn meta_generate_capabilities_coaching(
    request: CoachingRequest,
) -> Result<Vec<CoachingExample>, String> {
    let name = request.server_name.to_ascii_lowercase();
    let title = request.server_title.unwrap_or_else(|| request.server_name.clone());

    let examples = if name.contains("notion") {
        vec![
            CoachingExample {
                title: format!("Résumer ma dernière page {title}"),
                description: "Ouvre un chat et demande un résumé de votre dernière page Notion."
                    .into(),
                prompt: "Résume ma dernière page Notion en 5 points clés.".into(),
            },
            CoachingExample {
                title: "Lister mes bases de données".into(),
                description: "Explore les bases accessibles via ce connecteur.".into(),
                prompt: "Liste les bases de données Notion auxquelles tu as accès.".into(),
            },
        ]
    } else if name.contains("github") {
        vec![
            CoachingExample {
                title: "Lister mes issues ouvertes".into(),
                description: "Affiche les issues qui vous sont assignées.".into(),
                prompt: "Liste mes 10 dernières issues GitHub ouvertes.".into(),
            },
            CoachingExample {
                title: "Résumer un dépôt".into(),
                description: "Demande un résumé du README d'un dépôt.".into(),
                prompt: "Ouvre mon dépôt le plus récent et résume son README.".into(),
            },
        ]
    } else if name.contains("slack") {
        vec![
            CoachingExample {
                title: "Résumer un canal".into(),
                description: "Obtenez un résumé des derniers messages d'un canal.".into(),
                prompt: "Résume les 30 derniers messages de mon canal Slack principal.".into(),
            },
        ]
    } else if name.contains("filesystem") || name.contains("file") {
        vec![
            CoachingExample {
                title: "Explorer un dossier".into(),
                description: "Liste les fichiers d'un dossier autorisé.".into(),
                prompt: "Liste les fichiers à la racine du workspace.".into(),
            },
        ]
    } else if name.contains("brave") || name.contains("search") {
        vec![
            CoachingExample {
                title: "Lancer une recherche web".into(),
                description: "Cherche une information à jour sur le web.".into(),
                prompt: "Cherche sur le web les dernières nouvelles d'Apollia OS.".into(),
            },
        ]
    } else {
        vec![CoachingExample {
            title: format!("Explorer les outils de {title}"),
            description: "Demande à un assistant ce que ce connecteur peut faire.".into(),
            prompt: format!(
                "Quels outils propose la connexion {title} et que peux-tu faire avec ?"
            ),
        }]
    };

    Ok(examples)
}
