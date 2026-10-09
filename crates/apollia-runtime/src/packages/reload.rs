//! Re-reading the code of the installed packages after one changed.
//!
//! The Python tools and the workflows of the packages are read from the
//! package store by the host of the runtime, which owns the interpreter: the
//! desktop application, or the daemon. The runtime itself only knows that an
//! installation or an uninstallation changed that store. The host leaves a
//! hook here when it starts; the routes that change the store call it, so a
//! package installed while the runtime runs brings its tools and its
//! workflows at once, as it brings its agents.
//!
//! Without a hook nothing is re-read, which is the case of a runtime with no
//! interpreter and of the tests.

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, OnceLock};

/// What re-reads the tools and the workflows of the installed packages.
pub type PackageReload = Arc<dyn Fn() -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

static RELOAD: OnceLock<PackageReload> = OnceLock::new();

/// Leaves `reload` as what re-reads the package code of this process. Returns
/// `false` when a hook was already there, which stays.
pub fn install_package_reload(reload: PackageReload) -> bool {
    RELOAD.set(reload).is_ok()
}

/// Re-reads the tools and the workflows of the installed packages, when the
/// host left a hook for it.
pub async fn reload_package_code() {
    if let Some(reload) = RELOAD.get() {
        reload().await;
        tracing::event!(tracing::Level::INFO, "package.code.reloaded");
    }
}
