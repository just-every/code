use std::{error::Error, io, sync::mpsc, thread, time::Duration};

use ratatui::{
    backend::{Backend, TermionBackend},
    termion::{
        event::Key,
        input::{MouseTerminal, TermRead},
        raw::IntoRawMode,
        screen::IntoAlternateScreen,
    },
    Terminal,
};

use crate::{ui, App};

pub fn run() -> Result<(), Box<dyn Error>> {
    // setup terminal
    let stdout = io::stdout()
        .into_raw_mode()
        .unwrap()
        .into_alternate_screen()
        .unwrap();
    let stdout = MouseTerminal::from(stdout);
    let backend = TermionBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // create app and run it
    let app = App::new(&mut terminal);
    run_app(&mut terminal, app)?;

    Ok(())
}

fn run_app<B: Backend>(terminal: &mut Terminal<B>, mut app: App) -> Result<(), Box<dyn Error>> {
    let events = events(app.tick_rate);
    loop {
        terminal.draw(|f| ui(f, &mut app))?;

        match events.recv()? {
            Event::Input(key) => {
                if let Key::Char(c) = key {
                    app.on_key(c);
                }
            }
            Event::Tick => app.on_tick(),
        }
        if app.should_quit {
            return Ok(());
        }
    }
}

enum Event {
    Input(Key),
    Tick,
}

fn events(tick_rate: Duration) -> mpsc::Receiver<Event> {
    let (tx, rx) = mpsc::channel();
    let keys_tx = tx.clone();
    thread::spawn(move || {
        let stdin = io::stdin();
        for key in stdin.keys().flatten() {
            if let Err(err) = keys_tx.send(Event::Input(key)) {
                eprintln!("{err}");
                return;
            }
        }
    });
    thread::spawn(move || loop {
        if let Err(err) = tx.send(Event::Tick) {
            eprintln!("{err}");
            break;
        }
        thread::sleep(tick_rate);
    });
    rx
}
