use std::collections::HashMap;
use std::env;
use std::ffi::OsString;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use codex_mcp_client::McpClient;
use mcp_types::{ClientCapabilities, Implementation, InitializeRequestParams, MCP_SCHEMA_VERSION};
use serde_json::Value;
use tracing_subscriber::EnvFilter;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

#[tokio::main]
async fn main() -> Result<()> {
    init_logging();

    let CliArgs {
        tool_name,
        arguments,
        timeout,
        program,
        program_args,
        env,
    } = parse_args().context("failed to parse arguments")?;

    let client = McpClient::new_stdio_client(program, program_args, env)
        .await
        .context("failed to launch MCP server")?;

    initialize_client(&client, timeout).await?;

    let result = client
        .call_tool(tool_name, arguments, timeout)
        .await
        .context("tool call failed")?;

    println!("{}", serde_json::to_string_pretty(&result)?);

    Ok(())
}

fn init_logging() {
    let default_level = "warn";
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .or_else(|_| EnvFilter::try_new(default_level))
                .unwrap_or_else(|_| EnvFilter::new(default_level)),
        )
        .with_writer(std::io::stderr)
        .try_init();
}

async fn initialize_client(client: &McpClient, timeout: Option<Duration>) -> Result<()> {
    let params = InitializeRequestParams {
        capabilities: ClientCapabilities {
            experimental: None,
            roots: None,
            sampling: None,
            elicitation: None,
        },
        client_info: Implementation {
            name: "codex-mcp-client".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            title: Some("Codex".to_string()),
            user_agent: None,
        },
        protocol_version: MCP_SCHEMA_VERSION.to_string(),
    };

    client
        .initialize(params, None, timeout)
        .await
        .context("failed to initialize MCP session")?;

    Ok(())
}

struct CliArgs {
    tool_name: String,
    arguments: Option<Value>,
    timeout: Option<Duration>,
    program: OsString,
    program_args: Vec<OsString>,
    env: Option<HashMap<String, String>>,
}

fn parse_args() -> Result<CliArgs> {
    let args = env::args().skip(1).collect::<Vec<_>>();

    if args.is_empty() {
        return Err(anyhow!(usage()));
    }

    let mut tool_name: Option<String> = None;
    let mut arguments: Option<Value> = None;
    let mut timeout = Some(DEFAULT_TIMEOUT);
    let mut env_map: HashMap<String, String> = HashMap::new();

    let mut idx = 0;
    while idx < args.len() {
        match args[idx].as_str() {
            "--tool" => {
                idx += 1;
                tool_name = args.get(idx).cloned();
                if tool_name.is_none() {
                    return Err(anyhow!("--tool requires a value"));
                }
            }
            "--args" => {
                idx += 1;
                let raw = args
                    .get(idx)
                    .cloned()
                    .ok_or_else(|| anyhow!("--args requires JSON value"))?;
                let json: Value = serde_json::from_str(&raw)
                    .with_context(|| format!("failed to parse --args JSON: {raw}"))?;
                arguments = Some(json);
            }
            "--timeout" => {
                idx += 1;
                let raw = args
                    .get(idx)
                    .cloned()
                    .ok_or_else(|| anyhow!("--timeout requires integer seconds"))?;
                let secs: u64 = raw
                    .parse()
                    .with_context(|| format!("invalid timeout value: {raw}"))?;
                timeout = Some(Duration::from_secs(secs));
            }
            "--env" => {
                idx += 1;
                let raw = args
                    .get(idx)
                    .cloned()
                    .ok_or_else(|| anyhow!("--env requires KEY=VALUE"))?;
                let (key, value) = raw
                    .split_once('=')
                    .ok_or_else(|| anyhow!("--env must be in KEY=VALUE format"))?;
                env_map.insert(key.to_string(), value.to_string());
            }
            "--" => {
                idx += 1;
                break;
            }
            _ => return Err(anyhow!(usage())),
        }
        idx += 1;
    }

    if tool_name.is_none() {
        return Err(anyhow!("--tool is required"));
    }

    if idx >= args.len() {
        return Err(anyhow!("server command is required after `--`"));
    }

    let program = OsString::from(&args[idx]);
    let program_args = args[idx + 1..]
        .iter()
        .map(|s| OsString::from(s.as_str()))
        .collect::<Vec<_>>();

    Ok(CliArgs {
        tool_name: tool_name.unwrap(),
        arguments,
        timeout,
        program,
        program_args,
        env: if env_map.is_empty() {
            None
        } else {
            Some(env_map)
        },
    })
}

fn usage() -> String {
    "Usage: call_tool --tool <name> [--args '<json>'] [--timeout <secs>] [--env KEY=VALUE] -- <server> [args...]"
        .to_string()
}
