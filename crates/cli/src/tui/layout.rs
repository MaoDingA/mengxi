// tui/layout.rs — TUI layout with split panels

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

use super::app::{App, ChatMessage, ToolStatus};
use super::markdown;

/// Draw the full TUI layout.
pub fn draw(f: &mut Frame, app: &App) {
    let size = f.area();

    // Top-level layout: main area + input bar
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(5),     // Main area (chat + results)
            Constraint::Length(3),  // Input bar
        ])
        .split(size);

    // Split main area into chat (left) and results (right)
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(60),  // Chat panel
            Constraint::Percentage(40),  // Result panel
        ])
        .split(chunks[0]);

    draw_chat_panel(f, app, main_chunks[0]);
    draw_result_panel(f, app, main_chunks[1]);
    draw_input_panel(f, app, chunks[1]);
}

fn draw_chat_panel(f: &mut Frame, app: &App, area: Rect) {
    let lines: Vec<Line> = app
        .messages
        .iter()
        .flat_map(|msg| match msg {
            ChatMessage::User(text) => {
                vec![Line::from(vec![
                    Span::styled("You: ", Style::default().fg(Color::Cyan)),
                    Span::raw(text),
                ])]
            }
            ChatMessage::Assistant(text) => {
                let mut result = vec![Line::from(vec![
                    Span::styled("Mengxi: ", Style::default().fg(Color::Green)),
                ])];
                let md_lines = markdown::render_markdown(text);
                result.extend(md_lines);
                result
            }
            ChatMessage::System(text) => {
                vec![Line::from(vec![
                    Span::styled("System: ", Style::default().fg(Color::Yellow)),
                    Span::raw(text),
                ])]
            }
            ChatMessage::ToolCall { name, status } => {
                let icon = match status {
                    ToolStatus::Running => "⟳",
                    ToolStatus::Success(_) => "✓",
                    ToolStatus::Error(_) => "✗",
                };
                let color = match status {
                    ToolStatus::Running => Color::Yellow,
                    ToolStatus::Success(_) => Color::Green,
                    ToolStatus::Error(_) => Color::Red,
                };
                vec![Line::from(vec![
                    Span::styled(
                        format!(" {} ", icon),
                        Style::default().fg(color),
                    ),
                    Span::styled(
                        format!("Tool: {}", name),
                        Style::default().add_modifier(Modifier::DIM),
                    ),
                ])]
            }
        })
        .collect();

    let paragraph = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(" Chat "))
        .wrap(Wrap { trim: false })
        .scroll((app.scroll_offset as u16, 0));

    f.render_widget(paragraph, area);
}

fn draw_result_panel(f: &mut Frame, app: &App, area: Rect) {
    let mut lines: Vec<Line> = Vec::new();

    // Show the last result data if any
    if let Some(data) = &app.last_result {
        lines.push(Line::from(Span::styled(
            "Results & Visualizations",
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(""));

        let md_lines = markdown::render_markdown(data);
        lines.extend(md_lines);
    } else {
        lines.push(Line::from(Span::styled(
            "Results & Visualizations",
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Search results and charts will appear here.",
            Style::default().fg(Color::DarkGray),
        )));
    }

    let paragraph = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(" Results "))
        .wrap(Wrap { trim: false });

    f.render_widget(paragraph, area);
}

fn draw_input_panel(f: &mut Frame, app: &App, area: Rect) {
    let input_style = if app.active_panel == 1 {
        Style::default().fg(Color::White)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let input = Paragraph::new(app.input.as_str())
        .style(input_style)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Input (Enter to send, Esc to quit) "),
        );

    f.render_widget(input, area);

    // Show cursor in input field
    if app.active_panel == 1 {
        f.set_cursor_position((
            area.x + 1 + app.input.len() as u16,
            area.y + 1,
        ));
    }
}
