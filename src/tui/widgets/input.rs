use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};
use unicode_width::UnicodeWidthStr;

use crate::agent::ConfirmationType;
use crate::tui::app::{App, InputMode};

/// Draw the input area at the bottom
pub fn draw_input(f: &mut Frame, area: Rect, app: &App) {
    // Show confirmation dialog with diff if needed
    if let InputMode::Confirm { details, .. } = &app.input_mode {
        draw_confirm_dialog(f, area, details);
    }

    let input_style = match app.input_mode {
        InputMode::Processing => Style::default().fg(Color::DarkGray),
        InputMode::Confirm { .. } => Style::default().fg(Color::Yellow),
        _ => Style::default().fg(Color::White),
    };

    let prompt = Span::styled("> ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD));

    let cursor_pos = app.cursor_pos.min(app.input.chars().count());
    let input_text = if app.input.is_empty() && app.input_mode != InputMode::Processing {
        "输入消息，Enter 发送，Ctrl+C 退出".to_string()
    } else if app.input_mode == InputMode::Processing {
        "Agent 处理中... Esc 取消".to_string()
    } else {
        app.input.clone()
    };

    // Compute cursor column position before input_text is moved
    let before_chars: String = input_text.chars().take(cursor_pos).collect();
    let cursor_x = ("> ".width() + before_chars.width()) as u16;

    let text = if cursor_pos < input_text.chars().count() && app.input_mode == InputMode::Normal {
        let chars: Vec<char> = input_text.chars().collect();
        let before: String = chars[..cursor_pos].iter().collect();
        let at = chars[cursor_pos];
        let after: String = chars[cursor_pos + 1..].iter().collect();

        Line::from(vec![
            prompt,
            Span::styled(before, input_style),
            Span::styled(at.to_string(), Style::default().fg(Color::Black).bg(Color::White)),
            Span::styled(after, input_style),
        ])
    } else {
        Line::from(vec![
            prompt,
            Span::styled(input_text, input_style),
        ])
    };

    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(Color::Rgb(55, 55, 75)))
        .style(Style::default().bg(Color::Rgb(18, 18, 24)));

    let paragraph = Paragraph::new(text)
        .block(block)
        .style(Style::default().bg(Color::Rgb(18, 18, 24)));

    f.render_widget(paragraph, area);

    // Set cursor position (account for wide characters)
    if app.input_mode == InputMode::Normal {
        f.set_cursor_position((cursor_x, area.y + 1));
    }
}

fn draw_confirm_dialog(f: &mut Frame, area: Rect, details: &crate::agent::ConfirmationDetails) {
    let has_diff = details.old_content.is_some() || details.new_content.is_some();
    let dialog_h = if has_diff { 10.min(area.height.saturating_sub(4)) } else { 5 };
    let dialog_y = area.y.saturating_sub(dialog_h);
    let confirm_area = Rect {
        y: dialog_y,
        height: dialog_h,
        ..area
    };

    let mut lines: Vec<Line> = Vec::new();
    let border_color = match details.operation {
        ConfirmationType::WriteFile | ConfirmationType::EditFile => Color::Yellow,
        ConfirmationType::RunCommand => Color::Red,
        ConfirmationType::WebFetch => Color::Cyan,
        ConfirmationType::Generic => Color::Yellow,
    };

    // Header
    lines.push(Line::from(vec![
        Span::styled(" ⚠  ", Style::default().fg(border_color).add_modifier(Modifier::BOLD)),
        Span::styled(format!("Allow {}?", details.tool_name), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
    ]));
    lines.push(Line::from(""));

    // Summary
    lines.push(Line::from(vec![
        Span::styled("  ", Style::default().fg(Color::DarkGray)),
        Span::styled(&details.summary, Style::default().fg(Color::White)),
    ]));

    // Diff preview for file operations
    if has_diff {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("  ── Diff Preview ──", Style::default().fg(Color::DarkGray)),
        ]));

        if let (Some(old), Some(new)) = (&details.old_content, &details.new_content) {
            let old_lines: Vec<&str> = old.lines().take(8).collect();
            let new_lines: Vec<&str> = new.lines().take(8).collect();
            let max_lines = old_lines.len().max(new_lines.len());

            for i in 0..max_lines {
                let old_line = old_lines.get(i).unwrap_or(&"");
                let new_line = new_lines.get(i).unwrap_or(&"");
                if old_line != new_line {
                    if !old_line.is_empty() {
                        lines.push(Line::from(vec![
                            Span::styled(format!("  - {}", old_line), Style::default().fg(Color::Red)),
                        ]));
                    }
                    if !new_line.is_empty() {
                        lines.push(Line::from(vec![
                            Span::styled(format!("  + {}", new_line), Style::default().fg(Color::Green)),
                        ]));
                    }
                } else {
                    lines.push(Line::from(vec![
                        Span::styled(format!("    {}", old_line), Style::default().fg(Color::DarkGray)),
                    ]));
                }
            }
            if old.lines().count() > 8 || new.lines().count() > 8 {
                lines.push(Line::from(vec![
                    Span::styled("  ... (truncated)", Style::default().fg(Color::DarkGray)),
                ]));
            }
        }
    }

    // Action prompt
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("  Press ", Style::default().fg(Color::White)),
        Span::styled("Y", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
        Span::styled(" to approve, ", Style::default().fg(Color::White)),
        Span::styled("N", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
        Span::styled(" to deny, ", Style::default().fg(Color::White)),
        Span::styled("Esc", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
        Span::styled(" to cancel", Style::default().fg(Color::White)),
    ]));

    let dialog = Paragraph::new(Text::from(lines))
        .block(Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .title(format!(" {} Confirmation ", details.tool_name.to_uppercase()))
        )
        .style(Style::default().bg(Color::Rgb(22, 22, 30)))
        .wrap(Wrap { trim: false });
    f.render_widget(dialog, confirm_area);
}
