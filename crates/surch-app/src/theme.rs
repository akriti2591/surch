use gpui::*;

pub struct SurchTheme;

impl SurchTheme {
    pub fn bg_primary() -> Hsla {
        hsla(0.0, 0.0, 0.12, 1.0) // Dark background
    }

    pub fn bg_secondary() -> Hsla {
        hsla(0.0, 0.0, 0.15, 1.0) // Slightly lighter
    }

    pub fn bg_sidebar() -> Hsla {
        hsla(0.0, 0.0, 0.10, 1.0) // Darkest
    }

    pub fn bg_hover() -> Hsla {
        hsla(0.0, 0.0, 0.20, 1.0)
    }

    pub fn bg_selected() -> Hsla {
        hsla(0.58, 0.6, 0.25, 1.0) // Blue-ish highlight
    }

    pub fn text_primary() -> Hsla {
        hsla(0.0, 0.0, 0.90, 1.0)
    }

    pub fn text_secondary() -> Hsla {
        hsla(0.0, 0.0, 0.60, 1.0)
    }

    pub fn text_match() -> Hsla {
        hsla(0.1, 0.9, 0.6, 1.0) // Orange highlight for matches
    }

    pub fn border() -> Hsla {
        hsla(0.0, 0.0, 0.25, 1.0)
    }

    pub fn accent() -> Hsla {
        hsla(0.58, 0.7, 0.55, 1.0) // Blue accent
    }
}
