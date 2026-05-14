//! Toast/notification surface.
//!
//! Adapted from Warp's MIT-licensed component vocabulary and tuned for the
//! codex-fleet dark iOS palette.

use crate::{keyboard_shortcut::key_chip, palette::*};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph, Widget},
};
use std::borrow::Cow;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotificationKind {
    Info,
    Success,
    Warning,
    Error,
}

impl NotificationKind {
    pub fn color(self) -> Color {
        match self {
            NotificationKind::Info => IOS_TINT,
            NotificationKind::Success => IOS_GREEN,
            NotificationKind::Warning => IOS_ORANGE,
            NotificationKind::Error => IOS_DESTRUCTIVE,
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            NotificationKind::Info => "i",
            NotificationKind::Success => "✓",
            NotificationKind::Warning => "!",
            NotificationKind::Error => "✕",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notification<'a> {
    kind: NotificationKind,
    title: Cow<'a, str>,
    body: Option<Cow<'a, str>>,
    action: Option<Cow<'a, str>>,
}

impl<'a> Notification<'a> {
    pub fn new(kind: NotificationKind, title: impl Into<Cow<'a, str>>) -> Self {
        Self {
            kind,
            title: title.into(),
            body: None,
            action: None,
        }
    }

    pub fn body(mut self, body: impl Into<Cow<'a, str>>) -> Self {
        self.body = Some(body.into());
        self
    }

    pub fn action(mut self, action: impl Into<Cow<'a, str>>) -> Self {
        self.action = Some(action.into());
        self
    }

    pub fn height(&self) -> u16 {
        2 + if self.body.is_some() { 1 } else { 0 }
    }

    fn title_line(&self) -> Line<'static> {
        let color = self.kind.color();
        let mut spans = vec![
            Span::styled(
                format!(" {} ", self.kind.icon()),
                Style::default()
                    .fg(IOS_FG)
                    .bg(color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(
                self.title.to_string(),
                Style::default().fg(IOS_FG).add_modifier(Modifier::BOLD),
            ),
        ];
        if let Some(action) = &self.action {
            spans.push(Span::raw("  "));
            spans.push(key_chip(action));
        }
        Line::from(spans)
    }
}

impl Widget for Notification<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.kind.color()))
            .style(Style::default().fg(IOS_FG).bg(IOS_BG_GLASS));
        let inner = block.inner(area);
        block.render(area, buf);
        Paragraph::new(self.title_line()).render(
            Rect {
                x: inner.x,
                y: inner.y,
                width: inner.width,
                height: 1,
            },
            buf,
        );
        if let Some(body) = self.body {
            Paragraph::new(Line::from(Span::styled(
                body.to_string(),
                Style::default().fg(IOS_FG_MUTED),
            )))
            .render(
                Rect {
                    x: inner.x,
                    y: inner.y + 1,
                    width: inner.width,
                    height: 1,
                },
                buf,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_maps_to_palette() {
        assert_eq!(NotificationKind::Success.color(), IOS_GREEN);
        assert_eq!(NotificationKind::Error.icon(), "✕");
    }
}
