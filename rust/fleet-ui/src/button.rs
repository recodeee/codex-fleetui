// API shape borrowed from warpui_core/src/ui_components/button.rs.
// Copyright (C) 2020-2026 Denver Technologies, Inc. — MIT.

//! Button primitive for command bars and modal footers.
//!
//! Adapted from Warp's MIT-licensed component vocabulary and mapped to the
//! existing codex-fleet iOS palette.

use crate::{keyboard_shortcut::key_chip, palette::*};
use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph, Widget},
};
use std::borrow::Cow;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonVariant {
    Primary,
    Secondary,
    Destructive,
    Ghost,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonState {
    Normal,
    Focused,
    Pressed,
    Disabled,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Button<'a> {
    label: Cow<'a, str>,
    icon: Option<Cow<'a, str>>,
    shortcut: Option<Cow<'a, str>>,
    variant: ButtonVariant,
    state: ButtonState,
}

impl<'a> Button<'a> {
    pub fn new(label: impl Into<Cow<'a, str>>) -> Self {
        Self {
            label: label.into(),
            icon: None,
            shortcut: None,
            variant: ButtonVariant::Secondary,
            state: ButtonState::Normal,
        }
    }

    pub fn icon(mut self, icon: impl Into<Cow<'a, str>>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn shortcut(mut self, shortcut: impl Into<Cow<'a, str>>) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }

    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    pub fn state(mut self, state: ButtonState) -> Self {
        self.state = state;
        self
    }

    pub fn width(&self) -> u16 {
        let mut width = self.label.chars().count() as u16 + 4;
        if let Some(icon) = &self.icon {
            width += icon.chars().count() as u16 + 1;
        }
        if let Some(shortcut) = &self.shortcut {
            width += shortcut.chars().count() as u16 + 3;
        }
        width
    }

    pub fn line(&self) -> Line<'static> {
        let mut spans = Vec::new();
        let (fg, bg, _) = self.colors();
        if let Some(icon) = &self.icon {
            spans.push(Span::styled(
                format!("{} ", icon),
                Style::default().fg(fg).bg(bg).add_modifier(Modifier::BOLD),
            ));
        }
        spans.push(Span::styled(
            self.label.to_string(),
            Style::default().fg(fg).bg(bg).add_modifier(Modifier::BOLD),
        ));
        if let Some(shortcut) = &self.shortcut {
            spans.push(Span::raw(" "));
            spans.push(key_chip(shortcut));
        }
        Line::from(spans)
    }

    fn colors(&self) -> (Color, Color, Color) {
        if self.state == ButtonState::Disabled {
            return (IOS_FG_FAINT, IOS_BG_SOLID, IOS_HAIRLINE);
        }

        let (fg, bg, border) = match self.variant {
            ButtonVariant::Primary => (IOS_FG, IOS_TINT, IOS_TINT),
            ButtonVariant::Secondary => (IOS_FG, IOS_CARD_BG, IOS_HAIRLINE_STRONG),
            ButtonVariant::Destructive => (IOS_FG, IOS_DESTRUCTIVE, IOS_DESTRUCTIVE),
            ButtonVariant::Ghost => (IOS_TINT, IOS_BG_SOLID, IOS_HAIRLINE),
        };

        if self.state == ButtonState::Pressed {
            (fg, IOS_TINT_DARK, border)
        } else if self.state == ButtonState::Focused {
            (fg, bg, IOS_TINT)
        } else {
            (fg, bg, border)
        }
    }
}

impl Widget for Button<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let (_, bg, border) = self.colors();
        let mut style = Style::default().bg(bg);
        if self.state == ButtonState::Disabled {
            style = style.fg(IOS_FG_FAINT);
        }
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(border).add_modifier(
                if self.state == ButtonState::Focused {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                },
            ))
            .style(style);
        Paragraph::new(self.line())
            .alignment(Alignment::Center)
            .block(block)
            .render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn button_width_accounts_for_icon_and_shortcut() {
        let button = Button::new("Open").icon("⌘").shortcut("O");
        assert_eq!(button.width(), 14);
    }

    #[test]
    fn focused_button_keeps_primary_tint_border() {
        let button = Button::new("Run")
            .variant(ButtonVariant::Primary)
            .state(ButtonState::Focused);
        let (_, _, border) = button.colors();
        assert_eq!(border, IOS_TINT);
    }
}
