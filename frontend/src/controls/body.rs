//! The inside of a button: a 14px icon in a quiet tone, then the label at 13px.
use gpui_kit::component::{ActiveTheme, Icon};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

/// The icon tone of buttons (dimmer than their label).
pub fn icon_tone(cx: &App) -> Hsla {
    rgb(if cx.theme().is_dark() { 0xbdbdbe } else { 0x434345 }).into()
}

/// Put it in a button with `.child(...)`, not `.label(...)`, so the label keeps the page's 13px size.
pub fn button_body(icon: Option<&str>, label: Option<String>, cx: &App) -> Div {
    div()
        .flex()
        .items_center()
        .gap_2()
        .text_size(px(13.))
        .when_some(icon, |body, icon| {
            body.child(
                Icon::empty()
                    .path(format!("icons/{icon}.svg"))
                    .size(px(14.))
                    .text_color(icon_tone(cx)),
            )
        })
        .when_some(label, |body, label| body.child(label))
}
