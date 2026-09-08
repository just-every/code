use super::CapturedSnapshot;
use super::PreparedSnapshot;
use super::SnapshotCaptureOptions;
use super::SnapshotCredentialEnvironment;
use super::SnapshotStartup;
use super::prepare_snapshot_credentials;
use super::snapshot_capture_script;
use crate::shell_detect::ShellType;
use anyhow::Context;
use anyhow::Result;
use pretty_assertions::assert_eq;
use std::collections::HashMap;
use std::process::Command;
use tempfile::tempdir;

const CAPTURE_ALL: SnapshotCaptureOptions = SnapshotCaptureOptions {
    startup: SnapshotStartup::Interactive,
    declarations: true,
    environment: true,
};

fn snapshot_script(shell_type: ShellType) -> Option<String> {
    snapshot_capture_script(shell_type, CAPTURE_ALL)
}

#[test]
fn snapshot_capture_requires_marker_and_complete_records() {
    for captured in [
        "missing header\0\0\0",
        "# Snapshot file\n\0",
        "# Snapshot file\n\0\0NAME\0export NAME=value\n",
    ] {
        assert!(CapturedSnapshot::parse(ShellType::Bash, captured.as_bytes()).is_none());
    }
}

#[test]
fn snapshot_capture_selects_declarations_and_environment() -> Result<()> {
    let dir = tempdir()?;
    for (shell, shell_type) in [
        ("/bin/bash", ShellType::Bash),
        ("/bin/sh", ShellType::Sh),
        ("/bin/zsh", ShellType::Zsh),
    ] {
        if !std::path::Path::new(shell).exists() {
            continue;
        }
        for (declarations, environment) in
            [(true, false), (false, true), (true, true), (false, false)]
        {
            let script = snapshot_capture_script(
                shell_type,
                SnapshotCaptureOptions {
                    startup: SnapshotStartup::NonInteractive,
                    declarations,
                    environment,
                },
            )
            .unwrap();
            let output = Command::new(shell)
                .args(["-c", &script])
                .env_clear()
                .env("HOME", dir.path())
                .env("PATH", "/usr/bin:/bin")
                .env("APP_SETTING", "value")
                .output()?;
            assert!(output.status.success(), "{shell}: {output:?}");
            let captured = CapturedSnapshot::parse(shell_type, &output.stdout).unwrap();
            assert_eq!(
                (
                    captured
                        .exports
                        .iter()
                        .any(|export| export.key == "APP_SETTING"),
                    captured
                        .environment
                        .split(|byte| *byte == 0)
                        .any(|entry| entry == b"APP_SETTING=value"),
                ),
                (declarations, environment),
                "{shell}"
            );
        }
    }
    Ok(())
}

#[test]
fn bash_snapshot_filters_invalid_exports() -> Result<()> {
    let output = Command::new("/bin/bash")
        .arg("-c")
        .arg(snapshot_script(ShellType::Bash).expect("bash supports snapshots"))
        .env("BASH_ENV", "/dev/null")
        .env("VALID_NAME", "ok")
        .env("PWD", "/tmp/stale")
        .env("NEXTEST_BIN_EXE_codex-write-config-schema", "/path/to/bin")
        .env("BAD-NAME", "broken")
        .output()?;

    assert!(output.status.success());

    let stdout = captured_script(ShellType::Bash, &String::from_utf8(output.stdout)?)?;
    assert!(stdout.contains("VALID_NAME"));
    assert!(!stdout.contains("PWD=/tmp/stale"));
    assert!(!stdout.contains("NEXTEST_BIN_EXE_codex-write-config-schema"));
    assert!(!stdout.contains("BAD-NAME"));

    Ok(())
}

