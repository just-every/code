use std::{
    error::Error,
    io,
    time::{Duration, Instant},
};

use ratatui::{
    backend::TermwizBackend,
    termwiz::{input::*, terminal::Terminal as TermwizTerminal},
    Terminal,
};

use crate::{ui, App};

pub fn run() -> Result<(), Box<dyn Error>> {
    let backend = TermwizBackend::new()?;
    let mut terminal = Terminal::new(backend)?;
    terminal.hide_cursor()?;

    // create app and run it
    let app = App::new(&mut terminal);
    let res = run_app(&mut terminal, app);

    terminal.show_cursor()?;
    terminal.flush()?;

    if let Err(err) = res {
        println!("{err:?}");
    }

    Ok(())
}

fn run_app(terminal: &mut Terminal<TermwizBackend>, mut app: App) -> io::Result<()> {
    let mut last_tick = Instant::now();
    loop {
        terminal.draw(|f| ui(f, &mut app))?;

        let timeout = app
            .tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));
        if let Ok(Some(input)) = terminal
            .backend_mut()
            .buffered_terminal_mut()
            .terminal()
            .poll_input(Some(timeout))
        {
            match input {
                InputEvent::Key(key_code) => {
                    if let KeyCode::Char(c) = key_code.key {
                        app.on_key(c);
                    }
                }
                InputEvent::Resized { cols, rows } => {
                    terminal
                        .backend_mut()
                        .buffered_terminal_mut()
                        .resize(cols, rows);
                }
                _ => {}
            }
        }

        if last_tick.elapsed() >= app.tick_rate {
            app.on_tick();
            last_tick = Instant::now();
        }
        if app.should_quit {
            return Ok(());
        }
    }
}
