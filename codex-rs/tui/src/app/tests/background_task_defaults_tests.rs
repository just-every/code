//! Request-level coverage for background-task server defaults and dispatch recovery.

use super::*;
use crate::app::agents_overview::AGENTS_OVERVIEW_VIEW_ID;
use crate::app::tests::session_lifecycle_requests::HistoryCapabilities;
use crate::app::tests::session_lifecycle_requests::recorded_params;
use crate::app::tests::session_lifecycle_requests::start_recording_app_server_with_history;
use crate::test_support::PathBufExt;
use codex_state::SqliteConfig;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn background_task_reads_server_defaults_for_actual_destination() -> Result<()> {
    for (mode, explicit_cwd, launch_override, expected_cwd, expected_model) in [
        ("local", false, false, "launch", "server-model"),
        ("local", true, false, "destination", "destination-model"),
        ("local-cli-provider", false, false, "launch", "server-model"),
        ("local-cli-model", false, false, "launch", "cli-model"),
        (
            "local-default-provider",
            false,
            false,
            "launch",
            "server-model",
        ),
        ("remote", true, true, "destination", "destination-model"),
        ("remote", false, true, "launch", "server-model"),
        ("remote", false, false, ".", "server-model"),
        ("remote-null-fast", false, false, ".", ""),
    ] {
        let client_home = tempdir()?;
        let server_home = tempdir()?;
        let launch = tempdir()?;
        let destination = tempdir()?;
        std::fs::write(
            client_home.path().join("config.toml"),
            format!(
                "model = \"{}\"\nmodel_reasoning_effort = \"low\"\n{}",
                if mode == "remote-null-fast" {
                    "gpt-5.2"
                } else {
                    "client-model"
                },
                if mode == "local-default-provider" {
                    "model_provider = \"ollama\"\n"
                } else {
                    ""
                }
            ),
        )?;
        std::fs::write(
            server_home.path().join("config.toml"),
            format!(
                "{}model_reasoning_effort = \"high\"\n{}",
                if mode == "remote-null-fast" {
                    ""
                } else {
                    "model = \"server-model\"\n"
                },
                if mode == "local" || mode == "local-cli-provider" || mode == "local-cli-model" {
                    "model_provider = \"ollama\"\n"
                } else {
                    ""
                }
            ),
        )?;
        std::fs::create_dir(destination.path().join(".codex"))?;
        std::fs::write(
            destination.path().join(".codex/config.toml"),
            "model = \"destination-model\"\nservice_tier = \"flex\"\n",
        )?;
        for home in [client_home.path(), server_home.path()] {
            crate::legacy_core::config::set_project_trust_level(
                home,
                destination.path(),
                codex_protocol::config_types::TrustLevel::Trusted,
            )
            .map_err(|error| color_eyre::eyre::eyre!(error.to_string()))?;
        }
        let mut app = make_test_app_with_channels().await.0;
        if mode == "local" && explicit_cwd {
            app.harness_overrides.model_provider = Some("openai".into());
        }
        if mode == "local-cli-provider" {
            app.cli_kv_overrides
                .push(("model_provider".into(), TomlValue::String("openai".into())));
        }
        if mode == "local-cli-model" {
            app.harness_overrides.model = Some("cli-model".into());
        }
        app.chat_widget.set_service_tier(Some("priority".into()));
        app.harness_overrides.cwd = Some(launch.path().to_path_buf());
        app.config = ConfigBuilder::default()
            .codex_home(client_home.path().to_path_buf())
            .loader_overrides(app.loader_overrides.clone())
            .cli_overrides(app.cli_kv_overrides.clone())
            .harness_overrides(app.harness_overrides.clone())
            .build()
            .await?;
        if mode == "remote-null-fast" {
            app.config.features.enable(Feature::FastMode)?;
        }
        app.chat_widget
            .handle_thread_session_quiet(test_thread_session(
                ThreadId::new(),
                launch.path().to_path_buf(),
            ));
        let mut server_config = app.config.clone();
        server_config.codex_home = server_home.path().to_path_buf().abs();
        server_config.sqlite = SqliteConfig::new_for_testing(server_home.path().abs());
        let thread_mode = if mode.starts_with("remote") {
            crate::app_server_session::ThreadParamsMode::Remote
        } else {
            crate::app_server_session::ThreadParamsMode::Embedded
        };
        let (mut server, requests, proxy) = start_recording_app_server_with_history(
            &server_config,
            HistoryCapabilities::Current,
            /*blocked_thread_list*/ None,
            /*failed_thread_name*/ None,
            thread_mode,
            LoaderOverrides {
                user_config_path: Some(server_home.path().join("config.toml").abs()),
                ..LoaderOverrides::default()
            },
        )
        .await?;
        if launch_override {
            server = server.with_remote_cwd_override(Some(launch.path().to_path_buf()));
        }
        let bootstrap = server.bootstrap(&app.config).await?;
        let expected_model = if mode == "remote-null-fast" {
            let default_model = bootstrap
                .available_models
                .iter()
                .find(|model| model.is_default)
                .or_else(|| bootstrap.available_models.first())
                .expect("server catalog model")
                .model
                .clone();
            app.model_catalog = Arc::new(ModelCatalog::new(bootstrap.available_models));
            default_model
        } else {
            expected_model.to_string()
        };
        app.dispatch_agents_overview_task(
            &mut server,
            "background prompt".into(),
            explicit_cwd.then(|| destination.path().to_path_buf().abs()),
        )
        .await;
        let cwd = match expected_cwd {
            "launch" => launch.path().display().to_string(),
            "destination" => destination.path().display().to_string(),
            "." => ".".to_string(),
            _ => unreachable!(),
        };
        assert_eq!(
            recorded_params(&requests, "config/read"),
            vec![serde_json::json!({"cwd": cwd})],
            "{mode} {expected_cwd}"
        );
        let starts = recorded_params(&requests, "thread/start");
        assert_eq!(starts.len(), 1, "{mode} {expected_cwd}");
        assert_eq!(
            (
                &starts[0]["cwd"],
                &starts[0]["model"],
                &starts[0]["modelProvider"],
                &starts[0]["config"]["model_reasoning_effort"],
                &starts[0]["serviceTier"],
            ),
            (
                &if mode.starts_with("remote") && !explicit_cwd && !launch_override {
                    serde_json::Value::Null
                } else {
                    serde_json::json!(cwd)
                },
                &serde_json::json!(expected_model),
                &if mode.starts_with("remote") {
                    serde_json::Value::Null
                } else if mode == "local" && !explicit_cwd {
                    serde_json::json!("ollama")
                } else {
                    serde_json::json!("openai")
                },
                &serde_json::json!("high"),
                &serde_json::json!(if mode == "local" && explicit_cwd {
                    "flex"
                } else {
                    "priority"
                }),
            ),
            "{mode} {expected_cwd}"
        );
        assert_eq!(recorded_params(&requests, "turn/start").len(), 1);
        server.shutdown().await?;
        proxy.await??;
    }
    Ok(())
}

