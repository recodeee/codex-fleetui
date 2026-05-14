// API shape borrowed from warpui_core/src/ui_components/segmented_control.rs.
// Copyright (C) 2020-2026 Denver Technologies, Inc. — MIT.

//! Segmented control for mode switches.
//!
//! Adapted from Warp's MIT-licensed component vocabulary: active segments are
//! filled pills, inactive segments sit on the glass surface.

use crate::palette::*;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};
use std::borrow::Cow;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Segment<'a> {
    label: Cow<'a, str>,
    icon: Option<Cow<'a, str>>,
    badge: Option<Cow<'a, str>>,
}

impl<'a> Segment<'a> {
    pub fn new(label: impl Into<Cow<'a, str>>) -> Self {
        Self {
            label: label.into(),
            icon: None,
            badge: None,
        }
    }

    pub fn icon(mut self, icon: impl Into<Cow<'a, str>>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn badge(mut self, badge: impl Into<Cow<'a, str>>) -> Self {
        self.badge = Some(badge.into());
        self
    }

    fn label(&self) -> String {
        let mut out = String::new();
        if let Some(icon) = &self.icon {
            out.push_str(icon);
            out.push(' ');
        }
        out.push_str(&self.label);
        if let Some(badge) = &self.badge {
            out.push(' ');
            out.push_str(badge);
        }
        out
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SegmentedControl<'a> {
    segments: Vec<Segment<'a>>,
    selected: usize,
    focused: bool,
}

impl<'a> SegmentedControl<'a> {
    pub fn new(segments: Vec<Segment<'a>>, selected: usize) -> Self {
        Self {
            segments,
            selected,
            focused: false,
        }
    }

    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    pub fn line(&self) -> Line<'static> {
        let mut spans = Vec::new();
        spans.push(Span::styled(" ", Style::default().bg(IOS_BG_GLASS)));
        for (index, segment) in self.segments.iter().enumerate() {
            if index > 0 {
                spans.push(Span::styled(
                    " ",
                    Style::default().fg(IOS_HAIRLINE).bg(IOS_BG_GLASS),
                ));
            }
            let active = index == self.selected;
            let fg = if active { IOS_FG } else { IOS_FG_MUTED };
            let bg = if active { IOS_TINT } else { IOS_BG_GLASS };
            let mut style = Style::default().fg(fg).bg(bg);
            if active || self.focused {
                style = style.add_modifier(Modifier::BOLD);
            }
            spans.push(Span::styled(format!(" {} ", segment.label()), style));
        }
        spans.push(Span::styled(" ", Style::default().bg(IOS_BG_GLASS)));
        Line::from(spans)
    }
}

impl Widget for SegmentedControl<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        Paragraph::new(self.line()).render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_segment_gets_tint() {
        let control = SegmentedControl::new(vec![Segment::new("A"), Segment::new("B")], 1);
        let line = control.line();
        assert_eq!(line.spans[3].style.bg, Some(IOS_TINT));
    }
}
