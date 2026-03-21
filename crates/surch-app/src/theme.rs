use gpui::*;

pub struct SurchTheme;

impl SurchTheme {
    // === Backgrounds (4-tier depth hierarchy) ===

    /// Sidebar — deepest layer
    pub fn bg_sidebar() -> Hsla {
        hsla(0.63, 0.13, 0.09, 1.0)
    }

    /// Preview pane, main content area
    pub fn bg_primary() -> Hsla {
        hsla(0.63, 0.13, 0.11, 1.0)
    }

    /// Search panel background
    pub fn bg_secondary() -> Hsla {
        hsla(0.63, 0.13, 0.14, 1.0)
    }

    /// Input fields, file group headers — raised surfaces
    pub fn bg_surface() -> Hsla {
        hsla(0.63, 0.13, 0.17, 1.0)
    }

    /// Hover state for interactive rows
    pub fn bg_hover() -> Hsla {
        hsla(0.63, 0.10, 0.20, 1.0)
    }

    /// Selected result row
    pub fn bg_selected() -> Hsla {
        hsla(0.58, 0.25, 0.18, 1.0)
    }

    /// Preview focus line highlight
    pub fn bg_focus_line() -> Hsla {
        hsla(0.15, 0.40, 0.22, 0.45)
    }

    // === Text ===

    /// Main body text
    pub fn text_primary() -> Hsla {
        hsla(0.58, 0.10, 0.85, 1.0)
    }

    /// Headings, file names — brighter
    pub fn text_heading() -> Hsla {
        hsla(0.58, 0.10, 0.95, 1.0)
    }

    /// Labels, line numbers, status — WCAG AA compliant (≥4.5:1 on dark bg)
    pub fn text_secondary() -> Hsla {
        hsla(0.58, 0.08, 0.68, 1.0)
    }

    /// Placeholders, disabled — readable (≥3:1 on dark bg)
    pub fn text_muted() -> Hsla {
        hsla(0.58, 0.05, 0.52, 1.0)
    }

    /// Match highlight foreground
    pub fn text_match() -> Hsla {
        hsla(0.10, 0.90, 0.70, 1.0)
    }

    // === Accent & Semantic ===

    /// Primary accent (buttons, active indicators)
    pub fn accent() -> Hsla {
        hsla(0.58, 0.60, 0.55, 1.0)
    }

    /// Button hover
    pub fn accent_hover() -> Hsla {
        hsla(0.58, 0.65, 0.62, 1.0)
    }

    /// Match highlight background
    pub fn match_bg() -> Hsla {
        hsla(0.10, 0.70, 0.35, 0.55)
    }

    /// Toggle button active background (accent at 30% alpha)
    pub fn toggle_active_bg() -> Hsla {
        hsla(0.58, 0.60, 0.55, 0.30)
    }

    // === Borders ===

    /// Panel dividers, subtle borders
    pub fn border() -> Hsla {
        hsla(0.63, 0.10, 0.18, 1.0)
    }

    /// Focused input border (used by future focus ring styling)
    #[allow(dead_code)]
    pub fn border_focus() -> Hsla {
        hsla(0.58, 0.60, 0.55, 0.60)
    }
}
