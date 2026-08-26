//! Classification des lignes SSE et lecture de la decision d'approbation.

// ─── Classification des evenements ──────────────────────────────────────────

/// Action derivee d'une ligne SSE du stream de session chat.
#[derive(Debug, PartialEq)]
pub(super) enum ChatStreamAction {
    /// Un fragment de texte assistant a afficher en direct.
    Token(String),
    /// Un appel d'outil demarre (nom, apercu d'entree, intention).
    ToolStarted {
        tool_name: String,
        input_preview: String,
        rationale: Option<String>,
    },
    /// Un appel d'outil termine (succes, apercu de sortie, analyse d'erreur).
    ToolCompleted {
        tool_name: String,
        success: bool,
        output_preview: String,
        analysis: Option<String>,
    },
    /// L'agent demande l'autorisation d'utiliser un outil.
    ApprovalRequired {
        message_id: String,
        tool_name: String,
        prompt: String,
    },
    /// Reponse terminee pour ce tour (contenu complet, fallback si zero token).
    Completed { content: String },
    /// Erreur de generation pour ce tour.
    Error(String),
    /// Session fermee cote serveur, terminal global.
    SessionClosed,
    /// Rien a afficher (echo du prompt, autre message, ligne non pertinente).
    Ignore,
}

/// Classe une ligne SSE brute en [`ChatStreamAction`], filtree sur le tour
/// courant `my_id`. Fonction pure et testable, sans I/O ni socket.
pub(super) fn classify_chat_event(line: &str, my_id: &str) -> ChatStreamAction {
    let Some(data) = line.strip_prefix("data: ") else {
        return ChatStreamAction::Ignore;
    };
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(data) else {
        return ChatStreamAction::Ignore;
    };
    let event = parsed.get("event").and_then(|v| v.as_str()).unwrap_or("");

    // `session_closed` ne porte pas de message_id: fin de session, terminal.
    if event == "session_closed" {
        return ChatStreamAction::SessionClosed;
    }

    // Tous les autres evenements sont filtres sur le message_id du tour courant.
    // Le 202 renvoie le meme id que celui porte par les tokens et la reponse.
    let mid = parsed
        .get("message_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if mid != my_id {
        return ChatStreamAction::Ignore;
    }

    match event {
        "token" => ChatStreamAction::Token(str_field(&parsed, "token")),
        "tool_call_started" => ChatStreamAction::ToolStarted {
            tool_name: str_field(&parsed, "tool_name"),
            input_preview: str_field(&parsed, "input_preview"),
            rationale: extract_rationale(&parsed),
        },
        "tool_call_completed" => ChatStreamAction::ToolCompleted {
            tool_name: str_field(&parsed, "tool_name"),
            success: parsed
                .get("success")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            output_preview: str_field(&parsed, "output_preview"),
            analysis: extract_analysis(&parsed),
        },
        "approval_required" => ChatStreamAction::ApprovalRequired {
            message_id: mid.to_string(),
            tool_name: str_field(&parsed, "tool_name"),
            prompt: str_field(&parsed, "prompt"),
        },
        "response_completed" => ChatStreamAction::Completed {
            content: str_field(&parsed, "content"),
        },
        "error" => ChatStreamAction::Error(str_field(&parsed, "error")),
        // message_sent (echo du prompt), response_started, approval_resolved:
        // rien a afficher.
        _ => ChatStreamAction::Ignore,
    }
}

/// Lit un champ chaine, chaine vide si absent.
pub(super) fn str_field(v: &serde_json::Value, key: &str) -> String {
    v.get(key)
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string()
}

/// Extrait le resume d'intention d'un `ToolCallRationale` serialise, si present.
pub(super) fn extract_rationale(v: &serde_json::Value) -> Option<String> {
    let r = v.get("rationale")?;
    if r.is_null() {
        return None;
    }
    let summary = r.get("summary")?.as_str()?;
    if summary.is_empty() {
        None
    } else {
        Some(summary.to_string())
    }
}

/// Extrait le message humain d'un `ErrorAnalysis` serialise (avec le conseil
/// eventuel), si present.
pub(super) fn extract_analysis(v: &serde_json::Value) -> Option<String> {
    let a = v.get("analysis")?;
    if a.is_null() {
        return None;
    }
    let human = a.get("human_message")?.as_str()?;
    if human.is_empty() {
        return None;
    }
    match a.get("suggested_action").and_then(|x| x.as_str()) {
        Some(action) if !action.is_empty() => Some(format!("{human}  (hint: {action})")),
        _ => Some(human.to_string()),
    }
}

// ─── Decision d'approbation ──────────────────────────────────────────────────

/// Decision saisie au prompt d'approbation d'outil.
#[derive(Debug, PartialEq)]
pub(super) enum ToolDecisionInput {
    /// Autoriser une fois.
    Accept,
    /// Toujours autoriser (une portee sera demandee ensuite).
    Always,
    /// Refuser, avec une raison optionnelle.
    Refuse(Option<String>),
    /// Saisie non reconnue.
    Invalid,
}

/// Parse la decision saisie au prompt. Accepte le francais et l'anglais.
///
/// `a`/`autoriser`/`accept`/`oui` -> Accept ; `t`/`toujours`/`always` -> Always ;
/// `r`/`refuser`/`refuse`/`non` [raison] -> Refuse. Le reste est Invalid.
pub(super) fn parse_tool_decision(input: &str) -> ToolDecisionInput {
    let trimmed = input.trim();
    let (head, rest) = match trimmed.split_once(char::is_whitespace) {
        Some((h, r)) => (h, r.trim()),
        None => (trimmed, ""),
    };
    match head.to_ascii_lowercase().as_str() {
        "a" | "autoriser" | "accept" | "oui" | "y" => ToolDecisionInput::Accept,
        "t" | "toujours" | "always" => ToolDecisionInput::Always,
        "r" | "refuser" | "refuse" | "non" | "n" => {
            let reason = if rest.is_empty() {
                None
            } else {
                Some(rest.to_string())
            };
            ToolDecisionInput::Refuse(reason)
        }
        _ => ToolDecisionInput::Invalid,
    }
}

/// Traduit un choix de portee en valeur wire snake_case attendue par l'API.
///
/// Entree vide ou `1` -> `this_session` (defaut, le moins collant) ; `2` ->
/// `this_tool` ; `3` -> `this_project`. Le reste est invalide.
pub(super) fn parse_scope_choice(input: &str) -> Option<&'static str> {
    match input.trim() {
        "" | "1" => Some("this_session"),
        "2" => Some("this_tool"),
        "3" => Some("this_project"),
        _ => None,
    }
}
