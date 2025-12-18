use std::io::{self, Write};

use crate::colors;
use crossterm::cursor::MoveTo;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::queue;
use crossterm::style::{Attribute as CtAttribute, Color as CtColor, Print, ResetColor, SetAttribute, SetForegroundColor};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, Clear, ClearType};
use crossterm::ExecutableCommand;

pub(crate) enum ModelMigrationOutcome {
    Accepted,
    Rejected,
    Exit,
}

pub(crate) struct ModelMigrationCopy {
    pub heading: &'static str,
    pub content: &'static [&'static str],
    pub can_opt_out: bool,
}

pub(crate) fn migration_copy_for_key(key: &str) -> ModelMigrationCopy {
    match key {
        code_common::model_presets::HIDE_GPT5_1_MIGRATION_PROMPT_CONFIG => ModelMigrationCopy {
            heading: "Introducing our gpt-5.1 models",
            content: &[
                "We've upgraded Codex to gpt-5.1, gpt-5.1-codex, and gpt-5.1-codex-mini.",
                "Legacy gpt-5 models continue to work via -m or config.toml overrides.",
                "Learn more: www.openai.com/index/gpt-5-1",
                "Press Enter to continue.",
            ],
            can_opt_out: false,
        },
        code_common::model_presets::HIDE_GPT_5_2_MIGRATION_PROMPT_CONFIG => ModelMigrationCopy {
            heading: "Upgrade available: GPT-5.2",
            content: &[
                "OpenAI's latest frontier model is here! Improved knowledge, reasoning, and coding.",
                "Switch now to get better results; you can keep your current model if you prefer.",
                "Learn more: www.openai.com/index/gpt-5-2",
            ],
            can_opt_out: true,
        },
        code_common::model_presets::HIDE_GPT_5_2_CODEX_MIGRATION_PROMPT_CONFIG => ModelMigrationCopy {
            heading: "Upgrade available: GPT-5.2 Codex",
            content: &[
                "OpenAI's latest frontier agentic coding model is here: gpt-5.2-codex.",
                "Switch now for better coding results; you can keep your current model if you prefer.",
                "Learn more: https://openai.com/index/introducing-gpt-5-2-codex/",
            ],
            can_opt_out: true,
        },
        _ => ModelMigrationCopy {
            heading: "Codex just got an upgrade: meet gpt-5.1-codex-max",
            content: &[
                "Our flagship agentic coding model is smarter, faster, and tuned for long sessions.",
                "Everyone signed in with ChatGPT gets it automatically.",
                "Learn more: www.openai.com/index/gpt-5-1-codex-max",
                "Choose how you'd like Codex to proceed.",
            ],
            can_opt_out: true,
        },
    }
}

pub(crate) fn run_model_migration_prompt(copy: &ModelMigrationCopy) -> io::Result<ModelMigrationOutcome> {
    struct RawModeGuard;
    impl RawModeGuard {
        fn new() -> io::Result<Self> {
            enable_raw_mode()?;
            Ok(Self)
        }
    }
    impl Drop for RawModeGuard {
        fn drop(&mut self) {
            let _ = disable_raw_mode();
        }
    }

    let _guard = RawModeGuard::new()?;

    let mut stdout = io::stdout();
    let mut highlighted = 0usize;
    render_prompt(&mut stdout, copy, highlighted)?;

    loop {
        let event = event::read()?;
        if let Event::Key(KeyEvent { code, modifiers, kind, .. }) = event {
            if matches!(kind, KeyEventKind::Release) {
                continue;
            }

            if modifiers.contains(KeyModifiers::CONTROL)
                && matches!(code, KeyCode::Char('c') | KeyCode::Char('d'))
            {
                return Ok(ModelMigrationOutcome::Exit);
            }

            if !copy.can_opt_out {
                match code {
                    KeyCode::Enter | KeyCode::Esc => {
                        return Ok(ModelMigrationOutcome::Accepted);
                    }
                    _ => {}
                }
                continue;
            }

            match code {
                KeyCode::Up | KeyCode::Char('k') => {
                    highlighted = 0;
                    render_prompt(&mut stdout, copy, highlighted)?;
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    highlighted = 1;
                    render_prompt(&mut stdout, copy, highlighted)?;
                }
                KeyCode::Char('1') => return Ok(ModelMigrationOutcome::Accepted),
                KeyCode::Char('2') => return Ok(ModelMigrationOutcome::Rejected),
                KeyCode::Enter => {
                    return if highlighted == 0 {
                        Ok(ModelMigrationOutcome::Accepted)
                    } else {
                        Ok(ModelMigrationOutcome::Rejected)
                    };
                }
                KeyCode::Esc => return Ok(ModelMigrationOutcome::Rejected),
                KeyCode::Char('q') => return Ok(ModelMigrationOutcome::Exit),
                _ => {}
            }
        }
    }
}

fn render_prompt(stdout: &mut io::Stdout, copy: &ModelMigrationCopy, highlighted: usize) -> io::Result<()> {
    stdout.execute(Clear(ClearType::All))?;
    stdout.execute(MoveTo(0, 0))?;

    if copy.heading == "Upgrade available: GPT-5.2 Codex" {
        let success_fg = CtColor::from(colors::success());
        write_line_fg_bold(stdout, copy.heading, success_fg)?;
    } else {
        write_line(stdout, copy.heading)?;
    }
    write_blank(stdout)?;
    for line in copy.content {
        write_line(stdout, line)?;
    }

    if copy.can_opt_out {
        write_blank(stdout)?;
        let primary_fg = CtColor::from(colors::primary());
        for (idx, label) in ["Try new model (recommended)", "Use existing model"].iter().enumerate() {
            if idx == highlighted {
                queue!(stdout, SetForegroundColor(primary_fg), Print("> "), Print(*label), ResetColor, Print("\r\n"))?;
            } else {
                queue!(stdout, Print("  "), Print(*label), Print("\r\n"))?;
            }
        }
        write_blank(stdout)?;
        write_key_tip_line(stdout)?;
    }

    stdout.flush()
}

fn write_line(stdout: &mut io::Stdout, line: &str) -> io::Result<()> {
    stdout.write_all(line.as_bytes())?;
    stdout.write_all(b"\r\n")
}

fn write_line_fg_bold(stdout: &mut io::Stdout, line: &str, fg: CtColor) -> io::Result<()> {
    queue!(
        stdout,
        SetForegroundColor(fg),
        SetAttribute(CtAttribute::Bold),
        Print(line),
        SetAttribute(CtAttribute::NormalIntensity),
        ResetColor,
        Print("\r\n")
    )?;
    Ok(())
}

fn write_key_tip_line(stdout: &mut io::Stdout) -> io::Result<()> {
    let tip_fg = CtColor::from(colors::function());
    queue!(
        stdout,
        Print("Use "),
        SetForegroundColor(tip_fg),
        Print("↑/↓"),
        ResetColor,
        Print(" to move, "),
        SetForegroundColor(tip_fg),
        Print("Enter"),
        ResetColor,
        Print(" to confirm, "),
        SetForegroundColor(tip_fg),
        Print("Esc"),
        ResetColor,
        Print(" to keep current model.\r\n")
    )?;
    Ok(())
}

fn write_blank(stdout: &mut io::Stdout) -> io::Result<()> {
    stdout.write_all(b"\r\n")
}
