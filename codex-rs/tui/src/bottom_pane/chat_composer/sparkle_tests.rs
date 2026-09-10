use super::super::tests::new_test_composer;
use super::*;
use crate::render::renderable::Renderable;
use crate::terminal_palette::with_test_default_colors;
use crate::terminal_probe::DefaultColors;
use codex_protocol::openai_models::ReasoningEffort;
use pretty_assertions::assert_eq;
use ratatui::style::Style;

fn new_test_sparkle() -> Sparkle {
    Sparkle {
        model: "astra".to_string(),
        whimsy: true,
        animations: true,
        started: Instant::now(),
    }
}

#[test]
fn sparkle_keeps_the_existing_composer_layout() {
    with_test_default_colors(
        DefaultColors {
            fg: (230, 216, 255),
            bg: (36, 27, 53),
        },
        || {
            let (mut composer, _rx) = new_test_composer();
            composer.set_text_content("Explore the night sky".to_string(), Vec::new(), Vec::new());
            let area = Rect::new(
                /*x*/ 0,
                /*y*/ 0,
                /*width*/ 60,
                composer.desired_height(/*width*/ 60),
            );
            let [surface, _, _, _] = composer.layout_areas(area);
            let mut buffer = Buffer::empty(area);
            composer.render(area, &mut buffer);
            render_stars(
                surface,
                composer.cursor_pos(area),
                Duration::ZERO,
                /*foreground*/ (230, 216, 255),
                &mut buffer,
            );
            // Snapshot the composer layout without freezing the star distribution.
            let rows = (area.y..area.bottom())
                .map(|y| {
                    (area.x..area.right())
                        .map(|x| {
                            let symbol = buffer[(x, y)].symbol();
                            if DOTS.contains(&symbol) { " " } else { symbol }
                        })
                        .collect::<String>()
                })
                .collect::<Vec<_>>()
                .join("\n");
            insta::assert_snapshot!("astra_current_composer", rows);
        },
    );
}

#[test]
fn sparkle_preserves_content_cursor_and_background() {
    let area = Rect::new(
        /*x*/ 0, /*y*/ 0, /*width*/ 80, /*height*/ 3,
    );
    let mut before = Buffer::empty(area);
    before.set_style(area, Style::default().bg(rgb_color((36, 27, 53))));
    before[(0, 0)].set_symbol("界");
    before[(2, 0)].set_symbol("!");
    before[(4, 0)].set_symbol("✦");
    before[(5, 0)].set_style(Style::default().reversed());
    before[(6, 0)].set_diff_option(CellDiffOption::Skip);
    let protected = [(0, 0), (1, 0), (2, 0), (3, 0), (4, 0), (5, 0), (6, 0)];
    let mut seen = std::collections::HashMap::new();
    for tick in 0..80 {
        let mut after = before.clone();
        render_stars(
            area,
            Some((3, 0)),
            FRAME_TICK * tick,
            /*foreground*/ (230, 216, 255),
            &mut after,
        );
        assert_eq!(
            protected.map(|p| after[p].clone()),
            protected.map(|p| before[p].clone())
        );
        for (index, cell) in after.content.iter().enumerate() {
            assert_eq!(cell.bg, before.content[index].bg);
            if DOTS.contains(&cell.symbol())
                && let Some(previous) = seen.insert(index, cell.symbol().to_string())
            {
                assert_eq!(cell.symbol(), previous);
            }
        }
    }
    assert!(!seen.is_empty());
}

#[test]
fn stars_fade_using_the_custom_terminal_foreground() {
    for colors in [
        DefaultColors {
            fg: (230, 216, 255),
            bg: (36, 27, 53),
        },
        DefaultColors {
            fg: (101, 123, 131),
            bg: (253, 246, 227),
        },
    ] {
        with_test_default_colors(colors, || {
            let area = Rect::new(
                /*x*/ 0, /*y*/ 0, /*width*/ 80, /*height*/ 3,
            );
            let mut shades: std::collections::HashMap<usize, std::collections::HashSet<Color>> =
                std::collections::HashMap::new();
            for tick in 0..40 {
                let mut buffer = Buffer::empty(area);
                buffer.set_style(area, Style::default().bg(rgb_color(colors.bg)));
                render_stars(
                    area,
                    /*cursor*/ None,
                    FRAME_TICK * tick,
                    colors.fg,
                    &mut buffer,
                );
                for (index, cell) in buffer.content.iter().enumerate() {
                    if DOTS.contains(&cell.symbol()) {
                        let Color::Rgb(r, g, b) = cell.fg else {
                            panic!("expected RGB fade")
                        };
                        shades.entry(index).or_default().insert(cell.fg);
                        for (actual, fg, bg) in [
                            (r, colors.fg.0, colors.bg.0),
                            (g, colors.fg.1, colors.bg.1),
                            (b, colors.fg.2, colors.bg.2),
                        ] {
                            assert!((fg.min(bg)..=fg.max(bg)).contains(&actual));
                        }
                        assert_eq!(cell.bg, rgb_color(colors.bg));
                    }
                }
            }
            assert!(shades.values().any(|colors| colors.len() > 3));
        });
    }
}

