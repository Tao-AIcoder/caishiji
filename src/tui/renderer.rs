//! Message rendering — converts Messages into ratatui widgets.
//! Mirrors Claude Code's components/MessageList.tsx + ToolRenderer.tsx.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};

use crate::{
    messages::{ContentBlock, Message},
    state::AppState,
    tui::input::InputState,
};

const USER_COLOR: Color = Color::Cyan;
const ASSISTANT_COLOR: Color = Color::Green;
const TOOL_COLOR: Color = Color::Yellow;
const ERROR_COLOR: Color = Color::Red;
const SYSTEM_COLOR: Color = Color::DarkGray;
const LOADING_COLOR: Color = Color::Magenta;

/// Render the full REPL UI into the terminal frame.
pub fn render(frame: &mut Frame, state: &AppState, input: &InputState, scroll_offset: u16) {
    let area = frame.area();

    // Layout: messages | status bar | input box
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),       // messages
            Constraint::Length(1),    // status bar
            Constraint::Length(3),    // input box
        ])
        .split(area);

    render_messages(frame, state, chunks[0], scroll_offset);
    render_status_bar(frame, state, chunks[1]);
    render_input(frame, input, state.is_loading, chunks[2]);
}

fn render_messages(frame: &mut Frame, state: &AppState, area: Rect, _scroll_offset: u16) {
    let items: Vec<ListItem> = state
        .messages
        .iter()
        .flat_map(|msg| message_to_list_items(msg))
        .collect();

    let block = Block::default()
        .borders(Borders::NONE)
        .title(" 采石矶 ")
        .title_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));

    let list = List::new(items).block(block);

    // Simple scroll: ratatui List doesn't support offset natively,
    // so we skip rendering items above scroll_offset.
    frame.render_widget(list, area);
}

fn message_to_list_items(msg: &Message) -> Vec<ListItem<'static>> {
    match msg {
        Message::User { content, .. } => {
            let text = content_to_text(content, USER_COLOR, "> ");
            vec![
                ListItem::new(Line::from("")),
                ListItem::new(text),
            ]
        }
        Message::Assistant { content, api_error, .. } => {
            let mut items = vec![ListItem::new(Line::from(""))];

            if let Some(err) = api_error {
                items.push(ListItem::new(
                    Line::from(vec![
                        Span::styled("✗ Error: ", Style::default().fg(ERROR_COLOR).add_modifier(Modifier::BOLD)),
                        Span::raw(err.clone()),
                    ])
                ));
                return items;
            }

            for block in content {
                match block {
                    ContentBlock::Text { text } => {
                        for line in text.lines() {
                            items.push(ListItem::new(
                                Line::from(vec![
                                    Span::styled("  ", Style::default()),
                                    Span::styled(line.to_string(), Style::default().fg(ASSISTANT_COLOR)),
                                ])
                            ));
                        }
                    }
                    ContentBlock::ToolUse { name, input, .. } => {
                        let args = serde_json::to_string(input).unwrap_or_default();
                        let args_short = if args.len() > 80 {
                            format!("{}…", &args[..80])
                        } else {
                            args
                        };
                        items.push(ListItem::new(
                            Line::from(vec![
                                Span::styled("  ⚙ ", Style::default().fg(TOOL_COLOR)),
                                Span::styled(name.clone(), Style::default().fg(TOOL_COLOR).add_modifier(Modifier::BOLD)),
                                Span::styled(format!("({args_short})"), Style::default().fg(Color::DarkGray)),
                            ])
                        ));
                    }
                    ContentBlock::ToolResult { content, is_error, .. } => {
                        let color = if is_error.unwrap_or(false) { ERROR_COLOR } else { Color::DarkGray };
                        for line in content.lines().take(5) {
                            items.push(ListItem::new(
                                Line::from(vec![
                                    Span::styled("    │ ", Style::default().fg(Color::DarkGray)),
                                    Span::styled(line.to_string(), Style::default().fg(color)),
                                ])
                            ));
                        }
                        let line_count = content.lines().count();
                        if line_count > 5 {
                            items.push(ListItem::new(
                                Line::from(Span::styled(
                                    format!("    … ({} more lines)", line_count - 5),
                                    Style::default().fg(Color::DarkGray),
                                ))
                            ));
                        }
                    }
                }
            }
            items
        }
        Message::System { text, .. } => {
            vec![ListItem::new(
                Line::from(Span::styled(
                    format!("  ℹ {text}"),
                    Style::default().fg(SYSTEM_COLOR).add_modifier(Modifier::ITALIC),
                ))
            )]
        }
    }
}

fn content_to_text(blocks: &[ContentBlock], color: Color, prefix: &str) -> Text<'static> {
    let text = blocks
        .iter()
        .filter_map(|b| {
            if let ContentBlock::Text { text } = b {
                Some(text.clone())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join(" ");

    Text::from(Line::from(vec![
        Span::styled(
            prefix.to_string(),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(text, Style::default().fg(color)),
    ]))
}

fn render_status_bar(frame: &mut Frame, state: &AppState, area: Rect) {
    let loading_indicator = if state.is_loading { "⟳ " } else { "" };
    let cost = if state.settings.show_cost {
        format!(" ${:.4}", state.session_cost_usd)
    } else {
        String::new()
    };
    let tokens = format!(
        "↑{} ↓{}",
        state.session_usage.input_tokens,
        state.session_usage.output_tokens
    );
    let error_part = state
        .last_error
        .as_deref()
        .map(|e| format!(" | ✗ {e}"))
        .unwrap_or_default();

    let status_text = format!(
        " {loading_indicator}{} | {tokens}{cost}{error_part}",
        state.model
    );

    let style = if state.is_loading {
        Style::default().fg(LOADING_COLOR)
    } else if state.last_error.is_some() {
        Style::default().fg(ERROR_COLOR)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let paragraph = Paragraph::new(status_text).style(style);
    frame.render_widget(paragraph, area);
}

fn render_input(frame: &mut Frame, input: &InputState, is_loading: bool, area: Rect) {
    let border_color = if is_loading { LOADING_COLOR } else { Color::Blue };
    let title = if is_loading { " Thinking… " } else { " Message " };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(title);

    let display_text = if input.buffer.is_empty() && !is_loading {
        Span::styled(
            "Type a message (Enter to send, Ctrl+D to quit)",
            Style::default().fg(Color::DarkGray),
        )
    } else {
        Span::raw(input.buffer.clone())
    };

    let paragraph = Paragraph::new(Line::from(display_text))
        .block(block)
        .wrap(Wrap { trim: false });

    frame.render_widget(paragraph, area);
}
