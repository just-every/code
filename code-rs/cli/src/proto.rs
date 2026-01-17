use std::io::IsTerminal;

use clap::Parser;
use code_common::CliConfigOverrides;
use code_core::AuthManager;
use code_core::ConversationManager;
use code_core::NewConversation;
use code_core::config::Config;
use code_core::config::ConfigOverrides;
use code_core::protocol::Event;
use code_core::protocol::EventMsg;
use code_core::protocol::Submission;
use code_protocol::protocol::SessionSource;
use tokio::io::AsyncBufReadExt;
use tokio::io::BufReader;
use tracing::error;
use tracing::info;

#[derive(Debug, Parser)]
pub struct ProtoCli {
    #[clap(skip)]
    pub config_overrides: CliConfigOverrides,
}

pub async fn run_main(opts: ProtoCli) -> anyhow::Result<()> {
    if std::io::stdin().is_terminal() {
        anyhow::bail!("Protocol mode expects stdin to be a pipe, not a terminal");
    }

    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .init();

    let ProtoCli { config_overrides } = opts;
    let overrides_vec = config_overrides
        .parse_overrides()
        .map_err(anyhow::Error::msg)?;

    let config = Config::load_with_cli_overrides(overrides_vec, ConfigOverrides::default())?;
    // Use conversation_manager API to start a conversation
    let auth_manager = AuthManager::shared_with_mode_and_originator(
        config.code_home.clone(),
        config.preferred_auth_mode(),
        config.responses_originator_header.clone(),
    );
    let conversation_manager = ConversationManager::new(auth_manager.clone(), SessionSource::Cli);
    let NewConversation {
        conversation_id: _,
        conversation,
        session_configured,
    } = conversation_manager
        .new_conversation(config.clone())
        .await?;

    // Simulate streaming the session_configured event.
    let synthetic_event = Event {
        // Fake id value.
        id: "".to_string(),
        event_seq: 0,
        msg: EventMsg::SessionConfigured(session_configured),
        order: None,
    };
    let session_configured_event = match serde_json::to_string(&synthetic_event) {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to serialize session_configured: {e}");
            return Err(anyhow::Error::from(e));
        }
    };
    println!("{session_configured_event}");

    // Task that reads JSON lines from stdin and forwards to Submission Queue
    let sq_fut = {
        let conversation = conversation.clone();
        async move {
            let stdin = BufReader::new(tokio::io::stdin());
            let mut lines = stdin.lines();
            loop {
                let result = tokio::select! {
                    _ = tokio::signal::ctrl_c() => {
                        break
                    },
                    res = lines.next_line() => res,
                };

                match result {
                    Ok(Some(line)) => {
                        let line = line.trim();
                        if line.is_empty() {
                            continue;
                        }
                        match serde_json::from_str::<Submission>(line) {
                            Ok(sub) => {
                                if let Err(e) = conversation.submit_with_id(sub).await {
                                    error!("{e:#}");
                                    break;
                                }
                            }
                            Err(e) => {
                                error!("invalid submission: {e}");
                            }
                        }
                    }
                    _ => {
                        info!("Submission queue closed");
                        break;
                    }
                }
            }
        }
    };

    // Task that reads events from the agent and prints them as JSON lines to stdout
    let eq_fut = async move {
        loop {
            let event = tokio::select! {
                _ = tokio::signal::ctrl_c() => break,
                event = conversation.next_event() => event,
            };
            match event {
                Ok(event) => {
                    let event_str = match serde_json::to_string(&event) {
                        Ok(s) => s,
                        Err(e) => {
                            error!("Failed to serialize event: {e}");
                            continue;
                        }
                    };
                    println!("{event_str}");
                }
                Err(e) => {
                    error!("{e:#}");
                    break;
                }
            }
        }
        info!("Event queue closed");
    };

    tokio::join!(sq_fut, eq_fut);
    Ok(())
}
