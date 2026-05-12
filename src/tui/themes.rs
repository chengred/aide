#![allow(dead_code)]

use ratatui::style::Color;

/// Theme configuration for the TUI
#[derive(Debug, Clone)]
pub struct Theme {
    pub bg: Color,
    pub surface: Color,
    pub primary: Color,
    pub secondary: Color,
    pub accent: Color,
    pub text: Color,
    pub dim_text: Color,
    pub success: Color,
    pub error: Color,
    pub warning: Color,
    pub user_bubble: Color,
    pub agent_bubble: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

impl Theme {
    pub fn dark() -> Self {
        Self {
            bg: Color::Rgb(18, 18, 24),
            surface: Color::Rgb(30, 30, 40),
            primary: Color::Rgb(99, 102, 241),
            secondary: Color::Rgb(55, 55, 75),
            accent: Color::Rgb(34, 197, 94),
            text: Color::White,
            dim_text: Color::Gray,
            success: Color::Rgb(34, 197, 94),
            error: Color::Rgb(239, 68, 68),
            warning: Color::Rgb(234, 179, 8),
            user_bubble: Color::Cyan,
            agent_bubble: Color::Rgb(168, 85, 247),
        }
    }

    pub fn light() -> Self {
        Self {
            bg: Color::Rgb(250, 250, 250),
            surface: Color::Rgb(240, 240, 245),
            primary: Color::Rgb(79, 70, 229),
            secondary: Color::Rgb(228, 228, 235),
            accent: Color::Rgb(22, 163, 74),
            text: Color::Black,
            dim_text: Color::DarkGray,
            success: Color::Rgb(22, 163, 74),
            error: Color::Rgb(220, 38, 38),
            warning: Color::Rgb(202, 138, 4),
            user_bubble: Color::Blue,
            agent_bubble: Color::Rgb(147, 51, 234),
        }
    }
}
