//! Native tools bundled with `apollia-tools`.
//!
//! Each tool is a self-contained module exposing a struct with a `descriptor()` method
//! that returns a valid `ToolDescriptor` for registration in `ToolRegistry`.

pub mod bash_executor;
pub mod file_io;
pub mod python_executor;
