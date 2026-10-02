//! The Python tools installed packages ship, as the agent engine reaches them.
//!
//! A package tool is named `pkg:<package>/<tool>` in the shared tool registry
//! and runs in the embedded interpreter. That interpreter lives in
//! `apollia-aip`, which depends on this crate, so the engine reaches the tools
//! through [`PackageToolHost`], implemented there and handed to the chat
//! manager by whoever assembles the runtime.
//!
//! The executors a host hands out go into the same dispatcher as the native
//! tools, behind the same access check: a package tool is called only by an
//! agent whose access list grants it.

use apollia_tools::executor::ToolExecutor;
use tokio_util::sync::CancellationToken;

/// Source of the executors of the package tools.
pub trait PackageToolHost: Send + Sync {
    /// One executor per package tool, each running on behalf of `caller`, the
    /// agent whose run builds the dispatcher, and stopping when `cancel` is
    /// cancelled.
    fn executors(&self, caller: &str, cancel: &CancellationToken) -> Vec<Box<dyn ToolExecutor>>;
}
