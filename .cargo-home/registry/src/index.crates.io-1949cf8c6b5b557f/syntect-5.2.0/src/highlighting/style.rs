// Code based on [https://github.com/defuz/sublimate/blob/master/src/core/syntax/scope.rs](https://github.com/defuz/sublimate/blob/master/src/core/syntax/scope.rs)
// released under the MIT license by @defuz
use bitflags::bitflags;
use serde_derive::{Deserialize, Serialize};

/// Foreground and background colors, with font style
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Style {
    /// Foreground color
    pub foreground: Color,
    /// Background color
    pub background: Color,
    /// Style of the font
    pub font_style: FontStyle,
}

/// A change to a [`Style`] applied incrementally by a theme rule
///
/// Fields left empty (as `None`) will not modify the corresponding field on a `Style`
///
/// [`Style`]: struct.Style.html
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StyleModifier {
    /// Foreground color
    pub foreground: Option<Color>,
    /// Background color
    pub background: Option<Color>,
    /// Style of the font
    pub font_style: Option<FontStyle>,
}

/// RGBA color, directly from the theme
///
/// Because these numbers come directly from the theme, you might have to do your own color space
/// conversion if you're outputting a different color space from the theme. This can be a problem
/// because some Sublime themes use sRGB and some don't. This is specified in an attribute syntect
/// doesn't parse yet.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Color {
    /// Red component
    pub r: u8,
    /// Green component
    pub g: u8,
    /// Blue component
    pub b: u8,
    /// Alpha (transparency) component
    pub a: u8,
}

// More compact alternate debug representation by not using a separate line for each color field,
// also adapts the default debug representation to match.
impl std::fmt::Debug for Color {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let Color { r, g, b, a } = self;
        if f.alternate() {
            // when formatted with "{:#?}"
            write!(
                f,
                "Color {{ r/g/b/a: {: >3}/{: >3}/{: >3}/{: >3} }}",
                r, g, b, a
            )
        } else {
            // when formatted with "{:?}"
            write!(f, "Color {{ r/g/b/a: {}/{}/{}/{} }}", r, g, b, a)
        }
    }
}

bitflags! {
    /// The color-independent styling of a font - i.e. bold, italicized, and/or underlined
    #[derive(Serialize, Deserialize)]
    pub struct FontStyle: u8 {
        /// Bold font style
        const BOLD = 1;
        /// Underline font style
        const UNDERLINE = 2;
        /// Italic font style
        const ITALIC = 4;
    }
}

impl Color {
    /// The color black (`#000000`)
    pub const BLACK: Color = Color {
        r: 0x00,
        g: 0x00,
        b: 0x00,
        a: 0xFF,
    };

    /// The color white (`#FFFFFF`)
    pub const WHITE: Color = Color {
        r: 0xFF,
        g: 0xFF,
        b: 0xFF,
        a: 0xFF,
    };
}

impl Style {
    /// Applies a change to this style, yielding a new changed style
    pub fn apply(&self, modifier: StyleModifier) -> Style {
        Style {
            foreground: modifier.foreground.unwrap_or(self.foreground),
            background: modifier.background.unwrap_or(self.background),
            font_style: modifier.font_style.unwrap_or(self.font_style),
        }
    }
}

impl Default for Style {
    fn default() -> Style {
        Style {
            foreground: Color::BLACK,
            background: Color::WHITE,
            font_style: FontStyle::empty(),
        }
    }
}

impl StyleModifier {
    /// Applies the other modifier to this one, creating a new modifier.
    ///
    /// Values in `other` are preferred.
    pub fn apply(&self, other: StyleModifier) -> StyleModifier {
        StyleModifier {
            foreground: other.foreground.or(self.foreground),
            background: other.background.or(self.background),
            font_style: other.font_style.or(self.font_style),
        }
    }
}

impl Default for FontStyle {
    fn default() -> FontStyle {
        FontStyle::empty()
    }
}
