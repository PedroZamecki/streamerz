//! Shared visible and accessible validation messages.
use gpui_kit::component::ActiveTheme;
use gpui_kit::*;

pub fn error(message: &str, cx: &App) -> impl IntoElement {
    div()
        .id("validation-error")
        .test_support()
        .role(Role::Alert)
        .aria_label(message.to_owned())
        .text_color(cx.theme().danger)
        .child(message.to_owned())
}
