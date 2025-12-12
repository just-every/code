use std::io::{self, Write};

use crossterm::cursor::MoveTo;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
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
            heading: "Upgrade available: gpt-5.2",
            content: &[
                "Our latest frontier model improves knowledge, reasoning, and coding.",
                "Switch now to get better results; you can keep your current model if you prefer.",
                "Learn more: www.openai.com/index/gpt-5-2",
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

    writeln!(stdout, "{}", copy.heading)?;
    writeln!(stdout)?;
    for line in copy.content {
        writeln!(stdout, "{}", line)?;
    }

    if copy.can_opt_out {
        writeln!(stdout)?;
        for (idx, label) in ["Try new model (recommended)", "Use existing model"].iter().enumerate() {
            if idx == highlighted {
                writeln!(stdout, "> {label}")?;
            } else {
                writeln!(stdout, "  {label}")?;
            }
        }
        writeln!(stdout)?;
        writeln!(stdout, "Use ↑/↓ to move, Enter to confirm, Esc to keep current model.")?;
    }

    stdout.flush()
}
