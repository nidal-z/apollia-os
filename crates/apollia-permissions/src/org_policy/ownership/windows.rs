//! Owner and access list of a policy file on Windows, read through the Win32
//! security API.
//!
//! A path is trusted when its owner is the Administrators group or the
//! LocalSystem account, and when no entry of its access list lets anyone else
//! modify it. An absent access list grants everything to everyone and is
//! refused. Entries that only reach children (inherit-only) do not apply to
//! the path itself and are skipped; deny entries never grant and are skipped.
//! An allow entry whose layout carries an object type, and whose rights
//! include a write, is refused rather than parsed: this code never reads a
//! trustee it is not sure of.

// The Win32 security API is reached through FFI. Every unsafe block below
// carries its SAFETY comment.
#![allow(unsafe_code)]

use std::os::windows::ffi::OsStrExt as _;
use std::path::Path;

use windows_sys::Win32::Foundation::{LocalFree, ERROR_SUCCESS, GENERIC_ALL, GENERIC_WRITE};
use windows_sys::Win32::Security::Authorization::{GetNamedSecurityInfoW, SE_FILE_OBJECT};
use windows_sys::Win32::Security::{
    GetAce, IsWellKnownSid, WinBuiltinAdministratorsSid, WinLocalSystemSid, ACCESS_ALLOWED_ACE,
    ACE_HEADER, ACL, DACL_SECURITY_INFORMATION, INHERIT_ONLY_ACE, OWNER_SECURITY_INFORMATION,
    PSECURITY_DESCRIPTOR, PSID,
};
use windows_sys::Win32::Storage::FileSystem::{
    DELETE, FILE_APPEND_DATA, FILE_DELETE_CHILD, FILE_WRITE_ATTRIBUTES, FILE_WRITE_DATA,
    FILE_WRITE_EA, WRITE_DAC, WRITE_OWNER,
};
use windows_sys::Win32::System::SystemServices::{
    ACCESS_ALLOWED_ACE_TYPE, ACCESS_ALLOWED_CALLBACK_ACE_TYPE,
    ACCESS_ALLOWED_CALLBACK_OBJECT_ACE_TYPE, ACCESS_ALLOWED_OBJECT_ACE_TYPE,
};

/// Every right that lets its holder change the file, its folder content, its
/// attributes, its access list or its owner. `FILE_WRITE_DATA` and
/// `FILE_APPEND_DATA` are also, on a folder, the rights to add a file and to
/// add a sub-folder.
const WRITE_RIGHTS: u32 = FILE_WRITE_DATA
    | FILE_APPEND_DATA
    | FILE_WRITE_EA
    | FILE_WRITE_ATTRIBUTES
    | FILE_DELETE_CHILD
    | DELETE
    | WRITE_DAC
    | WRITE_OWNER
    | GENERIC_WRITE
    | GENERIC_ALL;

/// Frees the security descriptor `GetNamedSecurityInfoW` allocated.
struct SecurityDescriptor(PSECURITY_DESCRIPTOR);

impl Drop for SecurityDescriptor {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: the pointer was allocated by GetNamedSecurityInfoW, which
            // documents LocalFree as its release, and it is freed once, here.
            unsafe {
                LocalFree(self.0);
            }
        }
    }
}

fn is_trusted(sid: PSID) -> bool {
    // SAFETY: `sid` points inside a security descriptor that outlives this
    // call (owned by the caller's `SecurityDescriptor`), and IsWellKnownSid
    // only reads it.
    unsafe {
        IsWellKnownSid(sid, WinBuiltinAdministratorsSid) != 0
            || IsWellKnownSid(sid, WinLocalSystemSid) != 0
    }
}

/// Checks that `path` is owned by the Administrators or by LocalSystem and
/// that only they can modify it.
pub(super) fn check_system_owner(path: &Path) -> Result<(), String> {
    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut owner: PSID = std::ptr::null_mut();
    let mut dacl: *mut ACL = std::ptr::null_mut();
    let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    // SAFETY: `wide` is a NUL-terminated UTF-16 string alive for the call;
    // every out pointer is a valid local; the group and SACL out pointers are
    // null, which the API accepts when their information is not requested.
    let status = unsafe {
        GetNamedSecurityInfoW(
            wide.as_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            std::ptr::null_mut(),
            &mut dacl,
            std::ptr::null_mut(),
            &mut descriptor,
        )
    };
    let _descriptor = SecurityDescriptor(descriptor);
    if status != ERROR_SUCCESS {
        return Err(format!(
            "cannot read the rights of {}: Win32 error {status}",
            path.display()
        ));
    }
    if owner.is_null() || !is_trusted(owner) {
        return Err(format!(
            "{} is not owned by the Administrators or by the system",
            path.display()
        ));
    }
    if dacl.is_null() {
        return Err(format!(
            "{} has no access list, so everyone can modify it",
            path.display()
        ));
    }
    // SAFETY: `dacl` is non-null and points inside the descriptor kept alive
    // by `_descriptor`; the ACL header is read by value.
    let ace_count = unsafe { std::ptr::read_unaligned(dacl) }.AceCount;
    for index in 0..u32::from(ace_count) {
        let mut ace: *mut core::ffi::c_void = std::ptr::null_mut();
        // SAFETY: `index` is below the entry count of this ACL, and `ace` is
        // a valid out pointer.
        if unsafe { GetAce(dacl, index, &mut ace) } == 0 || ace.is_null() {
            return Err(format!(
                "cannot read entry {index} of the access list of {}",
                path.display()
            ));
        }
        // SAFETY: GetAce returned a pointer to an entry inside the ACL, which
        // starts with an ACE_HEADER.
        let header = unsafe { std::ptr::read_unaligned(ace as *const ACE_HEADER) };
        if u32::from(header.AceFlags) & INHERIT_ONLY_ACE != 0 {
            continue;
        }
        let ace_type = u32::from(header.AceType);
        match ace_type {
            ACCESS_ALLOWED_ACE_TYPE | ACCESS_ALLOWED_CALLBACK_ACE_TYPE => {
                // SAFETY: both types share the ACCESS_ALLOWED_ACE layout: the
                // header, the mask, then the trustee SID starting at
                // `SidStart`, which stays inside the entry.
                let (mask, sid) = unsafe {
                    let allowed = ace as *const ACCESS_ALLOWED_ACE;
                    (
                        std::ptr::read_unaligned(std::ptr::addr_of!((*allowed).Mask)),
                        std::ptr::addr_of!((*allowed).SidStart) as PSID,
                    )
                };
                if mask & WRITE_RIGHTS != 0 && !is_trusted(sid) {
                    return Err(format!(
                        "{} can be modified by someone other than the Administrators",
                        path.display()
                    ));
                }
            }
            ACCESS_ALLOWED_OBJECT_ACE_TYPE | ACCESS_ALLOWED_CALLBACK_OBJECT_ACE_TYPE => {
                // SAFETY: an object entry starts with the header and the mask,
                // like ACCESS_ALLOWED_ACE; only the mask is read.
                let mask = unsafe {
                    let allowed = ace as *const ACCESS_ALLOWED_ACE;
                    std::ptr::read_unaligned(std::ptr::addr_of!((*allowed).Mask))
                };
                if mask & WRITE_RIGHTS != 0 {
                    return Err(format!(
                        "{} carries an object access entry that grants a write",
                        path.display()
                    ));
                }
            }
            _ => {}
        }
    }
    Ok(())
}
