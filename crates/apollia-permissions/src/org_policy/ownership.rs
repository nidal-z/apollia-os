//! Who may modify the policy folder and its files.
//!
//! The policy is only as authentic as the rights on its folder: the
//! organisation key sits next to the policy, so a folder the user can write
//! to is a folder where the user can put their own key and their own policy.

use std::fs::Metadata;
use std::path::Path;

#[cfg(windows)]
mod windows;

/// Who must be the only one able to modify the policy folder and its files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum OwnerRequirement {
    /// The system administrator only: root on Unix, the Administrators on
    /// Windows. The one requirement a release build knows.
    System,
    /// Anyone owning the folder, provided neither its group nor the others
    /// can modify it. Only present in debug builds, which serve the tests and
    /// the desktop automaton.
    #[cfg(debug_assertions)]
    CurrentUser,
}

/// Checks that `path` is neither a symbolic link nor modifiable by anyone the
/// requirement does not trust. Returns why it is refused.
pub(super) fn check(path: &Path, requirement: OwnerRequirement) -> Result<(), String> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|e| format!("cannot read the rights of {}: {e}", path.display()))?;
    if metadata.file_type().is_symlink() {
        return Err(format!("{} is a symbolic link", path.display()));
    }
    check_metadata(path, &metadata, requirement)
}

#[cfg(unix)]
fn check_metadata(
    path: &Path,
    metadata: &Metadata,
    requirement: OwnerRequirement,
) -> Result<(), String> {
    use std::os::unix::fs::MetadataExt as _;

    const GROUP_WRITE: u32 = 0o020;
    const OTHER_WRITE: u32 = 0o002;
    let mode = metadata.mode();
    if mode & OTHER_WRITE != 0 {
        return Err(format!("{} is writable by every user", path.display()));
    }
    match requirement {
        OwnerRequirement::System => {
            if metadata.uid() != 0 {
                return Err(format!(
                    "{} is owned by uid {}, not by root",
                    path.display(),
                    metadata.uid()
                ));
            }
            if mode & GROUP_WRITE != 0 && metadata.gid() != 0 {
                return Err(format!(
                    "{} is writable by group {}, not only by root",
                    path.display(),
                    metadata.gid()
                ));
            }
        }
        #[cfg(debug_assertions)]
        OwnerRequirement::CurrentUser => {
            if mode & GROUP_WRITE != 0 {
                return Err(format!("{} is writable by its group", path.display()));
            }
        }
    }
    Ok(())
}

#[cfg(windows)]
fn check_metadata(
    path: &Path,
    _metadata: &Metadata,
    requirement: OwnerRequirement,
) -> Result<(), String> {
    match requirement {
        OwnerRequirement::System => windows::check_system_owner(path),
        #[cfg(debug_assertions)]
        OwnerRequirement::CurrentUser => Ok(()),
    }
}

#[cfg(all(not(unix), not(windows)))]
fn check_metadata(
    path: &Path,
    _metadata: &Metadata,
    requirement: OwnerRequirement,
) -> Result<(), String> {
    match requirement {
        // A platform with neither Unix modes nor Windows access lists: the
        // rights cannot be read, so the folder is refused and a policy is
        // never trusted on rights that were not read.
        OwnerRequirement::System => Err(format!(
            "the owner of {} cannot be checked on this platform",
            path.display()
        )),
        #[cfg(debug_assertions)]
        OwnerRequirement::CurrentUser => Ok(()),
    }
}