#[test]
fn bash_snapshot_preserves_multiline_exports() -> Result<()> {
    let multiline_cert = "-----BEGIN CERTIFICATE-----\nabc\n-----END CERTIFICATE-----";
    let output = Command::new("/bin/bash")
        .arg("-c")
        .arg(snapshot_script(ShellType::Bash).expect("bash supports snapshots"))
        .env("BASH_ENV", "/dev/null")
        .env("MULTILINE_CERT", multiline_cert)
        .output()?;

    assert!(output.status.success());

    let stdout = captured_script(ShellType::Bash, &String::from_utf8(output.stdout)?)?;
    assert!(
        stdout.contains("MULTILINE_CERT=") || stdout.contains("MULTILINE_CERT"),
        "snapshot should include the multiline export name"
    );

    let dir = tempdir()?;
    let snapshot_path = dir.path().join("snapshot.sh");
    std::fs::write(&snapshot_path, stdout.as_bytes())?;

    let validate = Command::new("/bin/bash")
        .arg("-c")
        .arg("set -e; . \"$1\"")
        .arg("bash")
        .arg(&snapshot_path)
        .env("BASH_ENV", "/dev/null")
        .output()?;

    assert!(
        validate.status.success(),
        "snapshot validation failed: {}",
        String::from_utf8_lossy(&validate.stderr)
    );

    Ok(())
}

#[test]
fn snapshot_environment_preserves_native_path_exports_and_shell_level() -> Result<()> {
    let dir = tempdir()?;
    for (shell, shell_type, setup) in [
        ("/bin/bash", ShellType::Bash, "export -n PATH"),
        ("/bin/zsh", ShellType::Zsh, "typeset +x PATH"),
    ] {
        if !std::path::Path::new(shell).exists() {
            continue;
        }
        let run = |script: &str| {
            // Keep Bash's final-command exec optimization from decrementing SHLVL.
            Command::new(shell)
                .args(["-c", &format!("{setup}\n{script}\n:")])
                .env_clear()
                .env("HOME", dir.path())
                .env("PATH", "/usr/bin:/bin")
                .env("SHLVL", "3")
                .output()
        };
        let native = run("/usr/bin/env -0")?;
        let script = snapshot_capture_script(
            shell_type,
            SnapshotCaptureOptions {
                startup: SnapshotStartup::NonInteractive,
                ..CAPTURE_ALL
            },
        )
        .unwrap();
        let output = run(&script)?;
        assert!(native.status.success() && output.status.success());
        let captured = CapturedSnapshot::parse(shell_type, &output.stdout).unwrap();
        let metadata = |environment: &[u8]| {
            environment
                .split(|byte| *byte == 0)
                .filter(|entry| entry.starts_with(b"PATH=") || entry.starts_with(b"SHLVL="))
                .map(<[u8]>::to_vec)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            metadata(captured.environment),
            metadata(&native.stdout),
            "{shell}"
        );
    }
    Ok(())
}

#[test]
fn sh_declarations_preserve_path_export_state() -> Result<()> {
    let dir = tempdir()?;
    let snapshot_path = dir.path().join("snapshot.sh");
    let script = snapshot_capture_script(
        ShellType::Sh,
        SnapshotCaptureOptions {
            startup: SnapshotStartup::NonInteractive,
            ..CAPTURE_ALL
        },
    )
    .expect("sh supports snapshots");
    for shell in [
        "/bin/sh",
        #[cfg(target_os = "macos")]
        "/bin/dash",
    ] {
        if !std::path::Path::new(shell).exists() {
            continue;
        }
        for (setup, expected) in [
            ("PATH=/enterprise/bin; export PATH", "x:/enterprise/bin"),
            ("PATH=; export PATH", "x:"),
            ("unset PATH; export PATH", ":"),
            ("unset PATH; PATH=/enterprise/bin", ":"),
        ] {
            let captured = Command::new(shell)
                .args(["-uc", &format!("{setup}\n{script}")])
                .env_clear()
                .output()?;
            assert!(captured.status.success(), "capture failed: {captured:?}");
            std::fs::write(
                &snapshot_path,
                captured_script(ShellType::Sh, &String::from_utf8(captured.stdout)?)?,
            )?;
            let restored = Command::new(shell)
                .args([
                    "-c",
                    "unset PATH; . \"$1\"; printf '%s:%s' \"${PATH+x}\" \"${PATH-}\"",
                    "snapshot",
                ])
                .arg(&snapshot_path)
                .env_clear()
                .output()?;
            assert!(restored.status.success(), "replay failed: {restored:?}");
            assert_eq!(restored.stdout, expected.as_bytes(), "{setup}");
        }
    }
    Ok(())
}

