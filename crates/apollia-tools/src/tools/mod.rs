//! Native tools bundled with `apollia-tools`.
//!
//! Each tool is a self-contained module exposing a struct with a `descriptor()` method
//! that returns a valid `ToolDescriptor` for registration in `ToolRegistry`.

pub mod ask_user;
pub mod bash_executor;
pub mod bash_validator;
pub mod code_context;
pub mod file_edit;
pub mod file_glob;
pub mod file_grep;
pub mod file_list;
pub mod file_read;
pub mod file_write;
#[cfg(feature = "http")]
pub mod http_fetch;
#[cfg(feature = "memory-search")]
pub mod memory_search;
pub mod notebook_edit;
pub mod notebook_read;
pub mod python_discovery;
pub mod python_executor;
pub mod risk_classifier;
pub mod rlimits;
pub mod shell_discovery;
pub mod source_search;
#[cfg(feature = "web-read")]
pub mod web_read;
#[cfg(feature = "web-search")]
pub mod web_search;

/// Every native tool descriptor this build ships, feature gates included.
///
/// The family a governance decision reads for a native tool is the one stated
/// here.
pub fn native_descriptors() -> Vec<crate::descriptor::ToolDescriptor> {
    let mut d = vec![
        ask_user::AskUser::descriptor(),
        bash_executor::BashExecutor::descriptor(),
        code_context::CodeContext::descriptor(),
        file_edit::FileEdit::descriptor(),
        file_glob::FileGlob::descriptor(),
        file_grep::FileGrep::descriptor(),
        file_list::FileList::descriptor(),
        file_read::FileRead::descriptor(),
        file_write::FileWrite::descriptor(),
        notebook_edit::NotebookEdit::descriptor(),
        notebook_read::NotebookRead::descriptor(),
        python_executor::PythonExecutor::descriptor(),
        source_search::SourceSearchTool::descriptor(),
    ];
    #[cfg(feature = "http")]
    d.push(http_fetch::HttpFetch::descriptor());
    #[cfg(feature = "memory-search")]
    d.push(memory_search::MemorySearchTool::descriptor());
    #[cfg(feature = "web-read")]
    d.push(web_read::WebRead::descriptor());
    #[cfg(feature = "web-search")]
    d.push(web_search::WebSearch::descriptor());
    d
}

#[cfg(test)]
mod tests {
    use crate::descriptor::ToolDescriptor;
    use apollia_core::SandboxProfile;

    fn all_descriptors() -> Vec<ToolDescriptor> {
        super::native_descriptors()
    }

    #[test]
    fn every_native_descriptor_passes_registration_validation() {
        // GIVEN every descriptor this crate ships
        // WHEN each is validated the way ToolRegistry validates it at registration
        // THEN none is refused: a descriptor the registry rejects is a tool the
        // agent never receives
        for d in all_descriptors() {
            assert!(d.validate().is_ok(), "{}: {:?}", d.name, d.validate());
        }
    }

    #[test]
    fn no_mutating_tool_declares_the_read_only_profile() {
        // GIVEN every descriptor this crate ships
        // WHEN the declared isolation intent is crossed with the mutation flag
        // THEN no tool that mutates state claims the ReadOnly profile. That
        // profile is what the audit trail records for the invocation, so a
        // mutating tool declaring it writes a false line in the ledger.
        for d in all_descriptors() {
            assert!(
                !matches!(d.sandbox_profile, SandboxProfile::ReadOnly) || d.is_read_only,
                "{} declares SandboxProfile::ReadOnly but is_read_only is false",
                d.name
            );
        }
    }
}
