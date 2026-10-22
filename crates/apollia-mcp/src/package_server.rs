//! An MCP server a package brings, as the connection it becomes.
//!
//! The `[[mcp_servers]]` table of `apollia.toml` has the keys of
//! [`McpServerConfig`] minus `format_version`, which versions `mcp.toml` and
//! not a package. [`config_from_package`] turns one entry into the
//! configuration the installer adds, its origin set to
//! [`Provenance::Package`]. Which package brought the server is not written
//! here: the package store keeps it, so `mcp.toml` stays the user's file.

use apollia_tools::apollia_manifest::McpServerDecl;

use crate::config::McpServerConfig;
use crate::origin::{ConnectionOrigin, Provenance};

/// The configuration of the server `decl` declares, with its origin.
#[must_use]
pub fn config_from_package(decl: &McpServerDecl) -> McpServerConfig {
    McpServerConfig {
        format_version: 1,
        name: decl.name.clone(),
        command: decl.command.clone(),
        args: decl.args.clone(),
        env: decl
            .env
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
        transport: decl.transport.clone(),
        url: decl.url.clone(),
        init_timeout_secs: decl.init_timeout_secs,
        call_timeout_secs: decl.call_timeout_secs,
        max_response_bytes: decl.max_response_bytes,
        max_tools: decl.max_tools,
        tags: decl.tags.clone(),
        tool_families: Default::default(),
        operator_families: Default::default(),
        origin: Some(ConnectionOrigin {
            provenance: Provenance::Package,
            official: false,
            entry: None,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_server_decl_has_exactly_the_keys_of_mcp_server_config() {
        // GIVEN a server config from mcp.toml, without its file-format version
        let config: McpServerConfig = toml::from_str(
            "name = \"ledger\"\ncommand = \"uvx\"\nurl = \"http://x\"\ntags = [\"t\"]\n",
        )
        .expect("config parses");
        let mut table = toml::Value::try_from(&config).expect("config serialises");
        if let Some(t) = table.as_table_mut() {
            t.remove("format_version");
            t.insert("args".into(), toml::Value::Array(vec!["a".into()]));
            let mut env = toml::Table::new();
            env.insert("K".into(), "v".into());
            t.insert("env".into(), toml::Value::Table(env));
        }

        // WHEN it is read as a package MCP server, then written back and read as a config
        let decl: McpServerDecl = table.clone().try_into().expect("decl accepts every key");
        let back: McpServerConfig =
            toml::from_str(&toml::to_string(&decl).expect("serialises")).expect("config reads");
        let mut back_table = toml::Value::try_from(&back).expect("serialises");
        if let Some(t) = back_table.as_table_mut() {
            t.remove("format_version");
        }

        // THEN both directions agree key for key
        assert_eq!(back_table, table);
    }

    #[test]
    fn test_config_from_package_keeps_every_value_and_marks_the_origin() {
        // GIVEN a package server with every key set away from its default
        let decl: McpServerDecl = toml::from_str(
            "name = \"crm\"\ncommand = \"uvx\"\nargs = [\"crm-mcp\"]\n\
             env = { TOKEN = \"${CRM_TOKEN}\" }\ntransport = \"stdio\"\n\
             init_timeout_secs = 5\ncall_timeout_secs = 7\n\
             max_response_bytes = 4096\nmax_tools = 3\ntags = [\"crm\"]\n",
        )
        .expect("decl parses");

        // WHEN it becomes a connection configuration
        let config = config_from_package(&decl);

        // THEN every value is carried and the origin says a package brought it
        assert_eq!(config.name, "crm");
        assert_eq!(config.args, vec!["crm-mcp".to_owned()]);
        assert_eq!(
            config.env.get("TOKEN").map(String::as_str),
            Some("${CRM_TOKEN}")
        );
        assert_eq!(config.init_timeout_secs, 5);
        assert_eq!(config.call_timeout_secs, 7);
        assert_eq!(config.max_response_bytes, 4096);
        assert_eq!(config.max_tools, 3);
        assert_eq!(config.tags, vec!["crm".to_owned()]);
        assert_eq!(
            config.origin.map(|o| o.provenance),
            Some(Provenance::Package)
        );
    }
}