#[tokio::test]
async fn background_task_preserves_explicit_choices_and_managed_defaults() -> Result<()> {
    for (choice, expected_model, expected_effort) in [
        ("saved", "server-model", "high"),
        ("cli_effort", "server-model", "low"),
        ("profile_model", "profile-model", "high"),
        ("managed", "managed-model", "medium"),
    ] {
        let client_home = tempdir()?;
        let server_home = tempdir()?;
        std::fs::write(
            client_home.path().join("config.toml"),
            "model = \"client-model\"\nmodel_reasoning_effort = \"low\"\n",
        )?;
        std::fs::write(
            server_home.path().join("config.toml"),
            "model = \"server-model\"\nmodel_reasoning_effort = \"high\"\n",
        )?;
        if choice == "managed" || choice.starts_with("cli_") {
            std::fs::write(
                server_home.path().join("requirements.toml"),
                "[models.new_thread]\nmodel = \"managed-model\"\nmodel_reasoning_effort = \"medium\"\n",
            )?;
        }
        let mut app = make_test_app_with_channels().await.0;
        match choice {
            "cli_effort" => app.cli_kv_overrides.push((
                "model_reasoning_effort".into(),
                TomlValue::String("low".into()),
            )),
            "profile_model" => {
                let path = client_home.path().join("work.config.toml");
                std::fs::write(&path, "model = \"profile-model\"\n")?;
                app.loader_overrides.user_config_path = Some(path.abs());
                app.loader_overrides.user_config_profile = Some("work".parse()?);
            }
            _ => {}
        }
        app.config = ConfigBuilder::default()
            .codex_home(client_home.path().to_path_buf())
            .loader_overrides(app.loader_overrides.clone())
            .cli_overrides(app.cli_kv_overrides.clone())
            .harness_overrides(app.harness_overrides.clone())
            .build()
            .await?;
        let mut server_config = app.config.clone();
        server_config.codex_home = server_home.path().to_path_buf().abs();
        server_config.sqlite = SqliteConfig::new_for_testing(server_home.path().abs());
        let (mut server, requests, proxy) = start_recording_app_server_with_history(
            &server_config,
            HistoryCapabilities::Current,
            /*blocked_thread_list*/ None,
            /*failed_thread_name*/ None,
            crate::app_server_session::ThreadParamsMode::Remote,
            LoaderOverrides {
                user_config_path: Some(server_home.path().join("config.toml").abs()),
                system_requirements_path: Some(server_home.path().join("requirements.toml")),
                ..LoaderOverrides::default()
            },
        )
        .await?;
        server.bootstrap(&app.config).await?;
        app.dispatch_agents_overview_task(
            &mut server,
            "background prompt".into(),
            /*cwd*/ None,
        )
        .await;
        let starts = recorded_params(&requests, "thread/start");
        assert_eq!(starts.len(), 1, "{choice}");
        assert_eq!(
            (
                &starts[0]["model"],
                &starts[0]["config"]["model_reasoning_effort"]
            ),
            (
                &serde_json::json!(expected_model),
                &serde_json::json!(expected_effort)
            ),
            "{choice}"
        );
        assert_eq!(
            recorded_params(&requests, "turn/start").len(),
            1,
            "{choice}"
        );
        server.shutdown().await?;
        proxy.await??;
    }
    Ok(())
}

