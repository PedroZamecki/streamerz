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
        // The window behind a dialog is dimmed.
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

/// Page title (H1) size and line height.
pub const FONT_PAGE_TITLE: f32 = 18.;
pub const LINE_PAGE_TITLE: f32 = 24.;
/// Section heading (H2) size and line height.
pub const FONT_SECTION_TITLE: f32 = 15.;
pub const LINE_SECTION_TITLE: f32 = 20.;
/// Primary body, label, and subtitle size and line height.
pub const FONT_BODY: f32 = 13.;
pub const LINE_BODY: f32 = 18.;
/// Caption, table header, and helper description size and line height.
pub const FONT_CAPTION: f32 = 12.;
pub const LINE_CAPTION: f32 = 16.;
/// Micro badge and sidebar group label size and line height.
pub const FONT_MICRO: f32 = 11.;
pub const LINE_MICRO: f32 = 14.;
/// Standard spacing rhythm tokens.
pub const SPACE_XS: f32 = 4.;
pub const SPACE_SM: f32 = 8.;
pub const SPACE_MD: f32 = 12.;
pub const SPACE_LG: f32 = 16.;
/// Outer horizontal and vertical gutters for the page content frame.
pub const PAGE_PAD_X: f32 = SPACE_LG;
pub const PAGE_PAD_TOP: f32 = SPACE_LG;
pub const PAGE_PAD_BOTTOM: f32 = SPACE_MD;
/// Vertical rhythm between major sections on a page.
pub const SECTION_GAP: f32 = SPACE_LG;
/// Height of a button, an input and a table row.
pub const CONTROL_HEIGHT: f32 = 28.;
/// Height of a button in a dense row (a bordered 28px row has 26px inside).
pub const DENSE_CONTROL_HEIGHT: f32 = 26.;
/// Height of a sidebar item (the list has 25 pages, so it is tighter than a button).
pub const NAV_ITEM_HEIGHT: f32 = 24.;
/// The sidebar's width, open and folded to icons.
pub const SIDEBAR_WIDTH: f32 = 176.;
pub const SIDEBAR_FOLDED_WIDTH: f32 = 52.;

/// The bright text of headings: near white on dark, near black on light.
pub fn strong(cx: &App) -> gpui_kit::Hsla {
    use gpui_kit::component::ActiveTheme;
    rgb(if cx.theme().is_dark() { 0xf4f4f5 } else { 0x09090b }).into()
}

/// The quiet text of descriptions and table headers.
pub fn quiet(cx: &App) -> gpui_kit::Hsla {
    use gpui_kit::component::ActiveTheme;
    rgb(if cx.theme().is_dark() { 0x7e7e80 } else { 0x848485 }).into()
}

/// Muted icon tone inside buttons and headers.
pub fn icon_tone(cx: &App) -> gpui_kit::Hsla {
    use gpui_kit::component::ActiveTheme;
    rgb(if cx.theme().is_dark() { 0xbdbdbe } else { 0x434345 }).into()
}

/// Inactive navigation item foreground.
pub fn nav_foreground(cx: &App) -> gpui_kit::Hsla {
    use gpui_kit::component::ActiveTheme;
    rgb(if cx.theme().is_dark() { 0xb4b4b5 } else { 0x4c4c4d }).into()
}

/// Active navigation item background.
pub fn nav_selected_bg(cx: &App) -> gpui_kit::Hsla {
    use gpui_kit::component::ActiveTheme;
    rgb(if cx.theme().is_dark() { 0x2b2b2e } else { 0xdcdcdd }).into()
}

/// Subtle divider inside the sidebar (between folded groups and above the footer).
pub fn nav_divider(cx: &App) -> gpui_kit::Hsla {
    use gpui_kit::component::ActiveTheme;
    rgb(if cx.theme().is_dark() { 0x2b2b2e } else { 0xdcdcdd }).into()
}

/// Right edge border of the sidebar.
pub fn sidebar_border(cx: &App) -> gpui_kit::Hsla {
    use gpui_kit::component::ActiveTheme;
    rgb(if cx.theme().is_dark() { 0x2b2b2e } else { 0xd4d4d8 }).into()
}

/// Surface fill for standalone list/setting/connection rows.
pub fn row_surface(cx: &App) -> gpui_kit::Hsla {
    use gpui_kit::component::ActiveTheme;
    rgb(if cx.theme().is_dark() { 0x18181b } else { 0xf4f4f5 }).into()
}

/// Hover fill for standalone list/setting/connection rows.
pub fn row_surface_hover(cx: &App) -> gpui_kit::Hsla {
    use gpui_kit::component::ActiveTheme;
    rgb(if cx.theme().is_dark() { 0x222226 } else { 0xebebee }).into()
}

/// Border color for standalone list/setting/connection rows.
pub fn row_border(cx: &App) -> gpui_kit::Hsla {
    use gpui_kit::component::ActiveTheme;
    rgb(if cx.theme().is_dark() { 0x27272a } else { 0xe4e4e7 }).into()
}

/// Pill and micro badge background.
pub fn badge_bg(cx: &App) -> gpui_kit::Hsla {
    use gpui_kit::component::ActiveTheme;
    rgb(if cx.theme().is_dark() { 0x27272a } else { 0xe4e4e7 }).into()
}

/// Accent foreground for locations, endpoints, and detail links.
pub fn link_color(cx: &App) -> gpui_kit::Hsla {
    use gpui_kit::component::ActiveTheme;
    rgb(if cx.theme().is_dark() { 0x93c5fd } else { 0x1d4ed8 }).into()
}

/// Height of a header row inside a page: a table head, a chat column head, a short bar.
pub const HEADER_ROW_HEIGHT: f32 = 24.;
