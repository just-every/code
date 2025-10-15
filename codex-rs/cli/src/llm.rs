use std::path::PathBuf;

use clap::Parser;
use codex_common::CliConfigOverrides;
use codex_core::AuthManager;
use codex_core::ModelClient;
use codex_core::ModelProviderInfo;
use codex_core::Prompt;
use codex_core::TextFormat;
use codex_core::config::Config;
use codex_core::config::ConfigOverrides;
use codex_protocol::mcp_protocol::AuthMode;
use codex_protocol::models::{ContentItem, ResponseItem};
use futures::StreamExt;

#[derive(Debug, Parser)]
pub struct LlmCli {
    #[clap(skip)]
    pub config_overrides: CliConfigOverrides,

    #[command(subcommand)]
    pub cmd: LlmSubcommand,
}

#[derive(Debug, clap::Subcommand)]
pub enum LlmSubcommand {
    /// Send a one-off structured request to the model (side-channel; no TUI events)
    Request(RequestArgs),
}

#[derive(Debug, Parser)]
pub struct RequestArgs {
    /// Developer message to prepend (kept separate from system instructions)
    #[arg(long)]
    pub developer: String,

    /// Primary user message/content
    #[arg(long)]
    pub message: String,

    /// `text.format.type` (e.g. json_schema)
    #[arg(long = "format-type", default_value = "json_schema")]
    pub format_type: String,

    /// Optional `text.format.name`
    #[arg(long = "format-name")]
    pub format_name: Option<String>,

    /// Set `text.format.strict`
    #[arg(long = "format-strict", default_value_t = true)]
    pub format_strict: bool,

    /// Inline JSON for the schema (mutually exclusive with --schema-file)
    #[arg(long = "schema-json")]
    pub schema_json: Option<String>,

    /// Path to a JSON schema file (mutually exclusive with --schema-json)
    #[arg(long = "schema-file")]
    pub schema_file: Option<PathBuf>,

    /// Optional model override (e.g. gpt-4.1, gpt-5)
    #[arg(long)]
    pub model: Option<String>,
}

pub async fn run_llm(opts: LlmCli) -> anyhow::Result<()> {
    match opts.cmd {
        LlmSubcommand::Request(req) => run_llm_request(opts.config_overrides, req).await,
    }
}

async fn run_llm_request(
    cli_overrides: CliConfigOverrides,
    args: RequestArgs,
) -> anyhow::Result<()> {
    let overrides_vec = cli_overrides
        .parse_overrides()
        .map_err(anyhow::Error::msg)?;

    let overrides = if let Some(model) = &args.model {
        ConfigOverrides {
            model: Some(model.clone()),
            ..ConfigOverrides::default()
        }
    } else {
        ConfigOverrides::default()
    };

    let config = Config::load_with_cli_overrides(overrides_vec, overrides)?;

    // Build Prompt with custom developer + user messages, no extra tools
    let mut input: Vec<ResponseItem> = Vec::new();
    input.push(ResponseItem::Message {
        id: None,
        role: "developer".to_string(),
        content: vec![ContentItem::InputText {
            text: args.developer.clone(),
        }],
    });
    input.push(ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputText {
            text: args.message.clone(),
        }],
    });

    // Resolve schema
    let schema_val: Option<serde_json::Value> = if let Some(s) = &args.schema_json {
        Some(serde_json::from_str::<serde_json::Value>(s)?)
    } else if let Some(p) = &args.schema_file {
        let data = std::fs::read_to_string(p)?;
        Some(serde_json::from_str::<serde_json::Value>(&data)?)
    } else {
        None
    };

    let text_format = TextFormat {
        r#type: args.format_type.clone(),
        name: args.format_name.clone(),
        strict: Some(args.format_strict),
        schema: schema_val,
    };

    let mut prompt = Prompt::default();
    prompt.input = input;
    prompt.store = true;
    prompt.user_instructions = None;
    prompt.status_items = vec![];
    prompt.base_instructions_override = None;
    prompt.text_format = Some(text_format);

    // Auth + provider
    let auth_mgr = AuthManager::shared(
        config.codex_home.clone(),
        AuthMode::ApiKey,
        config.responses_originator_header.clone(),
    );
    let provider: ModelProviderInfo = config.model_provider.clone();
    let client = ModelClient::new(
        std::sync::Arc::new(config.clone()),
        Some(auth_mgr),
        provider,
        config.model_reasoning_effort,
        config.model_reasoning_summary,
        config.model_text_verbosity,
        uuid::Uuid::new_v4(),
        std::sync::Arc::new(std::sync::Mutex::new(
            codex_core::debug_logger::DebugLogger::new(false)?,
        )),
    );

    // Collect the assistant message text from the stream (no TUI events)
    let mut stream = client.stream(&prompt).await?;
    let mut final_text: String = String::new();
    tracing::info!("LLM: created");
    while let Some(ev) = stream.next().await {
        let ev = ev?;
        match ev {
            codex_core::ResponseEvent::ReasoningSummaryDelta { delta, .. } => {
                tracing::info!(target: "llm", "thinking: {}", delta);
            }
            codex_core::ResponseEvent::ReasoningContentDelta { delta, .. } => {
                tracing::info!(target: "llm", "reasoning: {}", delta);
            }
            codex_core::ResponseEvent::OutputItemDone { item, .. } => {
                if let ResponseItem::Message { content, .. } = item {
                    for c in content {
                        if let ContentItem::OutputText { text } = c {
                            final_text.push_str(&text);
                        }
                    }
                }
            }
            codex_core::ResponseEvent::OutputTextDelta { delta, .. } => {
                tracing::info!(target: "llm", "delta: {}", delta);
                // For completeness, but we only print at the end to stay simple
                final_text.push_str(&delta);
            }
            codex_core::ResponseEvent::Completed { .. } => {
                tracing::info!("LLM: completed");
                break;
            }
            _ => {}
        }
    }

    println!("{}", final_text);
    Ok(())
}
