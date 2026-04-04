//! Native tools bundled with `apollia-tools`.
//!
//! Each tool is a self-contained module exposing a struct with a `descriptor()` method
//! that returns a valid `ToolDescriptor` for registration in `ToolRegistry`.

pub mod bash_executor;
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
pub mod persistent_bash;
pub mod python_executor;
