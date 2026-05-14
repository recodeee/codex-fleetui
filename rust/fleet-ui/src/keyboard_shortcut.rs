// API shape borrowed from warpui_core/src/ui_components/keyboard_shortcut.rs.
// Copyright (C) 2020-2026 Denver Technologies, Inc. — MIT.

//! Keyboard shortcut chips for command surfaces.
//!
//! Adapted from Warp's MIT-licensed component vocabulary and implemented as
//! ratatui spans for codex-fleet overlays, menus, and command rows.

use crate::palette::*;
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

/// A normalized keyboard chord such as `⌘ K` or `Ctrl Shift P`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyChord {
    parts: Vec<String>,
}

impl KeyChord {
    pub fn new(parts: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            parts: parts
                .into_iter()
                .map(Into::into)
                .filter(|part| !part.trim().is_empty())
                .collect(),
        }
    }

    pub fn single(key: impl Into<String>) -> Self {
        Self::new([key])
    }

    pub fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }

    pub fn label(&self) -> String {
        self.parts.join(" ")
    }

    pub fn width(&self) -> u16 {
        if self.parts.is_empty() {
            0
        } else {
            self.parts
                .iter()
                .map(|part| part.chars().count() as u16 + 2)
                .sum::<u16>()
                + self.parts.len().saturating_sub(1) as u16
        }
    }

    pub fn spans(&self) -> Vec<Span<'static>> {
        let mut spans = Vec::new();
        for (index, part) in self.parts.iter().enumerate() {
            if index > 0 {
                spans.push(Span::styled(" ", Style::default().fg(IOS_FG_FAINT)));
            }
            spans.push(key_chip(part));
        }
        spans
    }
}

/// Render a single key as a compact rounded-looking chip.
pub fn key_chip(label: &str) -> Span<'static> {
    Span::styled(
        format!(" {} ", label.trim()),
        Style::default()
            .fg(IOS_FG)
            .bg(IOS_CHIP_BG)
            .add_modifier(Modifier::BOLD),
    )
}

/// Render a full chord as one line.
pub fn shortcut_line(chord: &KeyChord) -> Line<'static> {
    Line::from(chord.spans())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_empty_parts() {
        let chord = KeyChord::new(["⌘", "", "K"]);
        assert_eq!(chord.label(), "⌘ K");
        assert_eq!(chord.spans().len(), 3);
    }

    #[test]
    fn reports_render_width() {
        let chord = KeyChord::new(["Ctrl", "P"]);
        assert_eq!(chord.width(), 10);
    }
}
