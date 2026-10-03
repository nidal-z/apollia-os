//! Installing a `.apo` package: what it contains, who signed it, what it
//! asks for, and what an update adds.
//!
//! Two steps, so that nothing is written before the user has seen what a
//! package does:
//!
//! - [`preview`] reads an archive and writes nothing. It reports the content
//!   (agents, workflows, tools), the accesses each agent asks for with what
//!   each permits (as the governance contract resolves them), the publisher
//!   and the state of the signature, the connections the package requires
//!   (present or missing) and the MCP servers it brings, and, when a version
//!   is already installed, what the incoming one adds.
//! - [`prepare`] repeats the preview, refuses what the preview refuses,
//!   checks the confirmations the preview calls for, and extracts the archive
//!   into a staging folder; [`commit`] then builds the package environment,
//!   adds the MCP servers, writes the definitions and the content of the
//!   package, and records the installation in the audit journal.
//!
//! A signature that does not hold is refused. An unsigned package installs
//! with a notice, unless the governance contract requires a signature; a
//! package Apollia ships inside its binary is the exception, see
//! [`install_embedded`]. A
//! version older than the installed one is refused unless the caller allows
//! it. The user's overrides of an agent survive an update: only the package
//! definition is replaced.
//!
//! [`uninstall_package`] and [`uninstall_agent`] remove what an installation
//! wrote, the triggers and model of each agent, and the MCP servers the
//! package brought; the memory and the history of the agents go too when the
//! user asks.
//!
//! A publisher is trusted when the key that signed the package is in the
//! keyring ([`apollia_tools::TrustedKeyRepository`]); the preview then names
//! it by the label the user gave. Any other signed package is named by the
//! publisher it declares, which nothing verifies, and by its key fingerprint.
//! When the organisation policy names its trusted publishers, only their keys
//! are trusted, and with a signature required a package signed by any other
//! key is refused ([`InstallRefusal::PublisherNotTrusted`]).

mod embedded;
mod install;
mod preview;
mod uninstall;

#[cfg(test)]
mod policy_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_embedded;
#[cfg(test)]
mod tests_install;
#[cfg(test)]
mod tests_uninstall;

use apollia_tools::apo_archive::ApoError;

pub use embedded::{install_embedded, EmbeddedOutcome, EMBEDDED_ACTOR};
pub use install::{
    commit, prepare, CommitTargets, InstallOptions, InstallReport, McpServerAdder, StagedInstall,
};
pub use preview::{
    connection_matches, preview, AccessPreview, AddedAccess, AgentPreview, BroughtServerPreview,
    InstallRefusal, ItemPreview, PackageNotice, PackageOrigin, PackagePreview, PreviewSources,
    RequiredConnectionPreview, SignaturePreview, SignatureStatus, TriggerPreview, UpdateDiff,
    WorkflowPreview,
};
pub use uninstall::{
    uninstall_agent, uninstall_package, UninstallError, UninstallOptions, UninstallReport,
    UninstallTargets,
};

/// Errors of the preview and of the installation of a package.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PackageError {
    /// The archive cannot be read, or its manifest is invalid.
    #[error(transparent)]
    Archive(#[from] ApoError),
    /// The installation is refused as the preview says.
    #[error("{0}")]
    Refused(InstallRefusal),
    /// The package installs Python dependencies the caller has not confirmed.
    #[error(
        "the package installs Python dependencies ({}); confirm them to install",
        .packages.join(", ")
    )]
    DepsConfirmationRequired {
        /// The requirements, as the package declares them.
        packages: Vec<String>,
    },
    /// The update adds accesses, required connections or MCP servers the
    /// caller has not confirmed.
    #[error("the update adds accesses or connections; review the preview and confirm them")]
    ChangesConfirmationRequired,
    /// The package brings MCP servers and no MCP manager runs.
    #[error("the package brings MCP servers and MCP is not available in this runtime")]
    McpUnavailable,
    /// An MCP server the package brings could not be added.
    #[error("cannot add the MCP server `{name}`: {message}")]
    McpServer {
        /// Server name.
        name: String,
        /// Why it was refused.
        message: String,
    },
    /// The package environment could not be built.
    #[error("cannot build the environment of the package: {0}")]
    Environment(String),
    /// A store of `agents.db` failed.
    #[error("package store: {0}")]
    Store(String),
    /// A file operation failed.
    #[error("{path}: {source}")]
    Io {
        /// The file or folder involved.
        path: std::path::PathBuf,
        /// The underlying error.
        #[source]
        source: std::io::Error,
    },
    /// The installation could not be recorded in the audit journal.
    #[error("cannot record the installation: {0}")]
    Journal(#[from] crate::governance::GovernanceError),
}

impl From<apollia_tools::PackageRepositoryError> for PackageError {
    fn from(error: apollia_tools::PackageRepositoryError) -> Self {
        PackageError::Store(error.to_string())
    }
}

impl From<apollia_tools::AgentRepositoryError> for PackageError {
    fn from(error: apollia_tools::AgentRepositoryError) -> Self {
        PackageError::Store(error.to_string())
    }
}

impl From<apollia_tools::TrustedKeyError> for PackageError {
    fn from(error: apollia_tools::TrustedKeyError) -> Self {
        PackageError::Store(error.to_string())
    }
}
