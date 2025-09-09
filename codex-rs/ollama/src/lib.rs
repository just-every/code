mod client;
mod parser;
mod pull;
mod url;

pub use client::OllamaClient;
use codex_core::config::Config;
pub use pull::CliProgressReporter;
pub use pull::PullEvent;
pub use pull::PullProgressReporter;
pub use pull::TuiProgressReporter;

/// Default OSS model to use when `--oss` is passed without an explicit `-m`.
pub const DEFAULT_OSS_MODEL: &str = "gpt-oss:20b";

/// Prepare the local OSS environment when `--oss` is selected.
///
/// - Ensures a local Ollama server is reachable.
/// - Checks if the model exists locally and pulls it if missing.
pub async fn ensure_oss_ready(config: &Config) -> std::io::Result<()> {
    // Only download when the requested model is the default OSS model (or when -m is not provided).
    let model = config.model.as_ref();

    // Verify local Ollama is reachable.
    let ollama_client = crate::OllamaClient::try_from_oss_provider(config).await?;

    // If the model is not present locally, pull it.
    match ollama_client.fetch_models().await {
        Ok(models) => {
            if !models.iter().any(|m| m == model) {
                let mut reporter = crate::CliProgressReporter::new();
                ollama_client
                    .pull_with_reporter(model, &mut reporter)
                    .await?;
            }
        }
        Err(err) => {
            // Not fatal; higher layers may still proceed and surface errors later.
            tracing::warn!("Failed to query local models from Ollama: {}.", err);
        }
    }

    // Attempt to detect the model's maximum context window and expose it as an
    // environment variable so downstream request builders can tune `num_ctx`.
    if let Ok(Some(ctx)) = ollama_client.fetch_model_max_context(model).await {
        // Avoid exporting obviously tiny defaults (some installs report 2048);
        // still set it so the client can override server defaults.
        // Mutating process env requires `unsafe` on Rust 2024; scoped to process.
        unsafe { std::env::set_var("CODEX_OLLAMA_NUM_CTX", ctx.to_string()); }
        tracing::info!("Detected Ollama model context length: {ctx}");
    }

    Ok(())
}
