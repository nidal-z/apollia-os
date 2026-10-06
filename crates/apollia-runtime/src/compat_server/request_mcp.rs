//! The MCP servers a request brings with it, read from its body.
//!
//! Two public forms name a server in a request: the `mcp` tool of the
//! Responses API (`tools: [{"type": "mcp", "server_url": ...}]`) and the
//! `mcp_servers` of the Messages API (`[{"type": "url", "url": ...}]`). Both
//! are read here into the same [`RequestedServer`], and refused here when they
//! ask for what a request may not have: a process to start, an address that is
//! not HTTP, tools of the client's own beside the server's.
//!
//! A server is named after the host of its address, `request-mcp:<host>`,
//! never after the label the request gives it: the name is what a rule and
//! the journal read, so it cannot be chosen by the caller. A configured
//! connection can never carry that name, which holds a colon.

use reqwest::Url;
use serde_json::Value;

use super::relay::Endpoint;

/// The prefix of the name of every server a request brings.
pub(crate) const SERVER_PREFIX: &str = "request-mcp:";

/// The most servers one request may bring.
pub(crate) const MAX_SERVERS: usize = 4;

/// Headers the transport sets itself; a request may not set them.
const RESERVED_HEADERS: [&str; 6] = [
    "host",
    "content-type",
    "content-length",
    "accept",
    "mcp-session-id",
    "transfer-encoding",
];

/// What every refusal of a process says.
const NO_PROCESS: &str = "A request may only bring an MCP server reached over HTTP: \
                          a request never starts a process, so `stdio` is refused.";

/// One MCP server a request brings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RequestedServer {
    /// `request-mcp:<host>`.
    pub(crate) name: String,
    /// Where it is reached, `http` or `https`.
    pub(crate) url: Url,
    /// Headers sent with every call, the authorization among them.
    pub(crate) headers: Vec<(String, String)>,
    /// The tools the request keeps, by their name on the server; `None` keeps
    /// them all.
    pub(crate) allowed_tools: Option<Vec<String>>,
}

/// Whether `body`, sent on `endpoint`, brings an MCP server.
pub(crate) fn brings_mcp(endpoint: Endpoint, body: &Value) -> bool {
    match endpoint {
        Endpoint::Responses => tools_of(body)
            .iter()
            .any(|tool| kind_of(tool) == Some("mcp")),
        Endpoint::Messages => match body.get("mcp_servers") {
            Some(Value::Array(servers)) => !servers.is_empty(),
            Some(Value::Null) | None => false,
            Some(_) => true,
        },
        _ => false,
    }
}

fn tools_of(body: &Value) -> &[Value] {
    body.get("tools")
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

fn kind_of(entry: &Value) -> Option<&str> {
    entry.get("type").and_then(Value::as_str)
}

/// Whether `entry` asks for a process: a command, or a transport that is one.
fn asks_for_a_process(entry: &Value) -> bool {
    entry.get("command").is_some()
        || entry.get("args").is_some()
        || kind_of(entry) == Some("stdio")
        || entry.get("transport").and_then(Value::as_str) == Some("stdio")
}

/// The name of the server reached at `url`: its host, lower case, with
/// anything a name does not hold replaced.
fn name_of(url: &Url) -> Option<String> {
    let host = url
        .host_str()?
        .trim_matches(['[', ']'])
        .to_ascii_lowercase();
    if host.is_empty() {
        return None;
    }
    let host: String = host
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    Some(format!("{SERVER_PREFIX}{host}"))
}

/// The address `raw` names, when a request may bring it.
fn address(raw: Option<&Value>, field: &str) -> Result<Url, String> {
    let Some(raw) = raw.and_then(Value::as_str) else {
        return Err(format!(
            "An MCP server of a request needs its address in `{field}`."
        ));
    };
    let url = Url::parse(raw).map_err(|_| format!("`{field}` is not an address: '{raw}'."))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(NO_PROCESS.to_owned());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(format!(
            "`{field}` must not carry a user name or a password; send the secret in the \
             authorization of the server."
        ));
    }
    Ok(url)
}

