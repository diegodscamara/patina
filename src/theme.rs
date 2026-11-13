//! Patina's colours, taken from the design canvas.
//!
//! The accent is olive drab, the green of a Jeep Willys, lightened a little so
//! it stays readable on the dark graphite background.

use iced::theme::Palette;
use iced::{Color, Theme};

const fn hex(rgb: u32) -> Color {
    Color::from_rgb(
        ((rgb >> 16) & 0xFF) as f32 / 255.0,
        ((rgb >> 8) & 0xFF) as f32 / 255.0,
        (rgb & 0xFF) as f32 / 255.0,
    )
}

/// Window background.
pub const GRAPHITE: Color = hex(0x131211);
/// Sidebar and player bar.
pub const SURFACE: Color = hex(0x1A1816);
/// Raised elements: selected rows, chips.
pub const RAISED: Color = hex(0x2A2622);
/// Hairlines between regions.
pub const BORDER: Color = hex(0x2A2622);
/// Primary text.
pub const BONE: Color = hex(0xF2EEE9);
/// Secondary text.
pub const ASH: Color = hex(0xA39C93);
/// The accent: Willys olive, lightened for contrast.
pub const OLIVE: Color = hex(0x7A8A4A);
/// True Willys olive drab, for darker accent surfaces.
#[allow(dead_code)] // for darker accent surfaces, not used yet
pub const OLIVE_DRAB: Color = hex(0x4B5320);

pub fn theme() -> Theme {
    Theme::custom(
        "Patina",
        Palette {
            background: GRAPHITE,
            text: BONE,
            primary: OLIVE,
            success: OLIVE,
            warning: hex(0xD4A24C),
            danger: hex(0xC96B6B),
        },
    )
}
