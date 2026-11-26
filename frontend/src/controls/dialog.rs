//! One shape for every dialog: a header (its title, and the close button the toolkit puts at its right),
//! the body, and a footer that holds the buttons, each part set apart from the next by a line. Pages fill
//! the parts; they never draw a dialog's frame themselves.
use crate::theme::{CONTROL_HEIGHT, FONT_BODY, LINE_BODY, SPACE_LG, SPACE_MD, SPACE_SM, SPACE_XS};
use gpui_kit::component::{
    button::{Button, ButtonVariants},
    dialog::Dialog,
    ActiveTheme, WindowExt,
};
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

/// Asks before an operation (Delete, Remove): `question` in the body, Cancel and a button named
/// `label` (red when `danger`) that runs `on_confirm` and closes the dialog. The buttons are one
/// group named for screen readers and the UI checks (`action-confirmation`), the same on every page.
pub fn confirm(
    window: &mut Window,
    cx: &mut App,
    label: String,
    question: String,
    danger: bool,
    on_confirm: impl Fn(&mut Window, &mut App) + 'static,
) {
    let on_confirm = std::rc::Rc::new(on_confirm);
    window.open_dialog(cx, move |dialog, window, cx| {
        let run = on_confirm.clone();
        let confirm = Button::new("confirm-action")
            .label(label.clone())
            .on_click(move |_, window, cx| {
                run(window, cx);
                window.close_dialog(cx);
            });
        let buttons = div()
            .id("action-confirmation")
            .test_support()
            .role(Role::Group)
            .aria_label(label.clone())
            .flex()
            .justify_end()
            .gap(px(SPACE_SM))
            .child(
                Button::new("cancel-action")
                    .label(crate::i18n::tr("dialogs.cancel"))
                    .on_click(|_, window, cx| dismiss(window, cx)),
            )
            .child(if danger { confirm.danger() } else { confirm });
        let buttons = footer(None, buttons, cx);
        shell(dialog, label.clone(), 448., window, cx)
            .child(body(question.clone()))
            .footer(buttons)
    });
}

/// The footer of a dialog that only shows something: its Close button.
pub fn close_footer(cx: &App) -> Div {
    footer(
        None,
        Button::new("dialog-close")
            .h(px(CONTROL_HEIGHT))
            .label(crate::i18n::tr("dialogs.close"))
            .on_click(|_, window, cx| dismiss(window, cx)),
        cx,
    )
}

#[cfg(all(test, feature = "test-support"))]
mod confirm_tests {
    use gpui_kit::component::WindowExt;
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{
        div, px, size, AppContext, Bounds, IntoElement, Render, Role, TestAppContext, WindowBounds, WindowOptions,
    };
    use std::{cell::Cell, rc::Rc};

    struct Host;
    impl Render for Host {
        fn render(&mut self, _: &mut gpui_kit::Window, _: &mut gpui_kit::Context<Self>) -> impl IntoElement {
            div()
        }
    }

    #[gpui_kit::test]
    fn a_confirmation_is_named_and_runs_its_operation_once_asked(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::theme::apply(true, cx);
        });
        let window = cx.update(|cx| {
            let bounds = Bounds {
                origin: gpui_kit::Point::default(),
                size: size(px(800.), px(600.)),
            };
            let (window, _) = gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                cx,
                |_, cx| cx.new(|_| Host),
            )
            .expect("open test window");
            window.downcast::<gpui_kit::base::Root>().expect("root window")
        });
        let done = Rc::new(Cell::new(0));
        let counted = done.clone();
        cx.update_window(window.into(), |_, window, cx| {
            super::confirm(
                window,
                cx,
                "Delete overlay".into(),
                "Delete Alerts?".into(),
                true,
                move |_, _| counted.set(counted.get() + 1),
            );
            window.render_frame(cx);
            let group = window.find("action-confirmation");
            assert_eq!(group.role(), Some(Role::Group));
            assert_eq!(group.label(), Some("Delete overlay"));
            assert!(window.has_active_dialog(cx));
            window.click("confirm-action", cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(window.into(), |_, window, cx| {
            assert_eq!(done.get(), 1);
            assert!(!window.has_active_dialog(cx));
        })
        .unwrap();
    }
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