#[test]
fn posix_startup_path_expansion_preserves_supported_forms() -> Result<()> {
    let script = format!(
        "{}\n__codex_snapshot_expand_env \"$ENV\"",
        super::posix_env_path_expansion_function()
    );
    let output = Command::new("/bin/sh")
        .arg("-uc")
        .arg(&script)
        .env("ENV", "$MISSING")
        .output()?;

    assert!(output.status.success(), "snapshot failed: {output:?}");
    assert_eq!(output.stdout, b"$MISSING");

    let dir = tempdir()?;
    let startup = dir.path().join("startup.sh");
    for (env_value, expansion_env) in [
        (
            startup.display().to_string(),
            Vec::<(String, String)>::new(),
        ),
        (
            "$STARTUP_FILE".to_string(),
            vec![("STARTUP_FILE".to_string(), startup.display().to_string())],
        ),
        (
            "${STARTUP_FILE}".to_string(),
            vec![("STARTUP_FILE".to_string(), startup.display().to_string())],
        ),
        (
            "${STARTUP_DIR}/startup.sh".to_string(),
            vec![("STARTUP_DIR".to_string(), dir.path().display().to_string())],
        ),
        (
            "${PATH%%:*}/startup.sh".to_string(),
            vec![(
                "PATH".to_string(),
                format!("{}:/usr/bin:/bin", dir.path().display()),
            )],
        ),
        (
            "$PATH/startup.sh".to_string(),
            vec![("PATH".to_string(), dir.path().display().to_string())],
        ),
        (
            "${PATH}/startup.sh".to_string(),
            vec![("PATH".to_string(), dir.path().display().to_string())],
        ),
        (
            format!("$PATH{}", startup.display()),
            vec![("PATH".to_string(), String::new())],
        ),
        (
            format!("${{PATH}}{}", startup.display()),
            vec![("PATH".to_string(), String::new())],
        ),
        (
            "~/startup.sh".to_string(),
            vec![("HOME".to_string(), dir.path().display().to_string())],
        ),
    ] {
        let output = Command::new("/bin/sh")
            .arg("-uc")
            .arg(&script)
            .env("ENV", &env_value)
            .envs(expansion_env)
            .output()?;

        assert!(output.status.success(), "snapshot failed: {output:?}");
        assert_eq!(
            String::from_utf8(output.stdout)?,
            startup.display().to_string(),
            "ENV={env_value}"
        );
    }

    let default_startup_dir = dir.path().join(".config");
    for xdg_config_home in [
        None,
        Some(std::ffi::OsStr::new("")),
        Some(default_startup_dir.as_os_str()),
    ] {
        let mut command = Command::new("/bin/sh");
        command
            .arg("-uc")
            .arg(&script)
            .env("HOME", dir.path())
            .env("ENV", "${XDG_CONFIG_HOME:-$HOME/.config}/startup.sh");
        if let Some(xdg_config_home) = xdg_config_home {
            command.env("XDG_CONFIG_HOME", xdg_config_home);
        } else {
            command.env_remove("XDG_CONFIG_HOME");
        }
        let output = command.output()?;

        assert!(output.status.success(), "snapshot failed: {output:?}");
        assert_eq!(
            String::from_utf8(output.stdout)?,
            default_startup_dir.join("startup.sh").display().to_string()
        );
    }

    let injected_marker = dir.path().join("env-injected");
    let injected_env = format!(
        "\"; touch '{}'; __codex_env_file=\"",
        injected_marker.display()
    );
    let output = Command::new("/bin/sh")
        .arg("-uc")
        .arg(&script)
        .env("ENV", injected_env)
        .output()?;
    assert!(output.status.success(), "snapshot failed: {output:?}");
    assert!(
        !injected_marker.exists(),
        "snapshot interpreted ENV as shell source"
    );

    Ok(())
}

