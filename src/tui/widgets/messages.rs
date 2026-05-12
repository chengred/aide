use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

use crate::tui::app::{App, BubbleRole, ToolStatus};

/// Draw the messages area with scrolling
pub fn draw_messages(f: &mut Frame, area: Rect, app: &App) {
    let mut lines: Vec<Line> = Vec::new();

    for msg in &app.messages {
        match msg.role {
            BubbleRole::User => {
                lines.push(Line::from(vec![
                    Span::styled("  You: ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                ]));
                for line in msg.content.lines() {
                    lines.push(Line::from(vec![
                        Span::styled(format!("  {}", line), Style::default().fg(Color::White)),
                    ]));
                }
                lines.push(Line::from(""));
            }
            BubbleRole::Assistant => {
                lines.push(Line::from(vec![
                    Span::styled("  Agent: ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                ]));
                for line in msg.content.lines() {
                    lines.push(Line::from(vec![
                        Span::styled(format!("  {}", line), Style::default().fg(Color::Gray)),
                    ]));
                }
                for tc in &msg.tool_calls {
                    let (icon, color) = match tc.status {
                        ToolStatus::Running => ("◌", Color::Yellow),
                        ToolStatus::Success => ("✓", Color::Green),
                        ToolStatus::Failed => ("✗", Color::Red),
                    };
                    lines.push(Line::from(vec![
                        Span::styled(format!("    {} {} ", icon, tc.name), Style::default().fg(color)),
                        Span::styled(&tc.preview, Style::default().fg(Color::DarkGray)),
                    ]));
                }
                lines.push(Line::from(""));
            }
            BubbleRole::System => {
                lines.push(Line::from(vec![
                    Span::styled(format!("  *** {}", msg.content), Style::default().fg(Color::Yellow)),
                ]));
                lines.push(Line::from(""));
            }
        }
    }

    // Streaming text (during agent processing)
    if !app.streaming_text.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("  Agent: ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
        ]));
        let preview: String = app.streaming_text.lines().last().unwrap_or("").to_string();
        lines.push(Line::from(vec![
            Span::styled(format!("  {}", preview), Style::default().fg(Color::Gray)),
        ]));
        lines.push(Line::from(""));
    }

    // Active tool calls (during agent processing)
    for tc in &app.active_tools {
        let (icon, color) = match tc.status {
            ToolStatus::Running => ("◌", Color::Yellow),
            ToolStatus::Success => ("✓", Color::Green),
            ToolStatus::Failed => ("✗", Color::Red),
        };
        lines.push(Line::from(vec![
            Span::styled(format!("    {} {} ", icon, tc.name), Style::default().fg(color)),
            Span::styled(&tc.preview, Style::default().fg(Color::DarkGray)),
        ]));
    }

    // Error message
    if let Some(ref err) = app.error_msg {
        lines.push(Line::from(vec![
            Span::styled(format!("  Error: {}", err), Style::default().fg(Color::Red)),
        ]));
    }

    let block = Block::default()
        .borders(Borders::NONE)
        .style(Style::default().bg(Color::Rgb(18, 18, 24)));

    let paragraph = Paragraph::new(Text::from(lines))
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((app.scroll_offset, 0));

    f.render_widget(paragraph, area);
}
