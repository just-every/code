//! New-session configuration from server defaults and explicit launch settings.
//!
//! Build the replacement configuration without replacing the active task's settings.

use super::*;
use codex_config::ConfigLayerSource;

pub(super) async fn read_new_session_defaults(
    app_server: &AppServerSession,
    cwd: &Path,
) -> Result<Option<codex_app_server_protocol::Config>> {
    // config/read resolves relative paths on the server. With no remote launch override,
    // "." uses the same server process directory as thread/start's omitted cwd.
    match crate::config_update::read_effective_config(
        app_server.request_handle(),
        cwd.display().to_string(),
    )
    .await
    {
        Ok(response) => Ok(Some(response.config)),
        Err(err)
            if matches!(
                err.downcast_ref::<TypedRequestError>(),
                Some(TypedRequestError::Server { source, .. })
                    if source.code == -32601
                        || source.code == -32600
                            && source.message.contains("config/read")
                            && (source.message.contains("unknown variant")
                                || source.message.contains("unknown method"))
            ) =>
        {
            // Older servers can still start threads using the existing local defaults.
            Ok(None)
        }
        Err(err) => Err(err),
    }
}

pub(super) fn has_launch_setting(
    config: &Config,
    cli_kv_overrides: &[(String, TomlValue)],
    key: &str,
) -> bool {
    // A remote server cannot resolve this invocation's explicitly selected local profile.
    cli_kv_overrides.iter().any(|(path, _)| path == key)
        || config.config_layer_stack.layers_high_to_low().any(|layer| {
            layer.disabled_reason.is_none()
                && matches!(
                    layer.name,
                    ConfigLayerSource::User {
                        profile: Some(_),
                        ..
                    }
                )
                && layer.config.get(key).is_some()
        })
}

pub(super) fn overlay_new_session_defaults(
    config: &mut Config,
    defaults: &codex_app_server_protocol::Config,
    cli_kv_overrides: &[(String, TomlValue)],
    harness_overrides: &ConfigOverrides,
) {
    if harness_overrides.model.is_none() && !has_launch_setting(config, cli_kv_overrides, "model") {
        config.model = defaults.model.clone();
    }
    if !has_launch_setting(config, cli_kv_overrides, "model_reasoning_effort") {
        config.model_reasoning_effort = defaults.model_reasoning_effort.clone();
    }
}

impl App {
    pub(super) async fn load_new_session_config(
        &mut self,
        app_server: &AppServerSession,
    ) -> Result<Config> {
        let cwd = self.chat_widget.config_ref().cwd.to_path_buf();
        let defaults_cwd = match app_server.thread_params_mode() {
            crate::app_server_session::ThreadParamsMode::Embedded => cwd.as_path(),
            crate::app_server_session::ThreadParamsMode::Remote => {
                app_server.remote_cwd_override().unwrap_or(Path::new("."))
            }
        };
        let defaults = read_new_session_defaults(app_server, defaults_cwd).await?;
        // Stage local preferences and permission carryover without changing the active task.
        let mut config = match self.rebuild_config_for_cwd(cwd).await {
            Ok(config) => config,
            Err(err) => {
                tracing::warn!(%err, "failed to refresh local settings before a new thread");
                self.config.clone()
            }
        };
        self.apply_runtime_policy_overrides(&mut config, RuntimePolicyOverrideScope::All);
        config.service_tier = self.chat_widget.configured_service_tier();
        if let Some(defaults) = defaults.as_ref() {
            overlay_new_session_defaults(
                &mut config,
                defaults,
                &self.cli_kv_overrides,
                &self.harness_overrides,
            );
        }
        Ok(config)
    }
}
