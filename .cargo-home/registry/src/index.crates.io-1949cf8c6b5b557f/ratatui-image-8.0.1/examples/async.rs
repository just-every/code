use std::{
    io,
    sync::mpsc::{self},
    thread,
    time::Duration,
};

use ratatui::{
    backend::CrosstermBackend,
    crossterm::{
        event::{self, Event, KeyCode, KeyEvent, KeyEventKind},
        execute,
        terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    },
    widgets::{Block, Borders, Paragraph},
    Frame, Terminal,
};
use ratatui_image::{
    errors::Errors,
    picker::Picker,
    thread::{ResizeRequest, ResizeResponse, ThreadProtocol},
    StatefulImage,
};

struct App {
    async_state: ThreadProtocol,
}

enum AppEvent {
    KeyEvent(KeyEvent),
    Redraw(Result<ResizeResponse, Errors>),
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen,)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let picker = Picker::from_query_stdio()?;
    let dyn_img = image::ImageReader::open("./assets/Ada.png")?.decode()?;

    // Send a [ResizeProtocol] to resize and encode it in a separate thread.
    let (tx_worker, rec_worker) = mpsc::channel::<ResizeRequest>();

    // Send UI-events and the [ResizeProtocol] result back to main thread.
    let (tx_main, rec_main) = mpsc::channel();

    // Resize and encode in background thread.
    let tx_main_render = tx_main.clone();
    thread::spawn(move || loop {
        if let Ok(request) = rec_worker.recv() {
            tx_main_render
                .send(AppEvent::Redraw(request.resize_encode()))
                .unwrap();
        }
    });

    // Poll events in background thread to demonstrate polling terminal events and redraw events
    // concurrently. It's not required to do it this way - the "redraw event" from the channel
    // could be read after polling the terminal events (as long as it's done with a timout). But
    // then the rendering of the image will always be somewhat delayed.
    let tx_main_events = tx_main.clone();
    thread::spawn(move || -> Result<(), std::io::Error> {
        loop {
            if ratatui::crossterm::event::poll(Duration::from_millis(1000))? {
                if let Event::Key(key) = event::read()? {
                    tx_main_events.send(AppEvent::KeyEvent(key)).unwrap();
                }
            }
        }
    });

    let mut app = App {
        async_state: ThreadProtocol::new(tx_worker, Some(picker.new_resize_protocol(dyn_img))),
    };

    loop {
        terminal.draw(|f| ui(f, &mut app))?;

        if let Ok(ev) = rec_main.try_recv() {
            match ev {
                AppEvent::KeyEvent(key) => {
                    if key.kind == KeyEventKind::Press && key.code == KeyCode::Char('q') {
                        break;
                    }
                }
                AppEvent::Redraw(completed) => {
                    let _ = app.async_state.update_resized_protocol(completed?);
                }
            }
        }
    }

    // restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen,)?;
    terminal.show_cursor()?;

    Ok(())
}

fn ui(f: &mut Frame<'_>, app: &mut App) {
    let area = f.area();
    let block = Block::default().borders(Borders::ALL).title("Async test");

    f.render_widget(
        Paragraph::new("PartiallyHiddenScreenshotParagraphBackground\n".repeat(10)),
        block.inner(area),
    );
    f.render_stateful_widget(
        StatefulImage::new(),
        block.inner(area),
        &mut app.async_state,
    );
    f.render_widget(block, area);
}
