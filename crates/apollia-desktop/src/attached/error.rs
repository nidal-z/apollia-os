//! The errors of the attached mode.

/// Why an attached command did not complete.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub(crate) enum AttachedError {
    /// The command reads state that only exists when the runtime runs in this
    /// process, and no HTTP route serves it yet.
    #[error("{command} is unavailable while Apollia runs as a service")]
    NotRouted {
        /// The command refused.
        command: String,
    },
    /// The HTTP stack could not be initialised.
    #[error("the HTTP client cannot be initialised")]
    Client(#[source] reqwest::Error),
    /// The runtime did not answer: stopped, restarting, or past the deadline.
    #[error("the Apollia service does not answer")]
    Unreachable(#[source] reqwest::Error),
    /// The runtime answered with an error status.
    #[error("the Apollia service refused the request ({status}): {message}")]
    Refused {
        /// The HTTP status.
        status: u16,
        /// The runtime's own message, empty when its body held none.
        message: String,
    },
    /// The runtime answered a body this application cannot read.
    #[error("the Apollia service answered an unreadable body")]
    Body(#[source] apollia_core::net::ReadCappedError),
}

impl AttachedError {
    /// The text the interface shows, in its language.
    pub(crate) fn for_display(&self, locale: crate::i18n::UiLocale) -> String {
        match self {
            Self::NotRouted { command } => crate::i18n::attached_unavailable(locale, command),
            Self::Refused { message, .. } if !message.is_empty() => message.clone(),
            other => other.to_string(),
        }
    }
}

impl From<AttachedError> for String {
    fn from(error: AttachedError) -> Self {
        error.for_display(crate::i18n::locale())
    }
}