fn header(name: &str, value: &str) -> Result<(String, String), String> {
    let known = axum::http::HeaderName::from_bytes(name.as_bytes()).is_ok()
        && axum::http::HeaderValue::from_str(value).is_ok();
    if !known {
        return Err(format!(
            "'{name}' is not a header an MCP server can be sent."
        ));
    }
    if RESERVED_HEADERS.contains(&name.to_ascii_lowercase().as_str()) {
        return Err(format!(
            "The header '{name}' is set by Apollia; a request cannot set it."
        ));
    }
    Ok((name.to_owned(), value.to_owned()))
}

fn bearer(token: Option<&Value>, field: &str) -> Result<Option<(String, String)>, String> {
    match token {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(token)) => {
            let value = if token.to_ascii_lowercase().starts_with("bearer ") {
                token.clone()
            } else {
                format!("Bearer {token}")
            };
            header("Authorization", &value).map(Some)
        }
        Some(_) => Err(format!("`{field}` must be a string.")),
    }
}

fn names(list: &Value, field: &str) -> Result<Vec<String>, String> {
    let wrong = || format!("`{field}` must be a list of tool names.");
    list.as_array()
        .ok_or_else(wrong)?
        .iter()
        .map(|name| name.as_str().map(str::to_owned).ok_or_else(wrong))
        .collect()
}

/// The server one `mcp` tool of the Responses API names.
fn responses_server(tool: &Value) -> Result<RequestedServer, String> {
    if asks_for_a_process(tool) {
        return Err(NO_PROCESS.to_owned());
    }
    if tool.get("connector_id").is_some() {
        return Err(
            "A connector named by `connector_id` is not served: name the MCP server by its \
             address, in `server_url`."
                .to_owned(),
        );
    }
    let url = address(tool.get("server_url"), "server_url")?;
    let mut headers = Vec::new();
    match tool.get("headers") {
        None | Some(Value::Null) => {}
        Some(Value::Object(given)) => {
            for (name, value) in given {
                let value = value
                    .as_str()
                    .ok_or_else(|| format!("The header '{name}' must be a string."))?;
                headers.push(header(name, value)?);
            }
        }
        Some(_) => return Err("`headers` must be an object of strings.".to_owned()),
    }
    if let Some(authorization) = bearer(tool.get("authorization"), "authorization")? {
        headers.retain(|(name, _)| !name.eq_ignore_ascii_case("authorization"));
        headers.push(authorization);
    }
    let allowed_tools = match tool.get("allowed_tools") {
        None | Some(Value::Null) => None,
        Some(Value::Object(filter)) => match filter.get("tool_names") {
            Some(list) => Some(names(list, "allowed_tools.tool_names")?),
            None => None,
        },
        Some(list) => Some(names(list, "allowed_tools")?),
    };
    finish(url, headers, allowed_tools)
}

/// The server one entry of `mcp_servers` of the Messages API names, `None`
/// when the request turned it off.
fn messages_server(entry: &Value) -> Result<Option<RequestedServer>, String> {
    if asks_for_a_process(entry) {
        return Err(NO_PROCESS.to_owned());
    }
    if kind_of(entry) != Some("url") {
        return Err(
            "An entry of `mcp_servers` must have the type `url` and give its address in `url`."
                .to_owned(),
        );
    }
    let url = address(entry.get("url"), "url")?;
    let headers = bearer(entry.get("authorization_token"), "authorization_token")?
        .into_iter()
        .collect();
    let configuration = entry.get("tool_configuration");
    if configuration.and_then(|c| c.get("enabled")) == Some(&Value::Bool(false)) {
        return Ok(None);
    }
    let allowed_tools = match configuration.and_then(|c| c.get("allowed_tools")) {
        None | Some(Value::Null) => None,
        Some(list) => Some(names(list, "tool_configuration.allowed_tools")?),
    };
    finish(url, headers, allowed_tools).map(Some)
}

