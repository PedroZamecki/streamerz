//! Connection and resource health: the zinc palette's state colours and Lucide icons, in both themes.
use gpui_kit::component::ActiveTheme;
use gpui_kit::{rgb, App, Hsla};

pub fn appearance(state: &str, cx: &App) -> (&'static str, Hsla) {
    let dark = cx.theme().is_dark();
    let (icon, dark_color, light_color) = match state {
        "up" => ("circle-check", 0x3cb371, 0x166534),
        "down" => ("circle-alert", 0xe06050, 0xb91c1c),
        "busy" => ("loader-circle", 0xe0a830, 0x92400e),
        "locked" => ("lock", 0xe0a830, 0x92400e),
        "standby" => ("moon", 0x64b5f6, 0x1d4ed8),
        _ => ("circle-off", 0x999999, 0x52525b),
    };
    (icon, rgb(if dark { dark_color } else { light_color }).into())
}
