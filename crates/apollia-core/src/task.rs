use serde::{Deserialize, Serialize};

use crate::result::InputResponseData;

/// Tâche soumise par le runtime à l'agent via le bridge AIP.
///
/// Contient tout le contexte nécessaire à l'agent pour traiter la demande :
/// identifiant, entrée multi-modale, historique de conversation, et timeout.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AIPTask {
    /// Identifiant unique de la tâche (UUID v4 généré par le runtime).
    pub task_id: String,
    /// Identifiant de contexte groupant les tâches liées (ex: session utilisateur).
    pub context_id: String,
    /// Entrée multi-modale de la tâche.
    pub input: AIPInput,
    /// Historique des échanges précédents dans ce contexte.
    pub history: Vec<AIPMessage>,
    /// Timeout en secondes (None = utiliser le défaut du runtime).
    pub timeout_seconds: Option<u32>,
    /// `true` si cette exécution est une reprise après un `input_required`.
    ///
    /// L'agent vérifie ce champ pour distinguer le premier appel d'une reprise.
    /// Toujours `false` sur le premier appel (valeur par défaut).
    #[serde(default)]
    pub is_resumed: bool,
    /// Réponse humaine fournie après la suspension — peuplée uniquement si `is_resumed == true`.
    ///
    /// Contient la décision (`approved`), la raison optionnelle, et le contexte
    /// JSON sérialisé par l'agent au moment du `input_required`.
    /// Construite par `TaskRepository::rebuild_for_resume()`.
    #[serde(default)]
    pub input_response: Option<InputResponseData>,
}

/// Entrée multi-modale d'une tâche AIP.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AIPInput {
    /// Parties constitutives de l'entrée (texte, fichiers, données structurées).
    pub parts: Vec<AIPPart>,
}

/// Part multi-modal aligné sur le protocole A2A.
///
/// Utilise un discriminant `type` en JSON pour la désérialisation polymorphique.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AIPPart {
    /// Contenu textuel.
    Text(TextPart),
    /// Fichier binaire (inline ou référencé par URI).
    File(FilePart),
    /// Données structurées JSON arbitraires.
    Data(DataPart),
}

/// Partie textuelle d'une entrée ou sortie.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextPart {
    /// Contenu textuel.
    pub text: String,
}

/// Partie fichier d'une entrée ou sortie.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilePart {
    /// Nom du fichier.
    pub name: String,
    /// Type MIME (ex: "application/pdf", "image/png").
    pub mime_type: String,
    /// Contenu binaire inline (None si référencé par URI).
    pub data: Option<Vec<u8>>,
    /// URI de référence (None si données inline).
    pub uri: Option<String>,
}

/// Partie données JSON structurées.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataPart {
    /// Données JSON arbitraires.
    pub data: serde_json::Value,
}

/// Message dans l'historique de conversation d'un contexte AIP.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIPMessage {
    /// Rôle de l'émetteur : "user" ou "agent".
    pub role: String,
    /// Parties du message.
    pub parts: Vec<AIPPart>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_ac2_aip_task_multi_part_input() {
        // GIVEN
        let task = AIPTask {
            task_id: "550e8400-e29b-41d4-a716-446655440000".into(),
            context_id: "session-abc".into(),
            input: AIPInput {
                parts: vec![
                    AIPPart::Text(TextPart {
                        text: "Génère un devis".into(),
                    }),
                    AIPPart::Data(DataPart {
                        data: json!({ "client": "Dupont" }),
                    }),
                ],
            },
            history: vec![],
            timeout_seconds: None,
            ..AIPTask::default()
        };
        // WHEN
        let cloned = task.clone();
        // THEN
        assert_eq!(cloned.input.parts.len(), 2);
        assert!(!cloned.task_id.is_empty());
    }

    #[test]
    fn test_aip_part_text_round_trip() {
        // GIVEN
        let part = AIPPart::Text(TextPart {
            text: "Bonjour".into(),
        });
        // WHEN
        let json = serde_json::to_string(&part).expect("serialize failed");
        let restored: AIPPart = serde_json::from_str(&json).expect("deserialize failed");
        // THEN
        assert!(json.contains("\"type\":\"text\""));
        match restored {
            AIPPart::Text(t) => assert_eq!(t.text, "Bonjour"),
            _ => panic!("expected TextPart"),
        }
    }

    #[test]
    fn test_aip_part_file_with_uri() {
        // GIVEN
        let part = AIPPart::File(FilePart {
            name: "rapport.pdf".into(),
            mime_type: "application/pdf".into(),
            data: None,
            uri: Some("file:///tmp/rapport.pdf".into()),
        });
        // WHEN
        let json = serde_json::to_string(&part).expect("serialize failed");
        let restored: AIPPart = serde_json::from_str(&json).expect("deserialize failed");
        // THEN
        match restored {
            AIPPart::File(f) => {
                assert_eq!(f.name, "rapport.pdf");
                assert!(f.data.is_none());
                assert!(f.uri.is_some());
            }
            _ => panic!("expected FilePart"),
        }
    }

    #[test]
    fn test_aip_message_round_trip() {
        // GIVEN
        let msg = AIPMessage {
            role: "user".into(),
            parts: vec![AIPPart::Text(TextPart {
                text: "Bonjour agent".into(),
            })],
        };
        // WHEN
        let json = serde_json::to_string(&msg).expect("serialize failed");
        let restored: AIPMessage = serde_json::from_str(&json).expect("deserialize failed");
        // THEN
        assert_eq!(restored.role, "user");
        assert_eq!(restored.parts.len(), 1);
    }
}
