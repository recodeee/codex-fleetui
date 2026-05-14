// API shape borrowed from warpui_core/src/ui_components/list.rs.
// Copyright (C) 2020-2026 Denver Technologies, Inc. — MIT.

//! Dense list rows for command palettes and session pickers.
//!
//! Adapted from Warp's MIT-licensed component vocabulary with codex-fleet's
//! existing icon-chip and muted-detail treatment.

use crate::{keyboard_shortcut::key_chip, palette::*};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};
use std::borrow::Cow;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListItem<'a> {
    icon: Option<Cow<'a, str>>,
    title: Cow<'a, str>,
    detail: Option<Cow<'a, str>>,
    shortcut: Option<Cow<'a, str>>,
    destructive: bool,
}

impl<'a> ListItem<'a> {
    pub fn new(title: impl Into<Cow<'a, str>>) -> Self {
        Self {
            icon: None,
            title: title.into(),
            detail: None,
            shortcut: None,
            destructive: false,
        }
    }

    pub fn icon(mut self, icon: impl Into<Cow<'a, str>>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn detail(mut self, detail: impl Into<Cow<'a, str>>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn shortcut(mut self, shortcut: impl Into<Cow<'a, str>>) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }

    pub fn destructive(mut self, destructive: bool) -> Self {
        self.destructive = destructive;
        self
    }

    pub fn line(&self, selected: bool) -> Line<'static> {
        let bg = if selected { IOS_CARD_BG } else { IOS_BG_SOLID };
        let fg = if self.destructive {
            IOS_DESTRUCTIVE
        } else {
            IOS_FG
        };
        let mut spans = Vec::new();
        spans.push(Span::styled(" ", Style::default().bg(bg)));
        if let Some(icon) = &self.icon {
            let icon_bg = if self.destructive {
                Color::Rgb(58, 24, 24)
            } else {
                IOS_ICON_CHIP
            };
            spans.push(Span::styled(
                format!(" {} ", icon),
                Style::default()
                    .fg(fg)
                    .bg(icon_bg)
                    .add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::styled(" ", Style::default().bg(bg)));
        }
        spans.push(Span::styled(
            self.title.to_string(),
            Style::default().fg(fg).bg(bg).add_modifier(if selected {
                Modifier::BOLD
            } else {
                Modifier::empty()
            }),
        ));
        if let Some(detail) = &self.detail {
            spans.push(Span::styled("  ", Style::default().bg(bg)));
            spans.push(Span::styled(
                detail.to_string(),
                Style::default().fg(IOS_FG_MUTED).bg(bg),
            ));
        }
        if let Some(shortcut) = &self.shortcut {
            spans.push(Span::styled(" ", Style::default().bg(bg)));
            spans.push(key_chip(shortcut));
        }
        Line::from(spans)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FleetList<'a> {
    items: Vec<ListItem<'a>>,
    selected: Option<usize>,
}

impl<'a> FleetList<'a> {
    pub fn new(items: Vec<ListItem<'a>>) -> Self {
        Self {
            items,
            selected: None,
        }
    }

    pub fn selected(mut self, selected: Option<usize>) -> Self {
        self.selected = selected;
        self
    }
}

impl Widget for FleetList<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        for (index, item) in self.items.iter().enumerate().take(area.height as usize) {
            let rect = Rect {
                x: area.x,
                y: area.y + index as u16,
                width: area.width,
                height: 1,
            };
            Paragraph::new(item.line(self.selected == Some(index))).render(rect, buf);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_row_is_bold() {
        let item = ListItem::new("Open").detail("pane 1");
        let line = item.line(true);
        assert!(line.spans[1].style.add_modifier.contains(Modifier::BOLD));
    }
}
