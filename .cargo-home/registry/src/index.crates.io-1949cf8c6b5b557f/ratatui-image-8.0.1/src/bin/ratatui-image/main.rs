use std::{
    env, io,
    time::{Duration, Instant},
};

use image::DynamicImage;
use ratatui::{
    backend::CrosstermBackend,
    crossterm::{
        event::{self, Event, KeyCode, KeyEventKind},
        execute,
        terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    },
    layout::{Constraint, Direction, Layout},
    text::Line,
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame, Terminal,
};
use ratatui_image::{picker::Picker, protocol::StatefulProtocol, StatefulImage};

struct App {
    pub filename: String,
    pub picker: Picker,
    pub image_source: DynamicImage,
    pub image_state: StatefulProtocol,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let filename = env::args()
        .nth(1)
        .expect("Usage: <program> <path/to/image>");

    let picker = Picker::from_query_stdio().unwrap_or_else(|_| {
        let font_width = env::args()
            .nth(2)
            .expect("Usage: <program> <path/to/image> <font-width> <font-height>");
        let font_height = env::args()
            .nth(3)
            .expect("Usage: <program> <path/to/image> <font-width> <font-height>");
        let font_size = (
            font_height.parse::<u16>().expect("could not parse size"),
            font_width.parse::<u16>().expect("could not parse size"),
        );
        Picker::from_fontsize(font_size)
    });

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let image_source = image::ImageReader::open(&filename)?.decode()?;

    let image_state = picker.new_resize_protocol(image_source.clone());

    let mut app = App {
        filename,
        picker,
        image_source,
        image_state,
    };

    let mut last_tick = Instant::now();
    let tick_rate = Duration::from_millis(1000);
    loop {
        terminal.draw(|f| ui(f, &mut app))?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));
        if ratatui::crossterm::event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char(c) => match c {
                            'q' => break,
                            ' ' => {
                                app.picker
                                    .set_protocol_type(app.picker.protocol_type().next());
                                app.image_state =
                                    app.picker.new_resize_protocol(app.image_source.clone());
                            }
                            _ => {}
                        },
                        KeyCode::Esc => break,
                        _ => {}
                    }
                }
            }
        }
        if last_tick.elapsed() >= tick_rate {
            last_tick = Instant::now();
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen,)?;
    terminal.show_cursor()?;

    Ok(())
}

fn ui(f: &mut Frame<'_>, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Max(5), Constraint::Min(1)].as_ref())
        .split(f.area());

    let block_top = Block::default()
        .borders(Borders::ALL)
        .title("ratatui-image");
    let lines = vec![
        Line::from(format!(
            "Protocol: {:?}, font size: {:?}",
            app.picker.protocol_type(),
            app.picker.font_size(),
        )),
        Line::from(format!("File: {}", app.filename)),
        Line::from(format!(
            "Image: {:?} {:?}",
            (app.image_source.width(), app.image_source.height()),
            app.image_source.color()
        )),
    ];
    f.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: true }),
        block_top.inner(chunks[0]),
    );
    f.render_widget(block_top, chunks[0]);

    let block_bottom = Block::default().borders(Borders::ALL).title("image");
    let image = StatefulImage::default();
    f.render_stateful_widget(image, block_bottom.inner(chunks[1]), &mut app.image_state);
    f.render_widget(block_bottom, chunks[1]);
}
