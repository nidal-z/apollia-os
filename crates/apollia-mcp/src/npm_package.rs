//! The npm package an MCP server configuration runs, read from its launch
//! command alone.
//!
//! An npm server arrives as `npx -y <package>[@<version>]` (or `bunx`), and
//! once installed is launched as `node <dir>/node_modules/<package>/<entry>`.
//! Both forms name the package; [`npm_package`] reads it from either, so what
//! a connection runs is known from the configuration that launches it, never
//! from a claim stored beside it.

use std::path::Path;

use crate::config::McpServerConfig;

/// Longest accepted npm package name.
const MAX_NAME_LEN: usize = 214;

/// An npm package name: `name` or `@scope/name`, each part lowercase
/// alphanumerics with `-`, `.`, `_` and `~`, not starting with `.` or `_`.
///
/// Narrow on purpose: the name ends up as an argv element of npm and as a
/// path under `node_modules`, so a leading `-` or a `..` must never pass.
#[must_use]
pub fn is_package_name(name: &str) -> bool {
    fn part(p: &str) -> bool {
        !p.is_empty()
            && !p.starts_with(['.', '_'])
            && p.chars().all(|c| {
                c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '.' | '_' | '~')
            })
    }
    if name.is_empty() || name.len() > MAX_NAME_LEN {
        return false;
    }
    match name.strip_prefix('@') {
        Some(scoped) => scoped
            .split_once('/')
            .is_some_and(|(scope, base)| part(scope) && part(base)),
        None => part(name),
    }
}

/// The npm package `config` launches, without its version, or `None` when it
/// launches none: `npx` or `bunx` with a package argument, or `node` on a file
/// under `node_modules`.
#[must_use]
pub fn npm_package(config: &McpServerConfig) -> Option<String> {
    let stem = Path::new(&config.command)
        .file_stem()?
        .to_str()?
        .to_ascii_lowercase();
    let package = match stem.as_str() {
        "npx" | "bunx" => {
            let spec = config.args.iter().find(|a| !a.starts_with('-'))?;
            without_version(spec).to_owned()
        }
        "node" => installed_package(config)?,
        _ => return None,
    };
    is_package_name(&package).then_some(package)
}

/// The package an installed npm connection runs: the one whose directory
/// under `node_modules` holds the file given to `node`.
#[must_use]
pub fn installed_package(config: &McpServerConfig) -> Option<String> {
    let stem = Path::new(&config.command).file_stem()?.to_str()?;
    if !stem.eq_ignore_ascii_case("node") {
        return None;
    }
    let script = config.args.first()?.replace('\\', "/");
    let (_, after) = script.rsplit_once("node_modules/")?;
    let mut parts = after.split('/');
    let first = parts.next()?;
    let package = if first.starts_with('@') {
        format!("{first}/{}", parts.next()?)
    } else {
        first.to_string()
    };
    is_package_name(&package).then_some(package)
}

/// `@scope/name@1.2` and `name@1.2` without their version.
fn without_version(spec: &str) -> &str {
    let search_from = usize::from(spec.starts_with('@'));
    match spec[search_from..].find('@') {
        Some(at) => &spec[..search_from + at],
        None => spec,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(command: &str, args: &[&str]) -> McpServerConfig {
        let mut config: McpServerConfig =
            toml::from_str("name = \"s\"\ncommand = \"x\"\n").expect("config parses");
        config.command = command.to_owned();
        config.args = args.iter().map(|a| (*a).to_owned()).collect();
        config
    }

    #[test]
    fn test_npx_names_its_package_without_the_version() {
        // GIVEN servers launched through npx and bunx, scoped or not, versioned or not
        let cases = [
            (
                config("npx", &["-y", "@notionhq/notion-mcp-server@1.9.0"]),
                "@notionhq/notion-mcp-server",
            ),
            (
                config("/usr/local/bin/npx", &["-y", "server-memory"]),
                "server-memory",
            ),
            (config("bunx", &["mcp-fetch@2"]), "mcp-fetch"),
        ];
        for (config, expected) in cases {
            // WHEN the package is read
            let package = npm_package(&config);
            // THEN it is the package name alone
            assert_eq!(package.as_deref(), Some(expected));
        }
    }

    #[test]
    fn test_an_installed_server_names_the_package_holding_its_entry_point() {
        // GIVEN a server installed by Apollia and launched on node
        let scoped = config(
            "node",
            &["/data/connections/notion/node_modules/@notionhq/notion-mcp-server/bin/cli.mjs"],
        );
        let windows = config(
            "node.exe",
            &["C:\\data\\connections\\m\\node_modules\\server-memory\\dist\\index.js"],
        );
        // WHEN the package is read
        // THEN it is the package whose directory holds the entry point
        assert_eq!(
            npm_package(&scoped).as_deref(),
            Some("@notionhq/notion-mcp-server")
        );
        assert_eq!(npm_package(&windows).as_deref(), Some("server-memory"));
    }

    #[test]
    fn test_a_server_that_launches_no_npm_package_names_none() {
        // GIVEN a Python server, a remote one, a bare node script and a bad name
        let cases = [
            config("uvx", &["mcp-server-time"]),
            config("", &[]),
            config("node", &["/home/ada/server.js"]),
            config("npx", &["-y", "Not_A_Package"]),
        ];
        for config in cases {
            // WHEN the package is read
            // THEN there is none
            assert_eq!(npm_package(&config), None);
        }
    }
}
