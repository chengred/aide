use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::tui::app::{App, InputMode};

/// Draw the top status bar
pub fn draw_status_bar(f: &mut Frame, area: Rect, app: &App) {
    let left = Span::styled(
        format!(" RustCC | {} | {}", app.provider, app.model),
        Style::default().fg(Color::White).bg(Color::Rgb(55, 55, 75)),
    );
    let mode_text = match app.input_mode {
        InputMode::Normal => "NORMAL",
        InputMode::Processing => "AGENT",
        InputMode::Confirm { .. } => "CONFIRM",
    };
    let mode_color = match app.input_mode {
        InputMode::Processing => Color::Yellow,
        InputMode::Confirm { .. } => Color::Red,
        _ => Color::Green,
    };
    let center = Span::styled(
        format!(" {} ", mode_text),
        Style::default().fg(Color::Black).bg(mode_color),
    );
    let right_text = format!(
        " tokens: {} | turns: {} ",
        app.token_count, app.turn_count
    );
    let right_len = right_text.len();
    let right = Span::styled(
        right_text,
        Style::default().fg(Color::Gray).bg(Color::Rgb(55, 55, 75)),
    );

    let mut spans = vec![left, center];
    let total_width = area.width as usize;
    let used = 30 + 10 + right_len;
    if total_width > used {
        spans.push(Span::styled(
            " ".repeat(total_width.saturating_sub(used)),
            Style::default().bg(Color::Rgb(55, 55, 75)),
        ));
    }
    spans.push(right);

    let status = Paragraph::new(Line::from(spans))
        .style(Style::default().bg(Color::Rgb(55, 55, 75)));
    f.render_widget(status, area);
}
