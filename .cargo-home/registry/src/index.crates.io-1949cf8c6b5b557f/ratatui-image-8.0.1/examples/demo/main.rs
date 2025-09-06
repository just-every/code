#[cfg(all(
    not(feature = "crossterm"),
    not(feature = "termion"),
    not(feature = "termwiz")
))]
compile_error!("The demo needs one of the crossterm, termion, or termwiz features");

#[cfg(feature = "crossterm")]
mod crossterm;
#[cfg(feature = "termion")]
mod termion;
#[cfg(feature = "termwiz")]
mod termwiz;

use std::{env, error::Error, num::Wrapping as w, path::PathBuf, time::Duration};

use image::DynamicImage;
use ratatui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Stylize},
    text::{Line, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame, Terminal,
};
use ratatui_image::{
    picker::Picker,
    protocol::{Protocol, StatefulProtocol},
    Image, Resize, StatefulImage,
};

fn main() -> Result<(), Box<dyn Error>> {
    #[cfg(feature = "crossterm")]
    crate::crossterm::run()?;
    #[cfg(feature = "termion")]
    crate::termion::run()?;
    #[cfg(feature = "termwiz")]
    crate::termwiz::run()?;
    Ok(())
}

#[derive(Debug)]
enum ShowImages {
    All,
    Fixed,
    Resized,
}

struct App {
    title: String,
    should_quit: bool,
    tick_rate: Duration,
    background: String,
    split_percent: u16,
    show_images: ShowImages,

    image_source_path: PathBuf,
    image_static_offset: (u16, u16),

    picker: Picker,
    image_source: DynamicImage,
    image_static: Protocol,
    image_fit_state: StatefulProtocol,
    image_crop_state: StatefulProtocol,
    image_scale_state: StatefulProtocol,
}

fn size() -> Rect {
    Rect::new(0, 0, 30, 16)
}

impl App {
    pub fn new<B: Backend>(_: &mut Terminal<B>) -> Self {
        let title = format!(
            "Demo ({})",
            env::var("TERM").unwrap_or("unknown".to_string())
        );

        let ada = "./assets/Ada.png";
        let image_source = image::ImageReader::open(ada).unwrap().decode().unwrap();

        let mut picker = Picker::from_query_stdio().unwrap();
        // Set completely transparent background (experimental, only works for iTerm2 and Kitty).
        picker.set_background_color([0, 0, 0, 0]);

        let image_static = picker
            .new_protocol(image_source.clone(), size(), Resize::Fit(None))
            .unwrap();
        let image_fit_state = picker.new_resize_protocol(image_source.clone());
        let image_crop_state = picker.new_resize_protocol(image_source.clone());
        let image_scale_state = picker.new_resize_protocol(image_source.clone());

        let mut background = String::new();

        let mut r: [u64; 2] = [0x8a5cd789635d2dff, 0x121fd2155c472f96];
        for _ in 0..5_000 {
            let mut s1 = w(r[0]);
            let s0 = w(r[1]);
            let result = s0 + s1;
            r[0] = s0.0;
            s1 ^= s1 << 23;
            r[1] = (s1 ^ s0 ^ (s1 >> 18) ^ (s0 >> 5)).0;
            let c = match result.0 % 4 {
                0 => '.',
                1 => ' ',
                _ => '…',
            };
            background.push(c);
        }

        Self {
            title,
            should_quit: false,
            tick_rate: Duration::from_millis(1000),
            background,
            show_images: ShowImages::All,
            split_percent: 70,
            picker,
            image_source,
            image_source_path: ada.into(),

            image_static,
            image_fit_state,
            image_crop_state,
            image_scale_state,

            image_static_offset: (0, 0),
        }
    }
    pub fn on_key(&mut self, c: char) {
        match c {
            'q' => {
                self.should_quit = true;
            }
            't' => {
                self.show_images = match self.show_images {
                    ShowImages::All => ShowImages::Fixed,
                    ShowImages::Fixed => ShowImages::Resized,
                    ShowImages::Resized => ShowImages::All,
                }
            }
            'i' => {
                let next = self.picker.protocol_type().next();
                self.picker.set_protocol_type(next);
                self.reset_images();
            }
            'o' => {
                let path = match self.image_source_path.to_str() {
                    Some("./assets/Ada.png") => "./assets/Jenkins.jpg",
                    Some("./assets/Jenkins.jpg") => "./assets/NixOS.png",
                    _ => "./assets/Ada.png",
                };
                self.image_source = image::ImageReader::open(path).unwrap().decode().unwrap();
                self.image_source_path = path.into();
                self.reset_images();
            }
            'H' => {
                if self.split_percent >= 10 {
                    self.split_percent -= 10;
                }
            }
            'L' => {
                if self.split_percent <= 90 {
                    self.split_percent += 10;
                }
            }
            'h' => {
                if self.image_static_offset.0 > 0 {
                    self.image_static_offset.0 -= 1;
                }
            }
            'j' => {
                self.image_static_offset.1 += 1;
            }
            'k' => {
                if self.image_static_offset.1 > 0 {
                    self.image_static_offset.1 -= 1;
                }
            }
            'l' => {
                self.image_static_offset.0 += 1;
            }
            _ => {}
        }
    }

