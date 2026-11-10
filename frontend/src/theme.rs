//! Streamerz's shared zinc palette, applied to the component family once.
use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::{px, rgb, App};
pub fn apply(dark: bool, cx: &mut App) {
    Theme::change(if dark { ThemeMode::Dark } else { ThemeMode::Light }, None, cx);
    Theme::update(cx, |theme| {
        theme.font_size = px(13.);
        theme.font_family = "Streamerz Sans".into();
        let colors = &mut theme.colors;
        colors.background = rgb(if dark { 0x18181b } else { 0xf4f4f5 }).into();
        colors.foreground = rgb(if dark { 0xf4f4f5 } else { 0x09090b }).into();
        colors.border = rgb(if dark { 0x3f3f46 } else { 0xd4d4d8 }).into();
        colors.muted = rgb(if dark { 0x27272a } else { 0xe4e4e7 }).into();
        colors.muted_foreground = rgb(if dark { 0x919196 } else { 0x71717a }).into();
        // The window behind a dialog is dimmed, as 1.6.0 darkens and blurs what is behind its dialogs.
        colors.overlay = gpui_kit::hsla(0., 0., 0., if dark { 0.5 } else { 0.3 });
        colors.accent = colors.muted;
        colors.accent_foreground = colors.foreground;
        colors.sidebar = rgb(if dark { 0x101013 } else { 0xf9f9fa }).into();
        colors.table = rgb(if dark { 0x09090b } else { 0xffffff }).into();
        colors.table_head = colors.background;
        colors.table_head_foreground = colors.muted_foreground;
        colors.table_row_border = colors.table;
        colors.table_even = colors.table;
        colors.table_active = colors.muted;
        colors.table_hover = colors.muted;
        colors.list_active = colors.muted;
        colors.list_active_border = colors.muted;
        colors.list_hover = colors.muted;
        colors.primary = rgb(if dark { 0x3b82f6 } else { 0x2563eb }).into();
        colors.primary_foreground = rgb(0xffffff).into();
        colors.switch = rgb(if dark { 0x4f4f51 } else { 0xb9b9ba }).into();
        colors.switch_thumb = rgb(0xffffff).into();
        colors.button = rgb(if dark { 0x2e2e30 } else { 0xdcdcdd }).into();
        colors.input = rgb(if dark { 0x4f4f51 } else { 0xb5b5b5 }).into();
        colors.button_foreground = colors.foreground;
    });
}

/// The bright text of headings (1.6.0: near white on dark, near black on light).
pub fn strong(cx: &App) -> gpui_kit::Hsla {
    use gpui_kit::component::ActiveTheme;
    rgb(if cx.theme().is_dark() { 0xf4f4f5 } else { 0x09090b }).into()
}

/// The quiet text of descriptions and table headers.
pub fn quiet(cx: &App) -> gpui_kit::Hsla {
    use gpui_kit::component::ActiveTheme;
    rgb(if cx.theme().is_dark() { 0x7e7e80 } else { 0x848485 }).into()
}
