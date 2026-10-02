//! The request an agent reads from a task input.

use apollia_core::{DataPart, FilePart, TextPart};
use serde_json::json;

use super::*;

// GIVEN an input of two text parts, a data part and a file part
// WHEN the request is read from it
// THEN the texts come first in order, the data follows as JSON, the file is left out
#[test]
fn test_message_of_joins_text_then_data() {
    // GIVEN
    let input = AIPInput {
        parts: vec![
            AIPPart::Text(TextPart {
                text: "Reconcile March.".into(),
            }),
            AIPPart::File(FilePart {
                name: "march.csv".into(),
                mime_type: "text/csv".into(),
                data: None,
                uri: None,
            }),
            AIPPart::Data(DataPart {
                data: json!({"account": "512"}),
            }),
            AIPPart::Text(TextPart {
                text: "Report the gaps.".into(),
            }),
        ],
    };

    // WHEN
    let message = message_of(&input);

    // THEN
    assert_eq!(
        message,
        "Reconcile March.\n\n{\"account\":\"512\"}\n\nReport the gaps."
    );
}
