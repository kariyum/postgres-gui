use iced::theme::Palette;
use iced::{Color, Theme};

pub const TEXT: Color = Color::from_rgb8(0xe1, 0xe4, 0xf0);
pub const TEXT_MUTED: Color = Color::from_rgb8(0x83, 0x89, 0xa3);
pub const PRIMARY: Color = Color::from_rgb8(0x6c, 0x8c, 0xff);
pub const SUCCESS: Color = Color::from_rgb8(0x4f, 0xde, 0x6c);
pub const DANGER: Color = Color::from_rgb8(0xff, 0x55, 0x75);

pub fn create() -> Theme {
    Theme::custom(
        "pgeru",
        Palette {
            background: Color::from_rgb8(0x0f, 0x11, 0x1a),
            text: TEXT,
            primary: PRIMARY,
            success: SUCCESS,
            danger: DANGER,
            warning: Color::from_rgb8(0xf0, 0xa0, 0x40),
        },
    )
}
