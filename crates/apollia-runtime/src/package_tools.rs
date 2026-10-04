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
//!
//! A host reads the installed packages once, when the runtime starts. When a
//! package is uninstalled while the runtime runs, [`WithoutPackage`] stands
//! in front of the host and hands out its executors minus those of that
//! package, so its tools are gone from the next run on.

use std::sync::Arc;

use apollia_tools::executor::ToolExecutor;
use tokio_util::sync::CancellationToken;

/// Source of the executors of the package tools.
pub trait PackageToolHost: Send + Sync {
    /// One executor per package tool, each running on behalf of `caller`, the
    /// agent whose run builds the dispatcher, and stopping when `cancel` is
    /// cancelled.
    fn executors(&self, caller: &str, cancel: &CancellationToken) -> Vec<Box<dyn ToolExecutor>>;
}

/// A host minus the tools of one package, uninstalled since the host read
/// the installed packages.
pub struct WithoutPackage {
    inner: Arc<dyn PackageToolHost>,
    /// `pkg:<package>/`, the prefix of the names of its tools.
    prefix: String,
}

impl WithoutPackage {
    /// `inner` without the tools of `package`.
    pub fn new(inner: Arc<dyn PackageToolHost>, package: &str) -> Self {
        Self {
            inner,
            prefix: format!(
                "{}{package}/",
                apollia_tools::apollia_manifest::PACKAGE_TOOL_PREFIX
            ),
        }
    }
}

impl PackageToolHost for WithoutPackage {
    fn executors(&self, caller: &str, cancel: &CancellationToken) -> Vec<Box<dyn ToolExecutor>> {
        self.inner
            .executors(caller, cancel)
            .into_iter()
            .filter(|executor| !executor.name().starts_with(&self.prefix))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_tools::executor::ToolExecutionError;

    struct Named(&'static str);

    impl ToolExecutor for Named {
        fn name(&self) -> &str {
            self.0
        }

        fn execute(
            &self,
            _input: serde_json::Value,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<Output = Result<serde_json::Value, ToolExecutionError>>
                    + Send
                    + '_,
            >,
        > {
            Box::pin(async { Ok(serde_json::Value::Null) })
        }
    }

    struct Host;

    impl PackageToolHost for Host {
        fn executors(&self, _: &str, _: &CancellationToken) -> Vec<Box<dyn ToolExecutor>> {
            vec![
                Box::new(Named("pkg:crm/lookup")),
                Box::new(Named("pkg:crm-extra/lookup")),
                Box::new(Named("pkg:desk/reply")),
            ]
        }
    }

    // GIVEN a host holding the tools of three packages, two with close names
    // WHEN one package is taken out
    // THEN only its tools are gone: a package whose name starts the same stays
    #[test]
    fn test_only_the_tools_of_the_uninstalled_package_are_gone() {
        let host = WithoutPackage::new(Arc::new(Host), "crm");

        let names: Vec<String> = host
            .executors("agent", &CancellationToken::new())
            .iter()
            .map(|e| e.name().to_owned())
            .collect();

        assert_eq!(names, ["pkg:crm-extra/lookup", "pkg:desk/reply"]);
    }
}
