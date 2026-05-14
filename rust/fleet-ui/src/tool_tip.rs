// API shape borrowed from warpui_core/src/ui_components/tool_tip.rs.
// Copyright (C) 2020-2026 Denver Technologies, Inc. — MIT.

//! Tooltip bubble for icon-first controls.
//!
//! Adapted from Warp's MIT-licensed component vocabulary and sized for
//! codex-fleet's dense terminal chrome.

use crate::{keyboard_shortcut::key_chip, palette::*};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph, Widget},
};
use std::borrow::Cow;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolTip<'a> {
    title: Cow<'a, str>,
    body: Option<Cow<'a, str>>,
    shortcut: Option<Cow<'a, str>>,
}

impl<'a> ToolTip<'a> {
    pub fn new(title: impl Into<Cow<'a, str>>) -> Self {
        Self {
            title: title.into(),
            body: None,
            shortcut: None,
        }
    }

    pub fn body(mut self, body: impl Into<Cow<'a, str>>) -> Self {
        self.body = Some(body.into());
        self
    }

    pub fn shortcut(mut self, shortcut: impl Into<Cow<'a, str>>) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }

    pub fn height(&self) -> u16 {
        2 + if self.body.is_some() { 1 } else { 0 }
    }

    fn title_line(&self) -> Line<'static> {
        let mut spans = vec![Span::styled(
            self.title.to_string(),
            Style::default().fg(IOS_FG).add_modifier(Modifier::BOLD),
        )];
        if let Some(shortcut) = &self.shortcut {
            spans.push(Span::raw("  "));
            spans.push(key_chip(shortcut));
        }
        Line::from(spans)
    }
}

impl Widget for ToolTip<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(IOS_HAIRLINE_STRONG))
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
    fn tooltip_height_grows_with_body() {
        assert_eq!(ToolTip::new("Copy").height(), 2);
        assert_eq!(ToolTip::new("Copy").body("Copy visible pane").height(), 3);
    }
}
