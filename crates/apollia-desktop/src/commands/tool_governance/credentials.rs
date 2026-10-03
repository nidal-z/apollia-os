//! The credentials of the native tools through the seam: embedded, the
//! functions of [`apollia_runtime::tool_credentials`] on this machine's data
//! directory; attached, the daemon's `/api/v1/tool-credentials` routes, which
//! call the same functions.

use apollia_runtime::api::routes_tool_credentials::{
    DeletedCredential, SetCredentialBody, StoredCredential,
};
use apollia_runtime::tool_credentials::{
    self, CredentialEntryView, CredentialError, CredentialTestView,
};

use super::home_base_dir;
use crate::attached::{segment, RuntimeLink};

/// The credentials, of one tool or of all, values left out.
pub(super) async fn list(
    link: &RuntimeLink,
    tool_name: Option<String>,
) -> Result<Vec<CredentialEntryView>, String> {
    match link {
        RuntimeLink::Embedded(_) => tool_credentials::list_credentials(home_base_dir()?, tool_name)
            .await
            .map_err(|e| e.to_string()),
        RuntimeLink::Attached(remote) => {
            let path = match tool_name {
                Some(tool) => format!("/api/v1/tool-credentials?tool_name={}", segment(&tool)),
                None => "/api/v1/tool-credentials".to_owned(),
            };
            Ok(remote.get(&path).await?)
        }
    }
}

/// Stores or replaces one credential.
pub(super) async fn set(
    link: &RuntimeLink,
    tool_name: String,
    key_name: String,
    value: String,
) -> Result<(), String> {
    match link {
        RuntimeLink::Embedded(_) => {
            tool_credentials::set_credential(home_base_dir()?, tool_name, key_name, value)
                .await
                .map_err(|e| e.to_string())
        }
        RuntimeLink::Attached(remote) => {
            let path = credential_path(&tool_name, &key_name)?;
            let _: StoredCredential = remote.put(&path, &SetCredentialBody { value }).await?;
            Ok(())
        }
    }
}

/// Removes one credential; one that is not there is not an error.
pub(super) async fn delete(
    link: &RuntimeLink,
    tool_name: String,
    key_name: String,
) -> Result<(), String> {
    match link {
        RuntimeLink::Embedded(_) => {
            tool_credentials::delete_credential(home_base_dir()?, tool_name, key_name)
                .await
                .map(|_| ())
                .map_err(|e| e.to_string())
        }
        RuntimeLink::Attached(remote) => {
            let path = credential_path(&tool_name, &key_name)?;
            let _: DeletedCredential = remote.delete(&path).await?;
            Ok(())
        }
    }
}

/// Validates the credential of a tool with a live call, made by whichever
/// process runs the runtime.
pub(super) async fn test(
    link: &RuntimeLink,
    tool_name: String,
) -> Result<CredentialTestView, String> {
    match link {
        RuntimeLink::Embedded(_) => tool_credentials::test_credential(home_base_dir()?, tool_name)
            .await
            .map_err(|e| e.to_string()),
        RuntimeLink::Attached(remote) => {
            let path = format!("/api/v1/tool-credential-tests/{}", segment(&tool_name));
            Ok(remote.post(&path, &()).await?)
        }
    }
}

/// The daemon route of one credential. A blank name is refused here, with the
/// daemon's own words, since an empty path segment would not reach its route.
fn credential_path(tool_name: &str, key_name: &str) -> Result<String, String> {
    if tool_name.trim().is_empty() || key_name.trim().is_empty() {
        return Err(CredentialError::EmptyName.to_string());
    }
    Ok(format!(
        "/api/v1/tool-credentials/{}/{}",
        segment(tool_name),
        segment(key_name)
    ))
}

#[cfg(test)]
mod tests;