#[test]
fn brokered_snapshots_filter_complete_multiline_exports() -> Result<()> {
    let dir = tempdir()?;
    for (shell, shell_type) in [
        ("/bin/bash", ShellType::Bash),
        ("/bin/sh", ShellType::Sh),
        ("/bin/dash", ShellType::Sh),
        ("/bin/zsh", ShellType::Zsh),
    ] {
        if !std::path::Path::new(shell).exists() {
            continue;
        }
        for value in [
            "before\nexport NOT_A_REAL_VARIABLE=value\nafter",
            "before'\ndeclare -x NOT_A_REAL_VARIABLE=\"quoted\"\ntypeset OTHER=value\nafter\\",
            "before\t\n# exports 99\nexport NOT_A_REAL_VARIABLE=value\nafter",
            "certificate\nwith trailing newlines\n\n",
            "before\nexport NOT_A_REAL_VARIABLE\nafter",
            "before\nexport name=not_the_actual_value\nafter",
        ] {
            let original = HashMap::from([
                ("APP_SCRIPT".to_string(), value.to_string()),
                ("EXCLUDED_SCRIPT".to_string(), value.to_string()),
                ("ZZZ_AFTER".to_string(), "kept".to_string()),
                ("name".to_string(), "original name".to_string()),
                ("value".to_string(), "original value".to_string()),
                ("escaped".to_string(), "original escaped".to_string()),
            ]);
            let mut allowed = original.clone();
            allowed.remove("EXCLUDED_SCRIPT");
            let function_value = "before\n# exports 1\nexport NOT_A_REAL_VARIABLE=value\nafter";
            let capture_script = snapshot_capture_script(
                shell_type,
                SnapshotCaptureOptions {
                    startup: SnapshotStartup::NonInteractive,
                    ..CAPTURE_ALL
                },
            )
            .expect("shell supports snapshots");
            let capture = if matches!(shell_type, ShellType::Sh) {
                capture_script
            } else {
                format!("emit_source() {{ cat <<'EOF'\n{function_value}\nEOF\n}}\n{capture_script}")
            };
            let capture = if matches!(shell_type, ShellType::Sh | ShellType::Zsh) {
                format!("export APP_SETTING EXCLUDED_SETTING\n{capture}")
            } else {
                capture
            };
            let captured = Command::new(shell)
                .args(["-c", &capture])
                .env_clear()
                .envs(&original)
                .env("PATH", "/usr/bin:/bin")
                .output()?;
            assert!(captured.status.success(), "{shell}: {captured:?}");
            let mut snapshot = String::from_utf8(captured.stdout)?;
            let records = CapturedSnapshot::parse(shell_type, snapshot.as_bytes()).unwrap();
            assert_eq!(
                records
                    .exports
                    .iter()
                    .filter(|export| export.key == "name")
                    .count(),
                1,
                "{shell}: duplicate export record"
            );
            if matches!(shell_type, ShellType::Sh) {
                let raw_path = dir.path().join("raw-snapshot.sh");
                std::fs::write(&raw_path, captured_script(shell_type, &snapshot)?)?;
                let replayed = Command::new(shell)
                    .args(["-c", ". \"$1\"; printf '%s\\n' \"${APP_SETTING-unset}\"; APP_SETTING=production; /usr/bin/printenv APP_SETTING; NOT_A_REAL_VARIABLE=unexpected; /usr/bin/printenv NOT_A_REAL_VARIABLE", "snapshot"])
                    .arg(&raw_path)
                    .env_clear()
                    .output()?;
                assert_eq!(
                    replayed.stdout, b"unset\nproduction\n",
                    "{shell}: {snapshot}"
                );
                assert_eq!(replayed.status.code(), Some(1));
            }
            rewrite_snapshot_credentials(
                shell_type,
                &mut snapshot,
                SnapshotCredentialEnvironment {
                    original: &original,
                    restored: &original,
                    configured: &HashMap::new(),
                    discovered: &allowed,
                    allowed: &allowed,
                    is_allowed_unset: &|key| key == "APP_SETTING",
                    brokered_keys: &[],
                    brokered_alias_keys: &[],
                    allowed_brokered_keys: &[],
                },
                |_| true,
            );
            let path = dir.path().join("snapshot.sh");
            std::fs::write(&path, snapshot)?;
            let replayed = Command::new(shell)
                .args(["-c", ". \"$1\"; printf '%s\\0%s\\0%s\\0%s\\0%s\\0%s' \"$APP_SCRIPT\" \"${EXCLUDED_SCRIPT-unset}\" \"$ZZZ_AFTER\" \"$name\" \"$value\" \"$escaped\"; if command -v emit_source >/dev/null; then emit_source; fi; APP_SETTING=production; EXCLUDED_SETTING=denied; /usr/bin/printenv APP_SETTING; /usr/bin/printenv EXCLUDED_SETTING || :", "snapshot"])
                .arg(&path)
                .env_clear()
                .output()?;
            assert!(replayed.status.success(), "{shell}: {replayed:?}");
            let function_value = if matches!(shell_type, ShellType::Sh) {
                String::new()
            } else {
                format!("{function_value}\n")
            };
            let exported_setting = if matches!(shell_type, ShellType::Sh | ShellType::Zsh) {
                "production\n"
            } else {
                ""
            };
            assert_eq!(
                replayed.stdout,
                format!("{value}\0unset\0kept\0original name\0original value\0original escaped{function_value}{exported_setting}").as_bytes(),
                "{shell}"
            );
        }
    }
    Ok(())
}

