use crate::ColorsConfig;
use ratatui::style::Color;

/// Convert a ratatui Color to a termcolor Color.
pub fn ratatui_to_termcolor(c: Color) -> termcolor::Color {
    match c {
        Color::Black => termcolor::Color::Black,
        Color::Red => termcolor::Color::Red,
        Color::Green => termcolor::Color::Green,
        Color::Yellow => termcolor::Color::Yellow,
        Color::Blue => termcolor::Color::Blue,
        Color::Magenta => termcolor::Color::Magenta,
        Color::Cyan => termcolor::Color::Cyan,
        Color::Gray => termcolor::Color::Ansi256(7),
        Color::DarkGray => termcolor::Color::Ansi256(8),
        Color::White => termcolor::Color::White,
        Color::LightRed => termcolor::Color::Ansi256(9),
        Color::LightGreen => termcolor::Color::Ansi256(10),
        Color::LightYellow => termcolor::Color::Ansi256(11),
        Color::LightBlue => termcolor::Color::Ansi256(12),
        Color::LightMagenta => termcolor::Color::Ansi256(13),
        Color::LightCyan => termcolor::Color::Ansi256(14),
        Color::Indexed(n) => termcolor::Color::Ansi256(n),
        Color::Rgb(r, g, b) => termcolor::Color::Ansi256(rgb_to_ansi256(r, g, b)),
        _ => termcolor::Color::White,
    }
}

fn rgb_to_ansi256(r: u8, g: u8, b: u8) -> u8 {
    let ri = (r as u16 * 5 / 255) as u8;
    let gi = (g as u16 * 5 / 255) as u8;
    let bi = (b as u16 * 5 / 255) as u8;
    16 + 36 * ri + 6 * gi + bi
}

pub fn status_color(status: &str, colors: &ColorsConfig) -> ratatui::style::Color {
    match status {
        "in-progress" => colors.status.in_progress,
        "open" => colors.status.open,
        "blocked" => colors.status.blocked,
        "backlog" => colors.status.backlog,
        "idea" => colors.status.idea,
        "done" => colors.status.done,
        _ => Color::White,
    }
}

pub fn priority_color(priority: &str, colors: &ColorsConfig) -> ratatui::style::Color {
    match priority {
        "high" => colors.priority.high,
        "medium" => colors.priority.medium,
        "low" => colors.priority.low,
        _ => Color::White,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Color;

    #[test]
    fn ratatui_to_termcolor_basic_colors() {
        assert_eq!(ratatui_to_termcolor(Color::Red), termcolor::Color::Red);
        assert_eq!(ratatui_to_termcolor(Color::Green), termcolor::Color::Green);
        assert_eq!(ratatui_to_termcolor(Color::Blue), termcolor::Color::Blue);
        assert_eq!(
            ratatui_to_termcolor(Color::Yellow),
            termcolor::Color::Yellow
        );
        assert_eq!(ratatui_to_termcolor(Color::Cyan), termcolor::Color::Cyan);
        assert_eq!(
            ratatui_to_termcolor(Color::Magenta),
            termcolor::Color::Magenta
        );
        assert_eq!(ratatui_to_termcolor(Color::Black), termcolor::Color::Black);
        assert_eq!(ratatui_to_termcolor(Color::White), termcolor::Color::White);
    }

    #[test]
    fn ratatui_to_termcolor_gray() {
        assert_eq!(
            ratatui_to_termcolor(Color::Gray),
            termcolor::Color::Ansi256(7)
        );
        assert_eq!(
            ratatui_to_termcolor(Color::DarkGray),
            termcolor::Color::Ansi256(8)
        );
    }

    #[test]
    fn ratatui_to_termcolor_indexed() {
        assert_eq!(
            ratatui_to_termcolor(Color::Indexed(42)),
            termcolor::Color::Ansi256(42)
        );
        assert_eq!(
            ratatui_to_termcolor(Color::Indexed(0)),
            termcolor::Color::Ansi256(0)
        );
        assert_eq!(
            ratatui_to_termcolor(Color::Indexed(255)),
            termcolor::Color::Ansi256(255)
        );
    }

    #[test]
    fn ratatui_to_termcolor_rgb() {
        // RGB should convert to an ANSI256 index
        let result = ratatui_to_termcolor(Color::Rgb(255, 0, 0));
        match result {
            termcolor::Color::Ansi256(_) => {} // expected
            _ => panic!("expected Ansi256, got {:?}", result),
        }
    }

    #[test]
    fn status_color_maps_correctly() {
        let colors = ColorsConfig::default();
        assert_eq!(status_color("open", &colors), Color::Yellow);
        assert_eq!(status_color("in-progress", &colors), Color::Magenta);
        assert_eq!(status_color("blocked", &colors), Color::Red);
        assert_eq!(status_color("backlog", &colors), Color::Blue);
        assert_eq!(status_color("idea", &colors), Color::Cyan);
        assert_eq!(status_color("done", &colors), Color::Green);
        assert_eq!(status_color("unknown", &colors), Color::White);
    }

    #[test]
    fn priority_color_maps_correctly() {
        let colors = ColorsConfig::default();
        assert_eq!(priority_color("high", &colors), Color::Red);
        assert_eq!(priority_color("medium", &colors), Color::Yellow);
        assert_eq!(priority_color("low", &colors), Color::DarkGray);
        assert_eq!(priority_color("unknown", &colors), Color::White);
    }
}
