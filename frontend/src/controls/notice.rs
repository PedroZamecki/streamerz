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

/// A quiet note: an info icon and a sentence in the muted colour (why something is empty or greyed out).
pub fn info(message: &str, cx: &App) -> Div {
    use gpui_kit::component::Icon;
    div()
        .flex()
        .items_start()
        .gap_2()
        .text_size(px(12.5))
        .line_height(px(18.))
        .text_color(cx.theme().muted_foreground)
        .child(
            div()
                .pt(px(2.))
                .flex_shrink_0()
                .child(Icon::empty().path("icons/info.svg").size_3p5()),
        )
        .child(div().flex_1().min_w_0().child(message.to_owned()))
}
