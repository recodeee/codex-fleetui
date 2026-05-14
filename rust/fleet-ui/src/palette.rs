//! iOS system colour palette (dark UIKit variants).
//!
//! Ported from `scripts/codex-fleet/fleet-tick.sh`'s `IOS_*` block and the
//! `GLASS` object in the iOS terminal-design bundle. These RGB triples
//! are the source of truth for every chip / rail / card / overlay in the
//! fleet-ui crate, so port consumers reference them by name rather than
//! re-typing hex literals.
//!
//! Hex equivalents (Apple SwiftUI `.system*` names in parens):
//!
//! | Const               | Hex       | SwiftUI               |
//! |---------------------|-----------|-----------------------|
//! | `IOS_TINT`          | `#0a84ff` | `.systemBlue` (dark)  |
//! | `IOS_DESTRUCTIVE`   | `#ff453a` | `.systemRed` (dark)   |
//! | `IOS_GREEN`         | `#30d158` | `.systemGreen` (dark) |
//! | `IOS_ORANGE`        | `#ff9f0a` | `.systemOrange` (dark)|
//! | `IOS_YELLOW`        | `#ffd60a` | `.systemYellow` (dark)|
//! | `IOS_PURPLE`        | `#bf5af2` | `.systemPurple` (dark)|
//! | `IOS_FG`            | `#f2f2f7` | `.label` (dark)       |
//! | `IOS_FG_MUTED`      | `#a0a0aa` | `.secondaryLabel`     |
//! | `IOS_FG_FAINT`      | `#6e6e78` | `.tertiaryLabel`      |
//! | `IOS_BG_GLASS`      | `#262628` | `.menuBackground`     |
//! | `IOS_BG_SOLID`      | `#1c1c1e` | `.systemBackground`   |
//! | `IOS_HAIRLINE`      | `#3c3c41` | `.separator`          |
//! | `IOS_HAIRLINE_STRONG` | `#55555a` | `.opaqueSeparator`  |
//! | `IOS_CHIP_BG`       | `#36363a` | shortcut-chip fill    |
//! | `IOS_CARD_BG`       | `#2c2c30` | grouped section bg    |
//! | `IOS_ICON_CHIP`     | `#46464c` | 30×30 icon tile bg    |
//! | `IOS_TINT_DARK`     | `#0764dc` | active-pill shadow    |
//! | `IOS_TINT_SUB`      | `#d2e0ff` | Top-Hit subtitle fg   |

use ratatui::style::Color;

// Accents
pub const IOS_TINT: Color = Color::Rgb(10, 132, 255);
pub const IOS_DESTRUCTIVE: Color = Color::Rgb(255, 69, 58);
pub const IOS_GREEN: Color = Color::Rgb(48, 209, 88);
pub const IOS_ORANGE: Color = Color::Rgb(255, 159, 10);
pub const IOS_YELLOW: Color = Color::Rgb(255, 214, 10);
pub const IOS_PURPLE: Color = Color::Rgb(191, 90, 242);

// Labels
pub const IOS_FG: Color = Color::Rgb(242, 242, 247);
pub const IOS_FG_MUTED: Color = Color::Rgb(160, 160, 170);
pub const IOS_FG_FAINT: Color = Color::Rgb(110, 110, 120);

// Surfaces
pub const IOS_BG_GLASS: Color = Color::Rgb(38, 38, 40);
pub const IOS_BG_SOLID: Color = Color::Rgb(28, 28, 30);
pub const IOS_HAIRLINE: Color = Color::Rgb(60, 60, 65);
pub const IOS_HAIRLINE_STRONG: Color = Color::Rgb(85, 85, 90);
pub const IOS_CHIP_BG: Color = Color::Rgb(54, 54, 58);
pub const IOS_CARD_BG: Color = Color::Rgb(44, 44, 48);
pub const IOS_ICON_CHIP: Color = Color::Rgb(70, 70, 76);

// Tint variants
pub const IOS_TINT_DARK: Color = Color::Rgb(7, 100, 220);
pub const IOS_TINT_SUB: Color = Color::Rgb(210, 224, 255);

#[cfg(test)]
mod tests {
    use super::*;

    /// Hex-table parity guard. If any const drifts, this asserts which one.
    #[test]
    fn palette_hex_parity() {
        assert_eq!(IOS_TINT,        Color::Rgb(0x0a, 0x84, 0xff), "IOS_TINT must be #0a84ff");
        assert_eq!(IOS_DESTRUCTIVE, Color::Rgb(0xff, 0x45, 0x3a), "IOS_DESTRUCTIVE must be #ff453a");
        assert_eq!(IOS_GREEN,       Color::Rgb(0x30, 0xd1, 0x58), "IOS_GREEN must be #30d158");
        assert_eq!(IOS_ORANGE,      Color::Rgb(0xff, 0x9f, 0x0a), "IOS_ORANGE must be #ff9f0a");
        assert_eq!(IOS_YELLOW,      Color::Rgb(0xff, 0xd6, 0x0a), "IOS_YELLOW must be #ffd60a");
        assert_eq!(IOS_PURPLE,      Color::Rgb(0xbf, 0x5a, 0xf2), "IOS_PURPLE must be #bf5af2");
        assert_eq!(IOS_FG,          Color::Rgb(0xf2, 0xf2, 0xf7), "IOS_FG must be #f2f2f7");
        assert_eq!(IOS_BG_SOLID,    Color::Rgb(0x1c, 0x1c, 0x1e), "IOS_BG_SOLID must be #1c1c1e");
    }
}
