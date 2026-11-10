//! One shape for every dialog: a header (its title, and the close button the toolkit puts at its right),
//! the body, and a footer that holds the buttons, each part set apart from the next by a line. Pages fill
//! the parts; they never draw a dialog's frame themselves.
use gpui_kit::component::{dialog::Dialog, ActiveTheme, WindowExt};
use gpui_kit::*;

/// The header's height: the close button sits in its middle.
const HEAD: f32 = 40.;

/// A dialog set up with this app's header, `width` px wide and centred over the dimmed window. Its body
/// goes in with `.child(body(...))` and its buttons with `.footer(footer(...))`.
pub fn shell(dialog: Dialog, title: impl Into<SharedString>, width: f32, cx: &App) -> Dialog {
    let theme = cx.theme();
    dialog
        .centered()
        .pt(px(0.))
        .pb(px(0.))
        .pl(px(0.))
        .pr(px(0.))
        .width(px(width))
        .shadow_lg()
        .title(
            div()
                .h(px(HEAD))
                .flex()
                .items_center()
                .px_4()
                .pr(px(44.))
                .border_b_1()
                .border_color(theme.border)
                .text_size(px(13.))
                .font_weight(FontWeight::BOLD)
                .text_color(theme.foreground)
                .child(div().min_w_0().truncate().child(title.into())),
        )
}

/// A dialog's content, inset from its edges.
pub fn body(content: impl IntoElement) -> Div {
    div().px_4().pb_1().flex().flex_col().gap_2().child(content)
}

/// A dialog's buttons: at its bottom, under a line, the main ones at the right. `left` holds what stands
/// apart from them (Edit JSON).
pub fn footer(left: Option<AnyElement>, right: impl IntoElement, cx: &App) -> Div {
    div()
        .flex()
        .items_center()
        .gap_2()
        .px_4()
        .py_3()
        .border_t_1()
        .border_color(cx.theme().border)
        .children(left)
        .child(div().flex_1())
        .child(div().flex().items_center().gap_2().child(right))
}

/// The footer of a dialog that only shows something: its Close button.
pub fn close_footer(cx: &App) -> Div {
    use gpui_kit::component::button::Button;
    footer(
        None,
        Button::new("dialog-close")
            .label(crate::i18n::tr("dialogs.close"))
            .on_click(|_, window, cx| window.close_dialog(cx)),
        cx,
    )
}
