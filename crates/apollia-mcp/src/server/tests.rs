use std::sync::Mutex;

use serde_json::json;

use super::*;

/// A daemon that serves one exposed agent, or refuses the bridge.
struct FakeDaemon {
    enabled: bool,
    calls: Mutex<Vec<(String, serde_json::Value)>>,
}

impl FakeDaemon {
    fn new(enabled: bool) -> Arc<Self> {
        Arc::new(Self {
            enabled,
            calls: Mutex::new(Vec::new()),
        })
    }

    fn refusal() -> BridgeError {
        BridgeError::Refused("the MCP bridge is turned off".into())
    }
}

impl McpBridge for FakeDaemon {
    fn list_tools(&self) -> BridgeFuture<'_, Result<Vec<serde_json::Value>, BridgeError>> {
        Box::pin(async move {
            if !self.enabled {
                return Err(Self::refusal());
            }
            Ok(vec![json!({
                "name": "agent__mailer",
                "description": "Writes mail",
                "inputSchema": { "type": "object" },
            })])
        })
    }

    fn call_tool(
        &self,
        name: String,
        arguments: serde_json::Value,
    ) -> BridgeFuture<'_, Result<BridgeCallOutcome, BridgeError>> {
        Box::pin(async move {
            if !self.enabled {
                return Err(Self::refusal());
            }
            self.calls
                .lock()
                .unwrap()
                .push((name.clone(), arguments.clone()));
            match name.as_str() {
                "agent__mailer" => Ok(BridgeCallOutcome {
                    text: format!("done: {}", arguments["message"]),
                    is_error: false,
                }),
                "agent__broken" => Ok(BridgeCallOutcome {
                    text: "the model failed".into(),
                    is_error: true,
                }),
                other => Err(BridgeError::UnknownTool(format!(
                    "no exposed item is named '{other}'"
                ))),
            }
        })
    }
}

#[tokio::test]
async fn test_initialize_names_the_server() {
    // GIVEN a bridge over a daemon
    let server = McpStdioServer::new(FakeDaemon::new(true));

    // WHEN the client sends the handshake
    let response = server
        .handle_line(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","clientInfo":{"name":"t"}}}"#,
        )
        .await
        .unwrap();

    // THEN the server answers with its name and the tools capability
    assert_eq!(response["result"]["protocolVersion"], "2024-11-05");
    assert_eq!(response["result"]["serverInfo"]["name"], "apollia-os");
    assert!(response["result"]["capabilities"]["tools"].is_object());
}

#[tokio::test]
async fn test_tools_list_relays_the_exposed_items_and_no_native_tool() {
    // GIVEN a daemon exposing one agent
    let server = McpStdioServer::new(FakeDaemon::new(true));

    // WHEN the client lists the tools
    let response = server
        .handle_line(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#)
        .await
        .unwrap();

    // THEN it receives that agent and nothing else
    let tools = response["result"]["tools"].as_array().unwrap();
    let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
    assert_eq!(names, vec!["agent__mailer"]);
}

#[tokio::test]
async fn test_tools_call_returns_the_final_result_of_the_run() {
    // GIVEN a daemon exposing one agent
    let daemon = FakeDaemon::new(true);
    let server = McpStdioServer::new(daemon.clone());

    // WHEN the client calls it with a message
    let response = server
        .handle_line(
            r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"agent__mailer","arguments":{"message":"hello"}}}"#,
        )
        .await
        .unwrap();

    // THEN the daemon ran it on that message and the result is relayed
    assert_eq!(response["result"]["content"][0]["type"], "text");
    assert_eq!(response["result"]["content"][0]["text"], "done: \"hello\"");
    assert_eq!(response["result"]["isError"], false);
    let calls = daemon.calls.lock().unwrap();
    assert_eq!(
        *calls,
        vec![("agent__mailer".to_string(), json!({ "message": "hello" }))]
    );
}

#[tokio::test]
async fn test_a_failed_run_is_a_tool_result_marked_as_an_error() {
    // GIVEN a daemon whose agent fails
    let server = McpStdioServer::new(FakeDaemon::new(true));

    // WHEN the client calls it
    let response = server
        .handle_line(
            r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"agent__broken","arguments":{"message":"x"}}}"#,
        )
        .await
        .unwrap();

    // THEN the failure comes back as a result with isError, not a protocol error
    assert!(response.get("error").is_none());
    assert_eq!(response["result"]["isError"], true);
    assert_eq!(response["result"]["content"][0]["text"], "the model failed");
}

#[tokio::test]
async fn test_a_native_tool_name_is_unknown_to_the_bridge() {
    // GIVEN a daemon exposing one agent
    let server = McpStdioServer::new(FakeDaemon::new(true));

    // WHEN the client calls a native tool the old server used to expose
    let response = server
        .handle_line(
            r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"bash_executor","arguments":{"command":"id"}}}"#,
        )
        .await
        .unwrap();

    // THEN it is refused as an unknown tool
    assert_eq!(response["error"]["code"], -32602);
    assert!(response["error"]["message"]
        .as_str()
        .unwrap()
        .contains("bash_executor"));
}

#[tokio::test]
async fn test_a_daemon_that_refuses_the_bridge_is_reported_readably() {
    // GIVEN a daemon where the bridge is turned off
    let server = McpStdioServer::new(FakeDaemon::new(false));

    // WHEN the client lists the tools, then calls one
    let listed = server
        .handle_line(r#"{"jsonrpc":"2.0","id":6,"method":"tools/list"}"#)
        .await
        .unwrap();
    let called = server
        .handle_line(
            r#"{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"agent__mailer"}}"#,
        )
        .await
        .unwrap();

    // THEN both answer the daemon's refusal
    for response in [listed, called] {
        assert_eq!(response["error"]["code"], -32000);
        assert_eq!(response["error"]["message"], "the MCP bridge is turned off");
    }
}

#[tokio::test]
async fn test_malformed_json_and_notifications() {
    // GIVEN a bridge
    let server = McpStdioServer::new(FakeDaemon::new(true));

    // WHEN the client sends a broken line, a notification and a blank line
    let broken = server.handle_line("{not json").await;
    let notification = server
        .handle_line(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#)
        .await;
    let blank = server.handle_line("   ").await;

    // THEN only the broken line is answered, with a parse error
    assert_eq!(broken.unwrap()["error"]["code"], -32700);
    assert!(notification.is_none());
    assert!(blank.is_none());
}