#[test]
fn brokered_snapshot_alias_values_follow_allowed_credential_overrides() {
    let real = "ghp_abcdefghijklmnopqrstuvwxyz0123456789";
    let discovered_dummy = "ghp_discovered_dummy_abcdefghijklmnopqr";
    let configured_dummy = "ghp_configured_dummy_abcdefghijklmnopqr";
    let original = HashMap::from([
        ("GH_TOKEN".to_string(), real.to_string()),
        ("AUTH_HEADER".to_string(), format!("Bearer {real}")),
    ]);
    let discovered = HashMap::from([("GH_TOKEN".to_string(), discovered_dummy.to_string())]);
    let brokered_keys = vec!["GH_TOKEN".to_string()];
    for (source_allowed, alias, expected) in [
        (
            true,
            format!("Bearer {discovered_dummy}"),
            Some((
                format!("Bearer {configured_dummy}"),
                r#"export AUTH_HEADER='Bearer '"${GH_TOKEN-}""#,
            )),
        ),
        (
            true,
            String::new(),
            Some((String::new(), "export AUTH_HEADER=''")),
        ),
        (false, format!("Bearer {discovered_dummy}"), None),
    ] {
        let mut allowed = HashMap::from([
            ("GH_TOKEN".to_string(), configured_dummy.to_string()),
            ("AUTH_HEADER".to_string(), alias),
        ]);
        let allowed_brokered_keys = if source_allowed {
            brokered_keys.as_slice()
        } else {
            allowed.remove("GH_TOKEN");
            &[]
        };
        let mut snapshot =
            format!("# Snapshot file\n\0\0AUTH_HEADER\0export AUTH_HEADER='Bearer {real}'\n\0\0");
        let prepared = rewrite_snapshot_credentials(
            ShellType::Bash,
            &mut snapshot,
            SnapshotCredentialEnvironment {
                original: &original,
                restored: &original,
                configured: &HashMap::new(),
                discovered: &discovered,
                allowed: &allowed,
                is_allowed_unset: &|_| false,
                brokered_keys: &brokered_keys,
                brokered_alias_keys: &[],
                allowed_brokered_keys,
            },
            |text| {
                *text = text.replace(real, discovered_dummy);
                true
            },
        );
        let expected = match expected {
            Some((value, assignment)) => (
                format!("# Snapshot file\n# exports (native declarations)\n{assignment}\n"),
                HashMap::from([("AUTH_HEADER".to_string(), value)]),
                Vec::new(),
            ),
            None => (
                "# Snapshot file\n".to_string(),
                HashMap::new(),
                vec!["AUTH_HEADER".to_string()],
            ),
        };
        assert_eq!(
            prepared.map(|rewrite| (rewrite.script, rewrite.aliases, rewrite.rejected_alias_keys)),
            Some(expected),
        );
    }
}