#[test]
fn model_changes_and_disable_setting_control_sparkle() {
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let mut pane = BottomPane::new(crate::bottom_pane::BottomPaneParams {
        app_event_tx: crate::app_event_sender::AppEventSender::new(tx),
        frame_requester: crate::tui::FrameRequester::test_dummy(),
        has_input_focus: true,
        enhanced_keys_supported: false,
        placeholder_text: String::new(),
        disable_paste_burst: true,
        animations_enabled: true,
        skills: None,
    });
    with_test_default_colors(
        DefaultColors {
            fg: (230, 216, 255),
            bg: (36, 27, 53),
        },
        || {
            let mut settings = Tui {
                whimsy: true,
                animations: true,
                ..Tui::default()
            };
            pane.set_astra_sparkle("gpt-6-astra", &settings);
            let started = pane
                .composer
                .astra_sparkle
                .as_ref()
                .map(|sparkle| sparkle.started);
            for model in ["astra", "ASTRA-preview", "openai/astra-2026-09-01"] {
                pane.set_astra_sparkle(model, &settings);
                assert_eq!(
                    pane.composer
                        .astra_sparkle
                        .as_ref()
                        .and_then(Sparkle::enabled_foreground),
                    Some((230, 216, 255)),
                );
                assert_eq!(
                    pane.composer
                        .astra_sparkle
                        .as_ref()
                        .map(|sparkle| sparkle.started),
                    started
                );
            }
            for model in [
                "gpt-5.6-sol",
                "astral",
                "castrated",
                "astra2",
                "astra_preview",
            ] {
                pane.set_astra_sparkle(model, &settings);
                assert_eq!(
                    pane.composer
                        .astra_sparkle
                        .as_ref()
                        .and_then(Sparkle::enabled_foreground),
                    None,
                    "{model}",
                );
            }
            for (whimsy, animations) in [(false, true), (true, false), (false, false), (true, true)]
            {
                settings.whimsy = whimsy;
                settings.animations = animations;
                pane.set_astra_sparkle("astra", &settings);
                assert_eq!(
                    pane.composer
                        .astra_sparkle
                        .as_ref()
                        .and_then(Sparkle::enabled_foreground),
                    (whimsy && animations).then_some((230, 216, 255)),
                );
            }
        },
    );
}

#[test]
fn sparkle_renders_with_effort_bursts_and_pauses_for_popups() {
    with_test_default_colors(
        DefaultColors {
            fg: (230, 216, 255),
            bg: (36, 27, 53),
        },
        || {
            let (mut composer, _rx) = new_test_composer();
            composer.astra_sparkle = Some(new_test_sparkle());
            composer.set_text_content("hello 界".to_string(), Vec::new(), Vec::new());
            composer.set_active_reasoning_effort_baseline(Some(&ReasoningEffort::High));
            let area = Rect::new(
                /*x*/ 0,
                /*y*/ 0,
                /*width*/ 80,
                composer.desired_height(/*width*/ 80),
            );
            for effort in [ReasoningEffort::Max, ReasoningEffort::Ultra] {
                composer
                    .set_active_reasoning_effort(Some(&effort), /*animations_enabled*/ true);
                let started = composer.astra_sparkle.take();
                let mut before = Buffer::empty(area);
                composer.render(area, &mut before);
                composer.astra_sparkle = started;
                let mut after = Buffer::empty(area);
                composer.render(area, &mut after);
                let text = after
                    .content
                    .iter()
                    .map(ratatui::buffer::Cell::symbol)
                    .collect::<String>();
                assert!(text.contains("hello 界"));
                assert!(
                    after
                        .content
                        .iter()
                        .any(|cell| DOTS.contains(&cell.symbol()))
                );
                assert_eq!(
                    composer.cursor_pos(area).map(|p| after[p].clone()),
                    composer.cursor_pos(area).map(|p| before[p].clone())
                );
                assert!(composer.effort_ignition.is_some());
            }
            composer.set_text_content("/mod".to_string(), Vec::new(), Vec::new());
            composer.draft.textarea.set_cursor(/*pos*/ 4);
            composer.sync_popups();
            assert!(!matches!(composer.popups.active, ActivePopup::None));
            let mut buffer = Buffer::empty(area);
            composer.render_sparkle(area, /*cursor*/ None, &mut buffer);
            assert_eq!(buffer, Buffer::empty(area));
        },
    );
}

