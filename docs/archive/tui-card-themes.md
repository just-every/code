# TUI Card Theme System

The TUI now exposes a shared card theme catalog that any surface (Agent, Browser, Auto Drive, Search, etc.) can use to present consistent gradients, palettes, and reveal animations.

## 1. Import the theme helpers

```rust
use crate::card_theme::{self, CardThemeDefinition};
use crate::gradient_background::{GradientBackground, RevealRender};
```

`card_theme` owns the data model (gradients, palettes, animation metadata). `gradient_background` handles all gradient and reveal rendering.

## 2. Pick a theme

Built-in helpers return `CardThemeDefinition` values that carry the display name and styling bundle:

```rust
let search_dark = card_theme::search_dark_theme();
let auto_drive_light = card_theme::auto_drive_light_theme();
let all = card_theme::theme_catalog();
```

Use `dark_theme_catalog()`, `light_theme_catalog()`, or `auto_drive_theme_catalog()` when you need grouped collections (for example, to build preview pickers).

To attach body content, convert the definition into a `CardPreviewSpec`:

```rust
const BODY: &[&str] = &["LLM output in a card", "Second paragraph..."];
let preview = theme.preview(BODY);
```

## 3. Render a gradient card

`GradientBackground::render` applies either a static gradient or an animated reveal. Pass the palette’s text color as the foreground; the helper keeps text legible while animating.

```rust
let reveal = preview
    .theme
    .reveal
    .map(|config| RevealRender {
        progress: animation.progress(),
        variant: config.variant,
        intro_light: preview.name.contains("Light"),
    });

GradientBackground::render(
    buf,
    area,
    &preview.theme.gradient,
    preview.theme.palette.text,
    reveal,
);
```

After painting the background, write title/body/footer text with the palette colors (`palette.border`, `palette.title`, etc.).

## 4. Drive animations when available

Themes that define a `RevealConfig` opt into animated reveals. Track an animation progress value (`0.0..=1.0`) of your choosing and pass it through `RevealRender`. Themes with `reveal: None` render statically; supply `None` to `GradientBackground::render` in that case.

## 5. Examples by surface

- **Search** – `search_dark_theme()` / `search_light_theme()`
- **Auto Drive** – `auto_drive_dark_theme()` / `auto_drive_light_theme()` (the only themes with `reveal` animation data)
- **Agent (read-only)** – `agent_read_only_dark_theme()` / `agent_read_only_light_theme()`
- **Agent (write)** – `agent_write_dark_theme()` / `agent_write_light_theme()`
- **Browser** – `browser_dark_theme()` / `browser_light_theme()`

## Tips

- Keep body copy narrow (`textwrap` width ≈ `area.width - padding`).
- When you introduce a new theme, add it to `card_theme.rs` so every surface can reuse it.
- Prefer `CardThemeDefinition::preview` over ad-hoc palette wiring; this keeps names, palettes, and animations aligned.