#[cfg(target_os = "macos")]
#[test]
fn zsh_snapshot_restores_tied_path() -> Result<()> {
    let dir = tempdir()?;
    let path_with_spaces = dir.path().join("path with spaces").join("bin");
    let plain_path = dir.path().join("plain-path").join("bin");
    let expected_path = format!(
        "{}:{}:/usr/bin:/bin",
        path_with_spaces.display(),
        plain_path.display()
    );
    let zshrc = format!(
        "export -UT PATH path=('{}' '{}' '{}' /usr/bin /bin)\n",
        path_with_spaces.display(),
        plain_path.display(),
        plain_path.display()
    );
    std::fs::write(dir.path().join(".zshrc"), zshrc)?;

    let snapshot = Command::new("/bin/zsh")
        .arg("-f")
        .arg("-c")
        .arg(snapshot_script(ShellType::Zsh).expect("zsh supports snapshots"))
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("ZDOTDIR", dir.path())
        .output()?;
    assert!(snapshot.status.success());

    let snapshot_path = dir.path().join("snapshot.sh");
    let snapshot = captured_script(ShellType::Zsh, &String::from_utf8(snapshot.stdout)?)?;
    std::fs::write(&snapshot_path, &snapshot)?;

    let restored = Command::new("/bin/zsh")
        .arg("-f")
        .arg("-c")
        .arg("set -e; . \"$1\"; print -r -- \"$PATH\"")
        .arg("zsh")
        .arg(&snapshot_path)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .output()?;
    assert!(restored.status.success());
    assert_eq!(
        String::from_utf8(restored.stdout)?.trim_end(),
        expected_path
    );

    assert!(
        snapshot
            .lines()
            .any(|line| line.starts_with("export -UT PATH path=")),
        "snapshot should capture the tied PATH export"
    );

    std::fs::write(dir.path().join(".zshrc"), "readonly PATH\n")?;
    let readonly_snapshot = Command::new("/bin/zsh")
        .arg("-f")
        .arg("-c")
        .arg(snapshot_script(ShellType::Zsh).expect("zsh supports snapshots"))
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("ZDOTDIR", dir.path())
        .output()?;
    assert!(readonly_snapshot.status.success());
    let readonly_snapshot = captured_script(
        ShellType::Zsh,
        &String::from_utf8(readonly_snapshot.stdout)?,
    )?;
    std::fs::write(&snapshot_path, &readonly_snapshot)?;

    let readonly_restored = Command::new("/bin/zsh")
        .arg("-f")
        .arg("-c")
        .arg("set -e; . \"$1\"; export PATH='/codex-path':\"$PATH\"; print -r -- \"$PATH\"")
        .arg("zsh")
        .arg(&snapshot_path)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .output()?;
    assert!(readonly_restored.status.success());
    assert_eq!(
        String::from_utf8(readonly_restored.stdout)?.trim_end(),
        "/codex-path:/usr/bin:/bin"
    );

    assert!(
        !readonly_snapshot
            .lines()
            .any(|line| line.starts_with("export -rT PATH path=")),
        "snapshot should not capture the readonly tied PATH export"
    );

    Ok(())
}

// Literal-only fixtures are shell source rather than capture output. Give those fixtures
// empty alias/export sections; real shell fixtures exercise the full framed capture.
fn rewrite_snapshot_credentials(
    shell_type: ShellType,
    snapshot: &mut String,
    environment: SnapshotCredentialEnvironment<'_>,
    virtualize_text: impl FnMut(&mut String) -> bool,
) -> Option<PreparedSnapshot> {
    let source_only = !snapshot.contains('\0');
    let framed = if source_only {
        format!("# Snapshot file\n{snapshot}\0\0\0")
    } else {
        snapshot.clone()
    };
    let capture = CapturedSnapshot::parse(shell_type, framed.as_bytes())?;
    let prepared = prepare_snapshot_credentials(&capture, environment, virtualize_text)?;
    *snapshot = if source_only {
        prepared
            .script
            .strip_prefix("# Snapshot file\n")?
            .to_string()
    } else {
        prepared.script.clone()
    };
    Some(prepared)
}

fn captured_script(shell_type: ShellType, source: &str) -> Result<String> {
    let captured =
        CapturedSnapshot::parse(shell_type, source.as_bytes()).context("invalid native capture")?;
    Ok(captured.render_script())
}