fn finish(
    url: Url,
    headers: Vec<(String, String)>,
    allowed_tools: Option<Vec<String>>,
) -> Result<RequestedServer, String> {
    let name = name_of(&url).ok_or_else(|| format!("'{url}' names no host."))?;
    Ok(RequestedServer {
        name,
        url,
        headers,
        allowed_tools,
    })
}

/// The MCP servers `body` brings on `endpoint`.
///
/// # Errors
///
/// What is wrong, for a client to read: a process asked for, an address that
/// is not HTTP, tools of the client's own beside the servers, too many
/// servers, or two servers on one host.
pub(crate) fn read_servers(
    endpoint: Endpoint,
    body: &Value,
) -> Result<Vec<RequestedServer>, String> {
    let mut servers = Vec::new();
    match endpoint {
        Endpoint::Responses => {
            for tool in tools_of(body) {
                if kind_of(tool) == Some("mcp") {
                    servers.push(responses_server(tool)?);
                } else if asks_for_a_process(tool) {
                    return Err(NO_PROCESS.to_owned());
                } else {
                    return Err(
                        "A request that brings an MCP server is run by Apollia, tool calls \
                         included: it cannot bring other tools. Keep only the `mcp` tools."
                            .to_owned(),
                    );
                }
            }
        }
        Endpoint::Messages => {
            let Some(Value::Array(entries)) = body.get("mcp_servers") else {
                return Err("`mcp_servers` must be a list.".to_owned());
            };
            for entry in entries {
                if let Some(server) = messages_server(entry)? {
                    servers.push(server);
                }
            }
            if tools_of(body)
                .iter()
                .any(|tool| kind_of(tool) != Some("mcp_toolset"))
            {
                return Err(
                    "A request that brings an MCP server is run by Apollia, tool calls \
                     included: it cannot bring other tools. Remove `tools`."
                        .to_owned(),
                );
            }
        }
        _ => {}
    }
    if servers.is_empty() {
        return Err("Every MCP server of the request is turned off.".to_owned());
    }
    if servers.len() > MAX_SERVERS {
        return Err(format!(
            "A request may bring at most {MAX_SERVERS} MCP servers."
        ));
    }
    for (index, server) in servers.iter().enumerate() {
        if servers[..index].iter().any(|s| s.name == server.name) {
            return Err(format!(
                "Two MCP servers of the request are on the host of '{}'; a request brings one \
                 server per host.",
                server.url
            ));
        }
    }
    Ok(servers)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn responses(tool: Value) -> Value {
        json!({ "model": "acme/chat", "input": "hi", "tools": [tool] })
    }

    fn messages(server: Value) -> Value {
        json!({ "model": "acme/chat", "messages": [], "mcp_servers": [server] })
    }

    #[test]
    fn test_both_forms_name_the_same_server() {
        // GIVEN the `mcp` tool of a response and the `mcp_servers` of a
        // message, on one address, each with a token and a list of tools
        let response = responses(json!({
            "type": "mcp", "server_label": "anything", "server_url": "https://MCP.example.com/mcp",
            "authorization": "s3cret", "allowed_tools": ["echo"]
        }));
        let message = messages(json!({
            "type": "url", "name": "anything", "url": "https://mcp.example.com/mcp",
            "authorization_token": "s3cret",
            "tool_configuration": {"enabled": true, "allowed_tools": ["echo"]}
        }));

        // WHEN each is read
        let from_response = read_servers(Endpoint::Responses, &response).unwrap();
        let from_message = read_servers(Endpoint::Messages, &message).unwrap();

        // THEN they are one server, named after its host and never its label
        assert_eq!(from_response, from_message);
        let server = &from_response[0];
        assert_eq!(server.name, "request-mcp:mcp.example.com");
        assert_eq!(
            server.headers,
            [("Authorization".to_owned(), "Bearer s3cret".to_owned())]
        );
        assert_eq!(server.allowed_tools, Some(vec!["echo".to_owned()]));
    }

    #[test]
    fn test_only_the_two_public_forms_bring_a_server() {
        // GIVEN bodies with and without a server, on each endpoint
        let tool = json!({"type": "mcp", "server_url": "http://localhost:1/mcp"});
        let server = json!({"type": "url", "url": "http://localhost:1/mcp"});

        // WHEN each is checked
        // THEN the `mcp` tool counts on responses, `mcp_servers` on messages,
        // and nothing on a chat completion
        assert!(brings_mcp(Endpoint::Responses, &responses(tool.clone())));
        assert!(brings_mcp(Endpoint::Messages, &messages(server)));
        assert!(!brings_mcp(Endpoint::ChatCompletions, &responses(tool)));
        assert!(!brings_mcp(
            Endpoint::Responses,
            &responses(json!({"type": "function", "name": "f"}))
        ));
        assert!(!brings_mcp(
            Endpoint::Messages,
            &json!({"model": "m", "mcp_servers": []})
        ));
    }

    #[test]
    fn test_a_process_is_refused_in_every_shape() {
        // GIVEN servers that ask for a process: a command, a stdio type, a
        // stdio transport, an address that is not HTTP
        let asked = [
            (
                Endpoint::Responses,
                responses(json!({"type": "mcp", "command": "npx", "args": ["server"]})),
            ),
            (
                Endpoint::Responses,
                responses(json!({"type": "mcp", "server_url": "stdio://server"})),
            ),
            (
                Endpoint::Responses,
                responses(json!({"type": "mcp", "transport": "stdio", "server_url": "http://h/"})),
            ),
            (
                Endpoint::Messages,
                messages(json!({"type": "stdio", "command": "npx"})),
            ),
            (
                Endpoint::Messages,
                messages(json!({"type": "url", "url": "file:///usr/bin/server"})),
            ),
        ];

        for (endpoint, body) in asked {
            // WHEN it is read
            let refusal = read_servers(endpoint, &body).unwrap_err();

            // THEN it is refused, saying a request never starts a process
            assert!(refusal.contains("never starts a process"), "{refusal}");
        }
    }

    #[test]
    fn test_other_tools_reserved_headers_and_two_servers_on_a_host_are_refused() {
        // GIVEN a function tool beside the server, a header the transport
        // sets, credentials in the address, and two servers on one host
        let mcp = json!({"type": "mcp", "server_url": "http://localhost:1/mcp"});
        let with_function =
            json!({"model": "m", "tools": [mcp, {"type": "function", "name": "f"}]});
        let with_host = responses(json!({
            "type": "mcp", "server_url": "http://localhost:1/mcp", "headers": {"Host": "other"}
        }));
        let with_password =
            responses(json!({"type": "mcp", "server_url": "http://u:p@localhost:1/mcp"}));
        let twice = json!({"model": "m", "mcp_servers": [
            {"type": "url", "url": "http://localhost:1/mcp"},
            {"type": "url", "url": "http://localhost:2/mcp"}
        ]});

        // WHEN each is read
        // THEN each is refused for its own reason
        let refusal = |endpoint, body: &Value| read_servers(endpoint, body).unwrap_err();
        assert!(refusal(Endpoint::Responses, &with_function).contains("other tools"));
        assert!(refusal(Endpoint::Responses, &with_host).contains("set by Apollia"));
        assert!(refusal(Endpoint::Responses, &with_password).contains("password"));
        assert!(refusal(Endpoint::Messages, &twice).contains("one server per host"));
    }

    #[test]
    fn test_a_server_turned_off_is_left_out() {
        // GIVEN a message with one server turned off and one left on
        let body = json!({"model": "m", "mcp_servers": [
            {"type": "url", "url": "http://localhost:1/mcp",
             "tool_configuration": {"enabled": false}},
            {"type": "url", "url": "http://127.0.0.1:2/mcp"}
        ]});

        // WHEN it is read
        let servers = read_servers(Endpoint::Messages, &body).unwrap();

        // THEN only the second is kept, with every tool it has
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].name, "request-mcp:127.0.0.1");
        assert_eq!(servers[0].allowed_tools, None);
    }
}
