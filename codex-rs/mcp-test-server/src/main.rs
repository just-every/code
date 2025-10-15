use serde::{Deserialize, Serialize};
use serde_json::json;
use std::env;
use std::io::{self, BufRead, Write};
use std::thread;
use std::time::Duration;

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum Id {
    Int(i64),
    Str(String),
}

#[derive(Deserialize)]
struct JsonRpcReq {
    jsonrpc: String,
    method: String,
    #[allow(dead_code)]
    params: Option<serde_json::Value>,
    id: Option<Id>,
}

#[derive(Serialize)]
struct JsonRpcResp {
    jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<Id>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<serde_json::Value>,
}

fn sleep_env_ms(var: &str) {
    if let Ok(v) = env::var(var) {
        if let Ok(ms) = v.parse::<u64>() {
            thread::sleep(Duration::from_millis(ms));
        }
    }
}

fn main() {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    for line in stdin.lock().lines() {
        let line = match line { Ok(l) => l, Err(_) => break };
        let Ok(req) = serde_json::from_str::<JsonRpcReq>(&line) else {
            continue;
        };

        if req.jsonrpc != "2.0" {
            // Only protocol version 2.0 is supported; ignore anything else.
            continue;
        }

        match req.method.as_str() {
            "initialize" => {
                sleep_env_ms("SLOW_INIT_MS");
                let result = json!({
                    "capabilities": { "tools": { "listChanged": true } },
                    "serverInfo": { "name": "codex-mcp-test-server", "version": "0.0.1" },
                    "protocolVersion": "2025-06-18"
                });
                let resp = JsonRpcResp { jsonrpc: "2.0".into(), id: req.id, result: Some(result), error: None };
                let _ = writeln!(stdout, "{}", serde_json::to_string(&resp).unwrap());
                let _ = stdout.flush();
            }
            "notifications/initialized" => {
                // No-op
            }
            "tools/list" => {
                sleep_env_ms("SLOW_LIST_MS");
                let tools = json!({
                    "tools": [
                        {
                            "name": "echo",
                            "description": "Echo back provided text",
                            "inputSchema": { "type": "object", "properties": { "text": { "type": "string" } }, "required": ["text"] }
                        }
                    ]
                });
                let resp = JsonRpcResp { jsonrpc: "2.0".into(), id: req.id, result: Some(tools), error: None };
                let _ = writeln!(stdout, "{}", serde_json::to_string(&resp).unwrap());
                let _ = stdout.flush();
            }
            _ => {
                // Unknown method -> echo minimal error structure
                let err = json!({ "code": -32601, "message": "Method not found" });
                let resp = JsonRpcResp { jsonrpc: "2.0".into(), id: req.id, result: None, error: Some(err) };
                let _ = writeln!(stdout, "{}", serde_json::to_string(&resp).unwrap());
                let _ = stdout.flush();
            }
        }
    }
}
