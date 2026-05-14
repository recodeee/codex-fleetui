// API shape borrowed from warpui_core/src/ui_components/text_input.rs.
// Copyright (C) 2020-2026 Denver Technologies, Inc. — MIT.

//! Text input line for spotlight/search prompts.
//!
//! Adapted from Warp's MIT-licensed component vocabulary and rendered with
//! ratatui spans so cursor state is deterministic in snapshots.

use crate::palette::*;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph, Widget},
};
use std::borrow::Cow;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextInput<'a> {
    value: Cow<'a, str>,
    placeholder: Cow<'a, str>,
    focused: bool,
    cursor: usize,
}

impl<'a> TextInput<'a> {
    pub fn new(value: impl Into<Cow<'a, str>>) -> Self {
        let value = value.into();
        let cursor = value.chars().count();
        Self {
            value,
            placeholder: Cow::Borrowed("Search"),
            focused: false,
            cursor,
        }
    }

    pub fn placeholder(mut self, placeholder: impl Into<Cow<'a, str>>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    pub fn cursor(mut self, cursor: usize) -> Self {
        self.cursor = cursor.min(self.value.chars().count());
        self
    }

    pub fn line(&self) -> Line<'static> {
        if self.value.is_empty() {
            return Line::from(vec![
                Span::styled("⌕ ", Style::default().fg(IOS_FG_FAINT)),
                Span::styled(
                    self.placeholder.to_string(),
                    Style::default().fg(IOS_FG_FAINT),
                ),
            ]);
        }

        let chars: Vec<char> = self.value.chars().collect();
        let cursor = self.cursor.min(chars.len());
        let before: String = chars[..cursor].iter().collect();
        let at = chars.get(cursor).copied();
        let after_start = if at.is_some() { cursor + 1 } else { cursor };
        let after: String = chars[after_start..].iter().collect();

        let mut spans = vec![
            Span::styled("⌕ ", Style::default().fg(IOS_FG_MUTED)),
            Span::styled(before, Style::default().fg(IOS_FG)),
        ];

        if self.focused {
            let cursor_symbol = at.unwrap_or(' ');
            spans.push(Span::styled(
                cursor_symbol.to_string(),
                Style::default()
                    .fg(IOS_FG)
                    .bg(IOS_TINT)
                    .add_modifier(Modifier::BOLD),
            ));
        } else if let Some(ch) = at {
            spans.push(Span::styled(ch.to_string(), Style::default().fg(IOS_FG)));
        }

        spans.push(Span::styled(after, Style::default().fg(IOS_FG)));
        Line::from(spans)
    }
}

impl Widget for TextInput<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let border = if self.focused {
            IOS_TINT
        } else {
            IOS_HAIRLINE_STRONG
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(border))
            .style(Style::default().bg(IOS_BG_SOLID));
        Paragraph::new(self.line()).block(block).render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_uses_placeholder() {
        let line = TextInput::new("").placeholder("Filter").line();
        assert_eq!(line.spans[1].content, "Filter");
    }

    #[test]
    fn focused_input_marks_cursor_cell() {
        let line = TextInput::new("abc").focused(true).cursor(1).line();
        assert_eq!(line.spans[2].content, "b");
        assert_eq!(line.spans[2].style.bg, Some(IOS_TINT));
    }
}
