//! What the preview says about one access an agent of a package declares:
//! what it permits, whether it is sensitive, and its action family.

use apollia_tools::apollia_manifest::{
    ActionFamily as PackageFamily, ApolliaManifest, PACKAGE_TOOL_PREFIX,
};
use apollia_tools::{ActionFamily, ToolDescriptor};

use crate::governance::DeclaredAccess;

/// The action family of the access `access_id`: a tool of the package by its
/// descriptor in the manifest, a tool the registry holds by its descriptor
/// there, `unclassified` for anything else.
pub(super) fn access_family(
    manifest: &ApolliaManifest,
    access_id: &str,
    tools: &[ToolDescriptor],
) -> String {
    let own_prefix = format!("{PACKAGE_TOOL_PREFIX}{}/", manifest.package.name);
    if let Some(tool) = access_id
        .strip_prefix(&own_prefix)
        .and_then(|name| manifest.tools.iter().find(|t| t.name == name))
    {
        return family_name(tool.action).to_owned();
    }
    tools
        .iter()
        .find(|t| t.name == access_id)
        .and_then(|t| serde_json::to_value(t.family).ok())
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_else(|| "unclassified".to_owned())
}

/// The access `access_id` as an agent of `manifest` declares it.
///
/// A tool of the package is described by its descriptor in the manifest; a
/// tool the registry holds, by its descriptor there. Anything else (a tool
/// of a connection not added yet) is described by its name and counted as
/// sensitive, since nothing classifies it.
pub(super) fn declared_access(
    manifest: &ApolliaManifest,
    access_id: &str,
    tools: &[ToolDescriptor],
) -> DeclaredAccess {
    let own_prefix = format!("{PACKAGE_TOOL_PREFIX}{}/", manifest.package.name);
    if let Some(tool) = access_id
        .strip_prefix(&own_prefix)
        .and_then(|name| manifest.tools.iter().find(|t| t.name == name))
    {
        return DeclaredAccess {
            access_id: access_id.to_owned(),
            permits: tool.description.clone(),
            declared_sensitive: tool.is_sensitive(),
        };
    }
    if let Some(tool) = tools.iter().find(|t| t.name == access_id) {
        return DeclaredAccess {
            access_id: access_id.to_owned(),
            permits: tool.description.clone(),
            declared_sensitive: tool.family != ActionFamily::Read,
        };
    }
    DeclaredAccess {
        access_id: access_id.to_owned(),
        permits: format!("use {access_id}"),
        declared_sensitive: true,
    }
}

pub(super) fn family_name(family: PackageFamily) -> &'static str {
    match family {
        PackageFamily::Read => "read",
        PackageFamily::ReadPersonal => "read_personal",
        PackageFamily::WriteOrDelete => "write_or_delete",
        PackageFamily::SendExternal => "send_external",
        PackageFamily::ExecuteCode => "execute_code",
    }
}
