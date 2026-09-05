//! Native tools bundled with `apollia-tools`.
//!
//! Each tool is a self-contained module exposing a struct with a `descriptor()` method
//! that returns a valid `ToolDescriptor` for registration in `ToolRegistry`.

pub mod ask_user;
pub mod bash_executor;
pub mod bash_validator;
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
pub mod permission_rules;
pub mod python_discovery;
pub mod python_executor;
pub mod risk_classifier;
pub mod rlimits;
pub mod shell_discovery;
#[cfg(feature = "web-read")]
pub mod web_read;
#[cfg(feature = "web-search")]
pub mod web_search;

#[cfg(test)]
mod tests {
    use crate::descriptor::ToolDescriptor;
    use apollia_core::SandboxProfile;

    /// Every descriptor this crate ships, feature gates included.
    fn all_descriptors() -> Vec<ToolDescriptor> {
        let mut d = vec![
            super::ask_user::AskUser::descriptor(),
            super::bash_executor::BashExecutor::descriptor(),
            super::file_edit::FileEdit::descriptor(),
            super::file_glob::FileGlob::descriptor(),
            super::file_grep::FileGrep::descriptor(),
            super::file_list::FileList::descriptor(),
            super::file_read::FileRead::descriptor(),
            super::file_write::FileWrite::descriptor(),
            super::notebook_edit::NotebookEdit::descriptor(),
            super::notebook_read::NotebookRead::descriptor(),
            super::permission_rules::PermissionRuleAdd::descriptor(),
            super::permission_rules::PermissionRuleRemove::descriptor(),
            super::permission_rules::PermissionRuleList::descriptor(),
            super::python_executor::PythonExecutor::descriptor(),
        ];
        #[cfg(feature = "http")]
        d.push(super::http_fetch::HttpFetch::descriptor());
        #[cfg(feature = "memory-search")]
        d.push(super::memory_search::MemorySearchTool::descriptor());
        #[cfg(feature = "web-read")]
        d.push(super::web_read::WebRead::descriptor());
        #[cfg(feature = "web-search")]
        d.push(super::web_search::WebSearch::descriptor());
        d
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
