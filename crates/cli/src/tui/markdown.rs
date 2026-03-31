// tui/markdown.rs — Lightweight markdown to ratatui Spans renderer

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

/// Render a markdown string into ratatui Lines.
pub fn render_markdown(text: &str) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();

    for raw_line in text.lines() {
        if raw_line.is_empty() {
            lines.push(Line::from(""));
            continue;
        }

        // Headings
        if let Some(rest) = raw_line.strip_prefix("### ") {
            lines.push(Line::from(vec![Span::styled(
                rest.to_string(),
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            )]));
            continue;
        }
        if let Some(rest) = raw_line.strip_prefix("## ") {
            lines.push(Line::from(vec![Span::styled(
                rest.to_string(),
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            )]));
            continue;
        }
        if let Some(rest) = raw_line.strip_prefix("# ") {
            lines.push(Line::from(vec![Span::styled(
                rest.to_string(),
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            )]));
            continue;
        }

        // Bullet lists
        if raw_line.starts_with("- ") || raw_line.starts_with("* ") {
            let content = &raw_line[2..];
            let spans = parse_inline_styles(content);
            let mut result = vec![Span::raw("  • ")];
            result.extend(spans);
            lines.push(Line::from(result));
            continue;
        }

        // Numbered lists
        if let Some(pos) = raw_line.find(". ") {
            if pos > 0 && raw_line[..pos].chars().all(|c| c.is_ascii_digit()) {
                let prefix = &raw_line[..=pos];
                let content = &raw_line[pos + 2..];
                let spans = parse_inline_styles(content);
                let mut result = vec![Span::raw(format!("  {} ", prefix))];
                result.extend(spans);
                lines.push(Line::from(result));
                continue;
            }
        }

        // Code block markers
        if let Some(lang) = raw_line.strip_prefix("```") {
            lines.push(Line::from(vec![Span::styled(
                format!("── {} ──", if lang.is_empty() { "code" } else { lang }),
                Style::default().fg(Color::DarkGray),
            )]));
            continue;
        }

        // Regular line with inline formatting
        let spans = parse_inline_styles(raw_line);
        lines.push(Line::from(spans));
    }

    lines
}

/// Parse inline markdown styles (bold, code, italic) into Spans.
fn parse_inline_styles(text: &str) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut chars = text.char_indices().peekable();
    let mut current_start = 0;
    let current_style = Style::default();

    while let Some((i, ch)) = chars.next() {
        // Inline code: `text`
        if ch == '`' {
            // Flush current text
            if i > current_start {
                let s = text[current_start..i].to_string();
                spans.push(Span::styled(s, current_style));
            }
            // Find closing backtick
            let code_start = i + 1;
            let mut code_end = None;
            for (j, c) in chars.by_ref() {
                if c == '`' {
                    code_end = Some(j);
                    break;
                }
            }
            if let Some(end) = code_end {
                let code = text[code_start..end].to_string();
                spans.push(Span::styled(
                    code,
                    Style::default().fg(Color::Yellow).bg(Color::DarkGray),
                ));
                current_start = end + 1;
            } else {
                // Unclosed backtick
                spans.push(Span::styled(
                    "`".to_string(),
                    Style::default().fg(Color::Yellow),
                ));
                current_start = code_start;
            }
            continue;
        }

        // Bold: **text**
        if ch == '*' && chars.peek().map(|(_, c)| *c) == Some('*') {
            // Flush current text
            if i > current_start {
                let s = text[current_start..i].to_string();
                spans.push(Span::styled(s, current_style));
            }
            chars.next(); // consume second *
            let bold_start = i + 2;
            let mut bold_end = None;
            let mut prev_was_star = false;
            for (j, c) in chars.by_ref() {
                if c == '*' && prev_was_star {
                    bold_end = Some(j - 1);
                    break;
                }
                prev_was_star = c == '*';
            }
            if let Some(end) = bold_end {
                let bold_text = text[bold_start..end].to_string();
                spans.push(Span::styled(
                    bold_text,
                    Style::default().add_modifier(Modifier::BOLD),
                ));
                current_start = end + 2;
            } else {
                spans.push(Span::styled(
                    "**".to_string(),
                    Style::default().fg(Color::DarkGray),
                ));
                current_start = bold_start;
            }
            continue;
        }
    }

    // Flush remaining text
    if current_start < text.len() {
        let s = text[current_start..].to_string();
        spans.push(Span::styled(s, current_style));
    }

    if spans.is_empty() {
        spans.push(Span::raw(text.to_string()));
    }

    spans
}