#[tokio::test]
async fn background_task_read_failure_keeps_prompt_and_does_not_start() -> Result<()> {
    for capability in [
        HistoryCapabilities::ConfigReadFails,
        HistoryCapabilities::ConfigReadUnsupported(-32600),
        HistoryCapabilities::ConfigReadUnsupported(-32601),
    ] {
        let (mut app, mut events, _) = make_test_app_with_channels().await;
        app.harness_overrides.model = Some("local-model".into());
        let view = app.agents_overview_view(Vec::new(), /*selected_thread_id*/ None);
        app.chat_widget.show_bottom_pane_view(Box::new(view));
        let (mut server, requests, proxy) = start_recording_app_server_with_history(
            &app.config,
            capability,
            /*blocked_thread_list*/ None,
            /*failed_thread_name*/ None,
            crate::app_server_session::ThreadParamsMode::Embedded,
            LoaderOverrides::default(),
        )
        .await?;
        app.dispatch_agents_overview_task(
            &mut server,
            "retry background prompt".into(),
            /*cwd*/ None,
        )
        .await;
        let failed = capability == HistoryCapabilities::ConfigReadFails;
        assert_eq!(recorded_params(&requests, "config/read").len(), 1);
        assert_eq!(
            recorded_params(&requests, "thread/start").len(),
            usize::from(!failed)
        );
        assert_eq!(
            recorded_params(&requests, "turn/start").len(),
            usize::from(!failed)
        );
        if failed {
            assert!(app.agents_overview.dispatched_requests.is_empty());
            assert_eq!(
                app.agents_overview
                    .view_state
                    .lock()
                    .unwrap()
                    .composer
                    .as_ref()
                    .unwrap()
                    .current_text_with_pending(),
                "retry background prompt"
            );
            assert!(
                app.chat_widget
                    .selected_index_for_present_view(AGENTS_OVERVIEW_VIEW_ID)
                    .is_some()
            );
            let error = std::iter::from_fn(|| events.try_recv().ok())
                .filter_map(|event| match event {
                    AppEvent::InsertHistoryCell(cell) => {
                        Some(lines_to_single_string(&cell.display_lines(/*width*/ 80)))
                    }
                    _ => None,
                })
                .find(|message| message.contains("Failed to load background task settings"))
                .expect("visible read error");
            insta::assert_snapshot!(error, @"■ Failed to load background task settings: config/read failed in TUI");
        } else {
            assert_eq!(
                recorded_params(&requests, "thread/start")[0]["model"],
                "local-model"
            );
        }
        server.shutdown().await?;
        proxy.await??;
    }
    Ok(())
}