#[test]
fn sparkle_waits_for_terminal_colors() {
    let (mut composer, _rx) = new_test_composer();
    composer.astra_sparkle = Some(new_test_sparkle());
    let area = Rect::new(
        /*x*/ 0, /*y*/ 0, /*width*/ 80, /*height*/ 3,
    );
    let mut buffer = Buffer::empty(area);
    composer.render_sparkle(area, /*cursor*/ None, &mut buffer);
    assert_eq!(buffer, Buffer::empty(area));
    with_test_default_colors(
        DefaultColors {
            fg: (230, 216, 255),
            bg: (36, 27, 53),
        },
        || {
            buffer.set_style(area, Style::default().bg(rgb_color((36, 27, 53))));
            composer.render_sparkle(area, /*cursor*/ None, &mut buffer);
            assert!(
                buffer
                    .content
                    .iter()
                    .any(|cell| DOTS.contains(&cell.symbol()))
            );
        },
    );
}

#[test]
fn sparkle_preserves_voice_indicator_styles() {
    use crate::bottom_pane::voice_strip::VoiceStripPhase;
    use crate::bottom_pane::voice_strip::VoiceStripState;
    use crate::tui::FrameRequester;

    with_test_default_colors(
        DefaultColors {
            fg: (230, 216, 255),
            bg: (36, 27, 53),
        },
        || {
            let (mut composer, _rx) = new_test_composer();
            composer.set_voice_strip(
                Some(VoiceStripState {
                    mute_hint: None,
                    phase: VoiceStripPhase::Active,
                    microphone_live: true,
                    microphone_muted: false,
                    microphone_history: vec![0, 37, 73, 110, 146, 255],
                    speaker_history: vec![255, 146, 110, 73, 37, 0],
                    activity: "listening",
                    animations: false,
                }),
                FrameRequester::test_dummy(),
            );
            let area = Rect::new(
                /*x*/ 0,
                /*y*/ 0,
                /*width*/ 60,
                composer.desired_height(/*width*/ 60),
            );
            let mut baseline = Buffer::empty(area);
            composer.render(area, &mut baseline);
            let mut saw_stars = false;
            for tick in 0..80 {
                composer.astra_sparkle = Some(Sparkle {
                    started: Instant::now() - FRAME_TICK * tick,
                    ..new_test_sparkle()
                });
                let mut actual = Buffer::empty(area);
                composer.render(area, &mut actual);
                saw_stars |= actual
                    .content
                    .iter()
                    .any(|cell| DOTS.contains(&cell.symbol()));
                // Compare whole cells: an unchanged glyph can still inherit a star's color.
                let content = baseline
                    .content
                    .iter()
                    .enumerate()
                    .filter(|(_, cell)| cell.symbol() != " ")
                    .map(|(index, _)| actual.content[index].clone())
                    .collect::<Vec<_>>();
                let expected = baseline
                    .content
                    .iter()
                    .filter(|cell| cell.symbol() != " ")
                    .cloned()
                    .collect::<Vec<_>>();
                assert_eq!(content, expected, "sparkle frame {tick}");
            }
            assert!(saw_stars);
            insta::assert_snapshot!(
                "astra_voice_composer",
                baseline
                    .content
                    .chunks(60)
                    .map(|row| row
                        .iter()
                        .map(ratatui::buffer::Cell::symbol)
                        .collect::<String>())
                    .collect::<Vec<_>>()
                    .join("\n")
            );
        },
    );
}