    fn reset_images(&mut self) {
        self.image_static = self
            .picker
            .new_protocol(self.image_source.clone(), size(), Resize::Fit(None))
            .unwrap();
        self.image_fit_state = self.picker.new_resize_protocol(self.image_source.clone());
        self.image_crop_state = self.picker.new_resize_protocol(self.image_source.clone());
        self.image_scale_state = self.picker.new_resize_protocol(self.image_source.clone());
    }

    pub fn on_tick(&mut self) {}

    fn render_resized_image(&mut self, f: &mut Frame<'_>, resize: Resize, area: Rect) {
        let (state, name, color) = match resize {
            Resize::Fit(_) => (&mut self.image_fit_state, "Fit", Color::Magenta),
            Resize::Crop(_) => (&mut self.image_crop_state, "Crop", Color::Green),
            Resize::Scale(_) => (&mut self.image_scale_state, "Scale", Color::Blue),
        };
        let block = block(name);
        let inner_area = block.inner(area);
        f.render_widget(paragraph(self.background.as_str().bg(color)), inner_area);
        match self.show_images {
            ShowImages::Fixed => (),
            _ => f.render_stateful_widget(StatefulImage::new().resize(resize), inner_area, state),
        };
        f.render_widget(block, area);
    }
}

fn ui(f: &mut Frame<'_>, app: &mut App) {
    let outer_block = Block::default()
        .borders(Borders::TOP)
        .title(app.title.as_str());

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(
            [
                Constraint::Percentage(app.split_percent),
                Constraint::Percentage(100 - app.split_percent),
            ]
            .as_ref(),
        )
        .split(outer_block.inner(f.area()));
    f.render_widget(outer_block, f.area());

    let left_chunks = vertical_layout().split(chunks[0]);
    let right_chunks = vertical_layout().split(chunks[1]);

    let block_left_top = block("Fixed");
    let area = block_left_top.inner(left_chunks[0]);
    f.render_widget(
        paragraph(app.background.as_str()).style(Color::Yellow),
        area,
    );
    f.render_widget(block_left_top, left_chunks[0]);
    match app.show_images {
        ShowImages::Resized => {}
        _ => {
            let image = Image::new(&app.image_static);
            // Let it be surrounded by styled text.
            let offset_area = Rect {
                x: area.x + 1,
                y: area.y + 1,
                width: area.width.saturating_sub(2),
                height: area.height.saturating_sub(2),
            };
            f.render_widget(image, offset_area);
        }
    }

    let chunks_left_bottom = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
        .split(left_chunks[1]);

    app.render_resized_image(f, Resize::Crop(None), chunks_left_bottom[0]);
    app.render_resized_image(f, Resize::Scale(None), chunks_left_bottom[1]);
    app.render_resized_image(f, Resize::Fit(None), right_chunks[0]);

    let block_right_bottom = block("Help");
    let area = block_right_bottom.inner(right_chunks[1]);
    f.render_widget(
        paragraph(vec![
            Line::from("Key bindings:"),
            Line::from("H/L: resize"),
            Line::from(format!(
                "i: cycle image protocols (current: {:?})",
                app.picker.protocol_type()
            )),
            Line::from("o: cycle image"),
            Line::from(format!("t: toggle ({:?})", app.show_images)),
            Line::from(format!("Font size: {:?}", app.picker.font_size())),
        ]),
        area,
    );
    f.render_widget(block_right_bottom, right_chunks[1]);
}

fn paragraph<'a, T: Into<Text<'a>>>(str: T) -> Paragraph<'a> {
    Paragraph::new(str).wrap(Wrap { trim: true })
}

fn vertical_layout() -> Layout {
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
}

fn block(name: &str) -> Block<'_> {
    Block::default().borders(Borders::ALL).title(name)
}
