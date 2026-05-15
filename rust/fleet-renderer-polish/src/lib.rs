//! Shared ratatui chrome primitives for codex-fleet dashboards.
//!
//! This crate exposes a small library of reusable visual building blocks
//! (rounded blocks, hairline dividers, iOS-style colored chips, two-line
//! page headers) so individual fleet binaries can present a consistent
//! iOS-flavored look without each crate re-implementing the same chrome.
//!
//! The crate is intentionally library-only. Adoption by the existing
//! `fleet-*` binaries is a follow-up; this lane only ships the primitives.

use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Padding};

/// A `Block` with rounded corners and 1-cell internal padding, titled with
/// `title`. Suitable as the outer chrome for cards/panels.
///
/// # Example
///
/// ```ignore
/// use fleet_renderer_polish::rounded_corner_block;
/// let block = rounded_corner_block("Fleet status");
/// // render `block` as the outer chrome of any widget area.
/// ```
pub fn rounded_corner_block(title: &str) -> Block<'_> {
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .padding(Padding::uniform(1))
}

/// A single-line hairline divider made of `─` characters, styled with the
/// iOS-flavored separator gray `#3A3A3C`. The returned `Line` owns its
/// string buffer, so it can outlive borrowed inputs.
///
/// # Example
///
/// ```ignore
/// use fleet_renderer_polish::hairline_divider;
/// let line = hairline_divider(40);
/// // place `line` between two sections to get a subtle visual break.
/// ```
pub fn hairline_divider(width: u16) -> Line<'static> {
    let count = width as usize;
    let mut buf = String::with_capacity(count * 3);
    for _ in 0..count {
        buf.push('─');
    }
    Line::from(Span::styled(
        buf,
        Style::default().fg(Color::Rgb(0x3A, 0x3A, 0x3C)),
    ))
}

/// iOS system palette colors used by the chrome primitives.
///
/// Each variant maps to the canonical iOS system color via
/// [`IosColor::ios_rgb`].
///
/// # Example
///
/// ```ignore
/// use fleet_renderer_polish::IosColor;
/// let color = IosColor::Green.ios_rgb();
/// // pass `color` to any ratatui Style::default().fg(...) call.
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IosColor {
    Green,
    Blue,
    Yellow,
    Red,
    Gray,
}

impl IosColor {
    /// Returns the ratatui [`Color`] corresponding to the iOS system palette
    /// entry for this variant.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use fleet_renderer_polish::IosColor;
    /// assert_eq!(IosColor::Blue.ios_rgb(), IosColor::Blue.ios_rgb());
    /// ```
    pub fn ios_rgb(&self) -> Color {
        match self {
            // iOS systemGreen
            IosColor::Green => Color::Rgb(0x34, 0xC7, 0x59),
            // iOS systemBlue
            IosColor::Blue => Color::Rgb(0x00, 0x7A, 0xFF),
            // iOS systemYellow
            IosColor::Yellow => Color::Rgb(0xFF, 0xCC, 0x00),
            // iOS systemRed
            IosColor::Red => Color::Rgb(0xFF, 0x3B, 0x30),
            // iOS systemGray
            IosColor::Gray => Color::Rgb(0x8E, 0x8E, 0x93),
        }
    }
}

/// A foreground-only iOS status chip, e.g. ` LIVE `, ` IDLE `, ` ERR `.
///
/// The chip is rendered as a single padded `Span` whose foreground color
/// comes from [`IosColor::ios_rgb`]. No background fill is applied so the
/// chip blends with whatever surface it lands on.
///
/// # Example
///
/// ```ignore
/// use fleet_renderer_polish::{ios_status_chip, IosColor};
/// let chip = ios_status_chip("LIVE", IosColor::Green);
/// // append `chip` to any ratatui Line to get a colored status pill.
/// ```
pub fn ios_status_chip<'a>(label: &'a str, color: IosColor) -> Span<'a> {
    Span::styled(
        format!(" {label} "),
        Style::default().fg(color.ios_rgb()),
    )
}

/// A two-line iOS-style page header: a bold title line followed by an
/// accent underline rendered in the chosen [`IosColor`].
///
/// The underline matches the visible length of `title`, so the accent bar
/// hugs the title rather than the full panel width.
///
/// # Example
///
/// ```ignore
/// use fleet_renderer_polish::{page_header_with_accent, IosColor};
/// let lines = page_header_with_accent("Fleet", IosColor::Blue);
/// // render `lines` with a Paragraph as the top of a page.
/// ```
pub fn page_header_with_accent(title: &str, accent: IosColor) -> Vec<Line<'_>> {
    let underline_len = title.chars().count();
    let mut underline = String::with_capacity(underline_len * 3);
    for _ in 0..underline_len {
        underline.push('─');
    }
    vec![
        Line::from(Span::styled(
            title,
            Style::default()
                .fg(Color::White)
                .add_modifier(ratatui::style::Modifier::BOLD),
        )),
        Line::from(Span::styled(
            underline,
            Style::default().fg(accent.ios_rgb()),
        )),
    ]
}
