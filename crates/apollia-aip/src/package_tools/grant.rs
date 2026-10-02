//! What the host grants a package tool beyond the bare context.
//!
//! A package tool runs with a bare `ctx`: no memory, no user profile. The one
//! exception is decided by the host, never by a package: a [`ProfileGrant`]
//! names a package, a module, and the exact source the binary carries for it.
//! A tool of that package whose module on disk is that source, byte for byte,
//! runs with the user profile writable (its writes recorded as the
//! onboarding's) and with the memory namespace the grant names. A package that
//! takes the same name with other code gets nothing.
//!
//! The onboarding of Apollia is the only holder: its tool records the profile
//! facts of the first conversation, as the onboarding agent did when it ran
//! code.

use std::path::{Path, PathBuf};

use apollia_core::paths::DataFile;
use apollia_memory::manager::MemoryManager;

use crate::context::RuntimeContext;
use crate::memory::MemoryInterface;
use crate::profile::ProfileInterface;

/// Package of the onboarding of Apollia.
pub const ONBOARDING_PACKAGE: &str = "onboarding-agent";

/// Module of the onboarding tool, relative to the package root.
pub const ONBOARDING_MODULE: &str = "tools/onboarding.py";

/// Source of the onboarding tool, as the binary ships it.
pub const ONBOARDING_SOURCE: &str =
    include_str!("../../../../agents/system/onboarding-agent/tools/onboarding.py");

/// Memory namespace of the onboarding, which the desktop reads to learn that
/// the onboarding is complete and which permission rules it proposes.
pub const ONBOARDING_MEMORY: &str = "onboarding";

/// The user profile and a memory namespace, granted to the tools of one
/// package whose module is exactly `source`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProfileGrant {
    /// Package name.
    pub package: &'static str,
    /// Module, relative to the package root.
    pub module: &'static str,
    /// The source the module must be, byte for byte.
    pub source: &'static str,
    /// Memory namespace the tool reads and writes.
    pub memory_namespace: &'static str,
}

impl ProfileGrant {
    /// The grant of the onboarding of Apollia.
    pub fn onboarding() -> Self {
        Self {
            package: ONBOARDING_PACKAGE,
            module: ONBOARDING_MODULE,
            source: ONBOARDING_SOURCE,
            memory_namespace: ONBOARDING_MEMORY,
        }
    }

    /// Whether the tool of `package`, whose module is `module` under `root`,
    /// is the code this grant names.
    pub fn covers(&self, package: &str, root: &Path, module: &str) -> bool {
        package == self.package
            && module == self.module
            && std::fs::read(root.join(module)).is_ok_and(|bytes| bytes == self.source.as_bytes())
    }
}

/// A grant resolved against the data directory of the runtime.
#[derive(Debug, Clone)]
pub(super) struct GrantedStores {
    grant: ProfileGrant,
    user_memory_db: PathBuf,
    memory_dir: PathBuf,
}

impl GrantedStores {
    /// `grant` with the stores under `data_dir`.
    pub(super) fn new(grant: ProfileGrant, data_dir: &Path) -> Self {
        Self {
            grant,
            user_memory_db: DataFile::UserMemory.path(data_dir),
            memory_dir: data_dir.join("memory"),
        }
    }

    /// Whether the tool of `package` at `module` under `root` is covered.
    pub(super) fn covers(&self, package: &str, root: &Path, module: &str) -> bool {
        self.grant.covers(package, root, module)
    }

    /// The context of a call by `caller`, with the profile and the memory the
    /// grant gives.
    pub(super) fn context(&self, caller: &str) -> RuntimeContext {
        let namespace = self.grant.memory_namespace.to_string();
        let manager = MemoryManager::new(&self.memory_dir, Some(namespace.clone()), vec![]);
        let memory = MemoryInterface::new(manager, namespace, caller.to_string());
        let profile =
            ProfileInterface::new(self.user_memory_db.clone(), caller.to_string(), true, true);
        let ctx = RuntimeContext::for_package_tool(caller).with_profile(profile);
        match memory {
            Some(memory) => ctx.with_memory(memory),
            None => ctx,
        }
    }
}
