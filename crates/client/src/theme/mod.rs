pub mod colors;

use iced::theme::Palette;
use iced::Theme;

use colors::*;

pub fn tephra_theme() -> Theme {
    Theme::custom("Tephra".to_string(), Palette {
        background: OBSIDIAN,
        text: PUMICE,
        primary: EMBER,
        success: GEOTHERMAL,
        warning: LAVA,
        danger: MAGMA,
    })
}
