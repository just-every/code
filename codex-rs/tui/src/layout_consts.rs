// Shared layout constants to keep composer and history wrapping aligned.

// Outer horizontal padding applied by BottomPane around the composer area.
pub const COMPOSER_OUTER_HPAD: u16 = 1;
// Border width around the composer input block (left + right).
pub const COMPOSER_BORDER_WIDTH: u16 = 1;
// Inner horizontal padding inside the composer input block (left + right).
pub const COMPOSER_INNER_HPAD: u16 = 1;

// When computing content width from the full bottom pane width, subtract:
// outer hpad (×2) + border (×2) + inner hpad (×2) = 6 columns total.
pub const COMPOSER_CONTENT_WIDTH_OFFSET: u16 =
    (COMPOSER_OUTER_HPAD * 2) + (COMPOSER_BORDER_WIDTH * 2) + (COMPOSER_INNER_HPAD * 2);

// When computing content width from an area that already excludes the outer
// padding (i.e., within BottomPane), subtract only border + inner padding.
// Currently unused; keep for reference and prefix with underscore to avoid warnings.
pub const _COMPOSER_INNER_AREA_OFFSET: u16 =
    (COMPOSER_BORDER_WIDTH * 2) + (COMPOSER_INNER_HPAD * 2); // = 4

// Extra right padding for user history cells so wrapped lines match the
// composer’s visual width. Keep this in sync with composer’s inner layout.
pub const USER_HISTORY_RIGHT_PAD: u16 = 2;
