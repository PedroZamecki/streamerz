//! One shape for every dialog: a header (its title, and the close button the toolkit puts at its right),
//! the body, and a footer that holds the buttons, each part set apart from the next by a line. Pages fill
//! the parts; they never draw a dialog's frame themselves.
use crate::theme::{CONTROL_HEIGHT, FONT_BODY, LINE_BODY, SPACE_LG, SPACE_MD, SPACE_SM, SPACE_XS};
use gpui_kit::component::{dialog::Dialog, ActiveTheme};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

/// The header's height: the close button sits in its middle.
const HEAD: f32 = 40.;

fn clamp_width(preferred: f32, viewport_width: f32) -> f32 {
    (viewport_width - SPACE_LG * 2.).min(preferred).max(280.)
}

fn clamp_height(reserve: f32, min: f32, max: f32, viewport_height: f32) -> f32 {
    (viewport_height - reserve).clamp(min, max.max(min))
}

/// Width clamped to the window so dialogs keep margin on both sides even on narrow windows.
pub fn fit_width(preferred: f32, window: &Window) -> f32 {
    clamp_width(preferred, f32::from(window.viewport_size().width))
}

/// Height bounded by the window after reserving `reserve` px for dialog chrome and margins.
pub fn fit_height(reserve: f32, min: f32, max: f32, window: &Window) -> f32 {
    clamp_height(reserve, min, max, f32::from(window.viewport_size().height))
}

/// A dialog set up with this app's header, at most `width` px wide and centred over the dimmed window.
/// Its body goes in with `.child(body(...))` and its buttons with `.footer(footer(...))`.
pub fn shell(dialog: Dialog, title: impl Into<SharedString>, width: f32, window: &Window, cx: &App) -> Dialog {
    let theme = cx.theme();
    let title: SharedString = title.into();
    let title_tip = title.clone();
    dialog
        .centered()
        .pt(px(0.))
        .pb(px(0.))
        .pl(px(0.))
        .pr(px(0.))
        .width(px(fit_width(width, window)))
        .shadow_lg()
        .title(
            div()
                .h(px(HEAD))
                .flex()
                .items_center()
                .pl(px(SPACE_LG))
                .pr(px(44.))
                .border_b_1()
                .border_color(theme.border)
                .text_size(px(FONT_BODY))
                .line_height(px(LINE_BODY))
                .font_weight(FontWeight::BOLD)
                .child(
                    div()
                        .id("dialog-title")
                        .test_support()
                        .aria_label(title.clone())
                        .min_w_0()
                        .truncate()
                        .child(title)
                        .when(!title_tip.is_empty(), |el| {
                            el.tooltip(move |window, cx| {
                                gpui_kit::component::tooltip::Tooltip::new(title_tip.clone()).build(window, cx)
                            })
                        }),
                ),
        )
}

/// A dialog's content, inset from its edges. The toolkit already leaves 8px above and below the body, so
/// its own padding is the 4px that makes 12px between the body and the lines of the header and footer.
pub fn body(content: impl IntoElement) -> Div {
    div()
        .px(px(SPACE_LG))
        .pt(px(SPACE_XS))
        .pb(px(SPACE_XS))
        .text_size(px(FONT_BODY))
        .line_height(px(LINE_BODY))
        .flex()
        .flex_col()
        .gap(px(SPACE_SM))
        .child(content)
}

/// A dialog's buttons: at its bottom, under a line, the main ones at the right. `left` holds what stands
/// apart from them (Edit JSON).
pub fn footer(left: Option<AnyElement>, right: impl IntoElement, cx: &App) -> Div {
    div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(SPACE_SM))
        .px(px(SPACE_LG))
        .py(px(SPACE_MD))
        .border_t_1()
        .border_color(cx.theme().border)
        .children(left)
        .child(div().flex_1())
        .child(
            div()
                .min_w_0()
                .flex()
                .flex_wrap()
                .justify_end()
                .items_center()
                .gap(px(SPACE_SM))
                .child(right),
        )
}

/// Closes the open dialog the way its X and Escape do: its `on_cancel` runs first (it forgets the
/// dialog, or keeps it open while something is pending). A plain `close_dialog` skips it, and a page
/// that still thinks its dialog is open never opens it again. Every Cancel and Close button uses this.
pub fn dismiss(window: &mut Window, cx: &mut App) {
    window.dispatch_action(Box::new(gpui_kit::base::actions::Cancel), cx);
}

/// The footer of a dialog that only shows something: its Close button.
pub fn close_footer(cx: &App) -> Div {
    use gpui_kit::component::button::Button;
    footer(
        None,
        Button::new("dialog-close")
            .h(px(CONTROL_HEIGHT))
            .label(crate::i18n::tr("dialogs.close"))
            .on_click(|_, window, cx| dismiss(window, cx)),
        cx,
    )
}

#[cfg(test)]
mod tests {
    use super::{clamp_height, clamp_width};

    #[test]
    fn dialogs_clamp_to_narrow_and_short_viewports() {
        assert_eq!(clamp_width(760., 1100.), 760.);
        assert_eq!(clamp_width(760., 540.), 508.);
        assert_eq!(clamp_width(560., 540.), 508.);
        assert_eq!(clamp_width(448., 540.), 448.);
        assert_eq!(clamp_height(180., 180., 680., 800.), 620.);
        assert_eq!(clamp_height(180., 180., 680., 320.), 180.);
    }
}
