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
pub mod python_executor;
pub mod risk_classifier;
pub mod rlimits;
#[cfg(feature = "web-read")]
pub mod web_read;
#[cfg(feature = "web-search")]
pub mod web_search;
