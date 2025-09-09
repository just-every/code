//! Spinner presets adapted from sindresorhus/cli-spinners (curated subset).
//! Each spinner has a name, frames, and an interval in milliseconds.

#[derive(Clone, Debug)]
pub struct Spinner {
    pub name: &'static str,
    pub frames: &'static [&'static str],
    pub interval_ms: u64,
}

// A curated, theme-friendly subset. Add more as desired.
static DIAMOND: Spinner = Spinner { name: "diamond", frames: &["◇", "◆", "◇"], interval_ms: 150 };
static DOTS: Spinner = Spinner { name: "dots", frames: &[
    "⠋","⠙","⠹","⠸","⠼","⠴","⠦","⠧","⠇","⠏"
], interval_ms: 80 };
static DOTS2: Spinner = Spinner { name: "dots2", frames: &[
    "⠋","⠙","⠚","⠞","⠖","⠦","⠴","⠲","⠳","⠓"
], interval_ms: 80 };
static LINE: Spinner = Spinner { name: "line", frames: &["-", "\\", "|", "/"], interval_ms: 100 };
static PIPE: Spinner = Spinner { name: "pipe", frames: &["┤", "┘", "┴", "└", "├", "┌", "┬", "┐"], interval_ms: 100 };
static STAR: Spinner = Spinner { name: "star", frames: &["✶", "✸", "✹", "✺", "✹", "✷"], interval_ms: 70 };
static SIMPLE_DOTS: Spinner = Spinner { name: "simpleDotsScrolling", frames: &[".  ", ".. ", "...", " ..", "  .", "   "], interval_ms: 200 };
static BOUNCING_BAR: Spinner = Spinner { name: "bouncingBar", frames: &[
    "[    ]","[=   ]","[==  ]","[=== ]","[ ===]","[  ==]","[   =]","[    ]","[   =]","[  ==]","[ ===]","[====]","[=== ]","[==  ]","[=   ]"
], interval_ms: 80 };
static BOUNCING_BALL: Spinner = Spinner { name: "bouncingBall", frames: &[
    "( ●    )","(  ●   )","(   ●  )","(    ● )","(     ●)","(    ● )","(   ●  )","(  ●   )","( ●    )","(●     )"
], interval_ms: 80 };
static TOGGLE: Spinner = Spinner { name: "toggle", frames: &["⊶", "⊷"], interval_ms: 120 };
static HAMBURGER: Spinner = Spinner { name: "hamburger", frames: &["☱", "☲", "☴"], interval_ms: 100 };
static GROW_VERT: Spinner = Spinner { name: "growVertical", frames: &["▁","▃","▄","▅","▆","▇","▆","▅","▄","▃"], interval_ms: 120 };
static ARROW3: Spinner = Spinner { name: "arrow3", frames: &["←","↖","↑","↗","→","↘","↓","↙"], interval_ms: 80 };
static CLOCK: Spinner = Spinner { name: "clock", frames: &["🕛","🕐","🕑","🕒","🕓","🕔","🕕","🕖","🕗","🕘","🕙","🕚"], interval_ms: 100 };

static ALL: &[&Spinner] = &[
    &DIAMOND,
    &DOTS,
    &DOTS2,
    &LINE,
    &PIPE,
    &STAR,
    &SIMPLE_DOTS,
    &BOUNCING_BAR,
    &BOUNCING_BALL,
    &TOGGLE,
    &HAMBURGER,
    &GROW_VERT,
    &ARROW3,
    &CLOCK,
];

pub fn default_name() -> &'static str { DIAMOND.name }

pub fn list() -> Vec<&'static str> { ALL.iter().map(|s| s.name).collect() }

pub fn get(name: &str) -> &'static Spinner {
    for s in ALL {
        if s.name.eq_ignore_ascii_case(name) { return s; }
    }
    // Fallback to default
    &DIAMOND
}

