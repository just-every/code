# Every Code TUI Theme Configuration

The Every Code TUI now supports customizable color themes! You can use predefined themes or create your own custom color scheme.

## Configuration

Add theme configuration to your `~/.code/config.toml` file:

### Using Predefined Themes

```toml
[tui.theme]
name = "light-photon"  # Default theme
```

### Available Predefined Themes

#### Light Themes
- **light-photon** (default): Clean professional light theme with confident blue accent - great for bright environments
- **light-prism-rainbow**: Vibrant rainbow accents with blue, violet, and teal highlights
- **light-vivid-triad**: Striking cyan, pink, and amber color triad for a modern look
- **light-porcelain**: Refined porcelain tones with sophisticated blue and purple accents
- **light-sandbar**: Warm sandy beach colors with amber and muted teal highlights
- **light-glacier**: Cool glacier blues with purple accents for a fresh, crisp appearance

#### Dark Themes
- **dark-carbon-night**: Sleek modern dark theme with blue accents - perfect for long coding sessions
- **dark-shinobi-dusk**: Japanese-inspired twilight theme with soothing purple and green tones
- **dark-oled-black-pro**: True black background for OLED displays with vibrant neon accents
- **dark-amber-terminal**: Retro amber CRT monitor aesthetic - nostalgic monochrome orange
- **dark-aurora-flux**: Northern lights inspired with cool blues and greens
- **dark-charcoal-rainbow**: High-contrast accessible theme with rainbow accents on dark gray
- **dark-zen-garden**: Calm, peaceful theme with mint and lavender tones
- **dark-paper-light-pro**: Premium paper-like dark theme for comfortable extended use

### Custom Color Configuration

You can override individual colors while using a base theme:

```toml
[tui.theme]
name = "carbon-night"  # Base theme

[tui.theme.colors]
primary = "#25c2ff"      # Override the primary color
secondary = "#a9e69e"    # Green
background = "#000000"   # Black
foreground = "#ffffff"   # White
border = "#404040"       # Dark gray
border_focused = "#00ffff"  # Cyan
text = "#ffffff"         # White
text_dim = "#808080"     # Gray
text_bright = "#ffffff"  # Bright white
success = "#00ff00"      # Green
warning = "#ffff00"      # Yellow
error = "#ff0000"        # Red
info = "#00ffff"         # Cyan
spinner = "#404040"      # Dark gray
progress = "#00ffff"     # Cyan
```

### Complete Custom Theme

For a fully custom theme:

```toml
[tui.theme]
name = "custom"

[tui.theme.colors]
# Primary colors
primary = "#ff79c6"      # Pink
secondary = "#50fa7b"    # Green
background = "#282a36"   # Dark purple
foreground = "#f8f8f2"   # Off-white

# UI elements
border = "#44475a"       # Purple-gray
border_focused = "#bd93f9"  # Purple
selection = "#44475a"    # Purple-gray
cursor = "#f8f8f2"       # Off-white

# Status colors
success = "#50fa7b"      # Green
warning = "#f1fa8c"      # Yellow
error = "#ff5555"        # Red
info = "#8be9fd"         # Cyan

# Text colors
text = "#f8f8f2"         # Off-white
text_dim = "#6272a4"     # Comment blue
text_bright = "#ffffff"  # Pure white

# Special colors
keyword = "#ff79c6"      # Pink
string = "#f1fa8c"       # Yellow
comment = "#6272a4"      # Comment blue
function = "#50fa7b"     # Green

# Animation colors
spinner = "#6272a4"      # Comment blue
progress = "#bd93f9"     # Purple
```

## Color Format

Colors can be specified in the following formats:

### Hex Colors
- `"#rrggbb"` - Standard 6-digit hex color (e.g., `"#ff79c6"`)

### Named Colors
- `"black"`, `"red"`, `"green"`, `"yellow"`, `"blue"`, `"magenta"`, `"cyan"`, `"white"`
- `"gray"` or `"grey"`, `"darkgray"` or `"darkgrey"`
- `"lightred"`, `"lightgreen"`, `"lightyellow"`, `"lightblue"`, `"lightmagenta"`, `"lightcyan"`

## Examples

### Example 1: Dark Shinobi Dusk Theme
```toml
[tui.theme]
name = "dark-shinobi-dusk"
```

### Example 2: Light Photon with Custom Accent
```toml
[tui.theme]
name = "light-photon"

[tui.theme.colors]
primary = "#007acc"      # VS Code blue
border_focused = "#007acc"
info = "#007acc"
```

### Example 3: Dark Amber Terminal for Retro Feel
```toml
[tui.theme]
name = "dark-amber-terminal"

[tui.theme.colors]
primary = "#ffa500"      # Make it more orange
success = "#ffff00"      # Bright yellow
error = "#ff6600"        # Orange-red
```

### Example 4: Custom Theme - Solarized-inspired
```toml
[tui.theme]
name = "custom"

[tui.theme.colors]
background = "#002b36"   # Base03
foreground = "#839496"   # Base0
primary = "#268bd2"      # Blue
secondary = "#859900"    # Green
border = "#073642"       # Base02
border_focused = "#268bd2"  # Blue
text = "#839496"         # Base0
text_dim = "#586e75"     # Base01
text_bright = "#93a1a1"  # Base1
success = "#859900"      # Green
warning = "#b58900"      # Yellow
error = "#dc322f"        # Red
info = "#2aa198"         # Cyan
```

## Tips

1. Start with a predefined theme that's close to what you want, then customize individual colors
2. Use a color picker tool to find exact hex values
3. Test your theme in different lighting conditions
4. Consider accessibility - ensure sufficient contrast between text and background colors
5. The theme takes effect immediately when you save the config file and restart Codex

## Color Meanings

- **primary**: Main accent color, used for important UI elements
- **secondary**: Secondary accent color
- **background**: Main background color
- **foreground**: Default text color
- **border**: Normal border color
- **border_focused**: Border color when element has focus
- **selection**: Background color for selected text
- **cursor**: Cursor color
- **success**: Success messages and indicators
- **warning**: Warning messages
- **error**: Error messages
- **info**: Informational messages
- **text**: Normal text
- **text_dim**: Dimmed/secondary text
- **text_bright**: Bright/emphasized text
- **spinner**: Loading spinner color
- **progress**: Progress bar color
