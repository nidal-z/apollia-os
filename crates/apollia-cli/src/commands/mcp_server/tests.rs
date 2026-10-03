//! The HTTP bridge against a fake daemon listening on a Unix socket.

use std::sync::{Arc, Mutex};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;

use super::*;

/// What the fake daemon answers on each path, and the bodies it was sent.
struct FakeDaemon {
    answers: Vec<(&'static str, u16, serde_json::Value)>,
    received: Mutex<Vec<(String, String)>>,
}

/// Reads one HTTP/1.1 request: its first line and its body.
async fn read_request(stream: &mut tokio::net::UnixStream) -> (String, String) {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let n = stream.read(&mut chunk).await.unwrap();
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
        let text = String::from_utf8_lossy(&buf).to_string();
        if let Some(end) = text.find("\r\n\r\n") {
            let length = text[..end]
                .lines()
                .find_map(|l| {
                    let lower = l.to_ascii_lowercase();
                    lower
                        .strip_prefix("content-length:")
                        .map(|v| v.trim().parse::<usize>().unwrap())
                })
                .unwrap_or(0);
            if buf.len() >= end + 4 + length {
                let first = text.lines().next().unwrap_or_default().to_string();
                let body = String::from_utf8_lossy(&buf[end + 4..end + 4 + length]).to_string();
                return (first, body);
            }
        }
    }
    (String::new(), String::new())
}

/// Starts the fake daemon on a socket under `dir`.
fn serve(dir: &std::path::Path, daemon: Arc<FakeDaemon>) -> PathBuf {
    let socket = dir.join("runtime.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            let daemon = daemon.clone();
            tokio::spawn(async move {
                let (first, body) = read_request(&mut stream).await;
                daemon.received.lock().unwrap().push((first.clone(), body));
                let (status, answer) = daemon
                    .answers
                    .iter()
                    .find(|(path, _, _)| first.contains(path))
                    .map(|(_, status, body)| (*status, body.to_string()))
                    .unwrap_or((404, r#"{"error":"no route"}"#.to_string()));
                let response = format!(
                    "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{answer}",
                    answer.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
                stream.shutdown().await.ok();
            });
        }
    });
    socket
}

fn daemon(answers: Vec<(&'static str, u16, serde_json::Value)>) -> Arc<FakeDaemon> {
    Arc::new(FakeDaemon {
        answers,
        received: Mutex::new(Vec::new()),
    })
}

#[tokio::test]
async fn test_the_bridge_lists_what_the_daemon_serves() {
    // GIVEN a daemon serving one exposed agent
    let dir = tempfile::tempdir().unwrap();
    let tool = serde_json::json!({
        "name": "agent__mailer",
        "description": "Writes mail",
        "inputSchema": {"type": "object"}
    });
    let socket = serve(
        dir.path(),
        daemon(vec![(
            "GET /api/v1/mcp/bridge/tools",
            200,
            serde_json::json!({ "tools": [tool.clone()] }),
        )]),
    );

    // WHEN the bridge lists the tools
    let tools = HttpBridge::new(socket).list_tools().await.unwrap();

    // THEN it relays them as the daemon wrote them
    assert_eq!(tools, vec![tool]);
}

#[tokio::test]
async fn test_the_bridge_sends_the_call_and_relays_the_final_result() {
    // GIVEN a daemon that runs the agent to its end
    let dir = tempfile::tempdir().unwrap();
    let fake = daemon(vec![(
        "POST /api/v1/mcp/bridge/call",
        200,
        serde_json::json!({
            "text": "Sent.", "isError": false, "status": "completed",
            "kind": "agent", "itemId": "mailer"
        }),
    )]);
    let socket = serve(dir.path(), fake.clone());

    // WHEN the bridge calls the agent with a message
    let outcome = HttpBridge::new(socket)
        .call_tool(
            "agent__mailer".into(),
            serde_json::json!({"message": "Write to Ana"}),
        )
        .await
        .unwrap();

    // THEN the daemon received the name and the arguments, and the answer
    //   comes back as the final result
    assert_eq!(
        outcome,
        BridgeCallOutcome {
            text: "Sent.".into(),
            is_error: false
        }
    );
    let received = fake.received.lock().unwrap();
    let sent: serde_json::Value = serde_json::from_str(&received[0].1).unwrap();
    assert_eq!(
        sent,
        serde_json::json!({"name": "agent__mailer", "arguments": {"message": "Write to Ana"}})
    );
}

#[tokio::test]
async fn test_a_daemon_that_refuses_the_bridge_stops_it_with_its_reason() {
    // GIVEN a daemon where the bridge is turned off
    let dir = tempfile::tempdir().unwrap();
    let socket = serve(
        dir.path(),
        daemon(vec![(
            "GET /api/v1/mcp/bridge/tools",
            403,
            serde_json::json!({
                "error": "the MCP bridge is turned off; turn it on with `apollia-os config set mcp.server_enabled true`",
                "code": "bridge_disabled"
            }),
        )]),
    );

    // WHEN the bridge starts
    let result = run(socket).await;

    // THEN it stops before serving, with the daemon's readable reason
    let Err(McpServerCommandError::Bridge(BridgeError::Refused(reason))) = result else {
        panic!("expected a refusal, got {result:?}");
    };
    assert!(reason.contains("mcp.server_enabled"), "{reason}");
}

#[tokio::test]
async fn test_a_stopped_daemon_is_named_with_the_command_that_starts_it() {
    // GIVEN no daemon on the socket
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("absent.sock");

    // WHEN the bridge starts
    let result = run(socket).await;

    // THEN it says the daemon is not running and how to start it
    let Err(McpServerCommandError::Bridge(BridgeError::Unavailable(reason))) = result else {
        panic!("expected the daemon to be unavailable, got {result:?}");
    };
    assert!(reason.contains("apollia-os start"), "{reason}");
}

#[tokio::test]
async fn test_an_unknown_tool_and_bad_arguments_keep_their_meaning() {
    // GIVEN a daemon answering 404 then 400 on calls
    let dir = tempfile::tempdir().unwrap();
    let socket = serve(
        dir.path(),
        daemon(vec![(
            "POST /api/v1/mcp/bridge/call",
            404,
            serde_json::json!({"error": "no exposed agent or workflow is named 'bash_executor'", "code": "unknown_tool"}),
        )]),
    );
    let bridge = HttpBridge::new(socket);

    // WHEN a native tool name is called
    let unknown = bridge
        .call_tool("bash_executor".into(), serde_json::json!({}))
        .await;

    // THEN it reads as an unknown tool
    assert!(
        matches!(unknown, Err(BridgeError::UnknownTool(ref m)) if m.contains("bash_executor")),
        "{unknown:?}"
    );
    assert!(matches!(
        bridge_error(ClientError::ServerError {
            status: 400,
            body: "x".into()
        }),
        BridgeError::InvalidArguments(_)
    ));
}
