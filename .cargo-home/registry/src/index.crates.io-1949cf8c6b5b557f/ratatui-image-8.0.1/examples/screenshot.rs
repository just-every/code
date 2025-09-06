use std::{
    assert_eq, env, io,
    process::{Command, Stdio},
};

use ratatui::{
    backend::CrosstermBackend,
    crossterm::{
        execute,
        terminal::{
            disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen, SetSize,
        },
    },
    layout::Rect,
    widgets::{Block, Borders, Paragraph},
    Frame, Terminal,
};
use ratatui_image::{picker::Picker, protocol::Protocol, Image, Resize};
struct App {
    image: Protocol,
}

const ASSERT_FONT_SIZE: (u16, u16) = (9, 18);
const SCREEN_SIZE: (u16, u16) = (46, 12);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        SetSize(SCREEN_SIZE.0, SCREEN_SIZE.1)
    )?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut picker = Picker::from_query_stdio()?;
    picker.set_background_color([255, 0, 255, 255]);
    if false {
        assert_eq!(
            ASSERT_FONT_SIZE,
            picker.font_size(),
            "Font size must be fixed to a specific size: {ASSERT_FONT_SIZE:?}",
        );
    }
    let dyn_img = image::ImageReader::open("./assets/Ada.png")?.decode()?;
    let image = picker.new_protocol(
        dyn_img,
        Rect::new(0, 0, SCREEN_SIZE.0 - 10, SCREEN_SIZE.1 - 4),
        Resize::Fit(None),
    )?;
    let mut app = App { image };

    terminal.draw(|f| ui(f, &mut app))?;
    std::thread::sleep(std::time::Duration::from_secs(1)); // let the terminal actually draw.
    #[allow(clippy::zombie_processes)] // TODO: fix this!
    let xwd = Command::new("xwd")
        .args(["-root", "-silent"])
        .stdout(Stdio::piped())
        .spawn()
        .expect("Failed to start xwd command");
    let screenshot_term = env::var("SCREENSHOT_TERM_NAME").unwrap_or("unknown".to_string());
    std::process::Command::new("convert")
        .args([
            "xwd:-",
            &format!("png:./target/screenshot_{screenshot_term}.png"),
        ])
        .stdin(xwd.stdout.expect("failed to get stdout"))
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .and_then(|mut child| child.wait())?;

    // restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen,)?;
    terminal.show_cursor()?;

    Ok(())
}

fn ui(f: &mut Frame<'_>, app: &mut App) {
    let area = Rect::new(0, 0, SCREEN_SIZE.0, SCREEN_SIZE.1);
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Screenshot test");

    f.render_widget(
        Paragraph::new("PartiallyHiddenScreenshotParagraphBackground\n".repeat(10)),
        block.inner(area),
    );

    let image = Image::new(&app.image);
    f.render_widget(image, block.inner(area));
    f.render_widget(block, area);
}
