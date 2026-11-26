//! Shared visible and accessible validation, info, warning, and structured error banners.
use crate::error::{ActiveError, StreamerzError};
use gpui_kit::component::{ActiveTheme, Icon};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

/// The icon's box at the start of a message: one body line high, the icon centred in it.
fn icon_slot() -> Div {
    div()
        .h(px(crate::theme::LINE_BODY))
        .flex()
        .items_center()
        .flex_shrink_0()
}

/// A caption under a heading or a button row (a hint, a scope warning): 12/16, quiet.
pub fn note(text: &str, cx: &App) -> Div {
    div()
        .w_full()
        .min_w_0()
        .text_size(px(crate::theme::FONT_CAPTION))
        .line_height(px(crate::theme::LINE_CAPTION))
        .text_color(crate::theme::quiet(cx))
        .child(text.to_owned())
}

/// Render a structured `StreamerzError` card with title, description, and recommendations.
pub fn error(err: &StreamerzError, cx: &App) -> impl IntoElement {
    error_card("validation-error", err, None, None, cx)
}

/// Render a persistent active error banner with optional recovery action and dismiss button.
pub fn active_banner(
    index: usize,
    active: &ActiveError,
    action_button: Option<AnyElement>,
    dismiss_button: AnyElement,
    cx: &App,
) -> impl IntoElement {
    error_card(
        ("active-error-banner", index),
        &active.error,
        action_button,
        Some(dismiss_button),
        cx,
    )
}

fn error_card(
    id: impl Into<ElementId>,
    err: &StreamerzError,
    action_button: Option<AnyElement>,
    dismiss_button: Option<AnyElement>,
    cx: &App,
) -> impl IntoElement {
    let recs = err.all_recommendations();
    let rec_prefix = crate::error::tr_or("error.recommended_prefix", "Recommended: ");
    let mut card = div()
        .id(id)
        .test_support()
        .role(Role::Alert)
        .aria_label(err.full_label())
        .flex()
        .flex_col()
        .flex_shrink_0()
        .min_w_0()
        .gap(px(crate::theme::SPACE_XS))
        .px(px(crate::theme::SPACE_MD))
        .py(px(crate::theme::SPACE_SM))
        .rounded(px(6.))
        .border_1()
        .border_color(cx.theme().danger.opacity(0.42))
        .bg(cx.theme().danger.opacity(0.10))
        .child(
            div()
                .flex()
                .items_start()
                .gap(px(crate::theme::SPACE_SM))
                .min_w_0()
                .child(
                    icon_slot()
                        .text_color(cx.theme().danger)
                        .child(Icon::empty().path("icons/triangle-alert.svg").size_3p5()),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(px(crate::theme::FONT_BODY))
                        .line_height(px(crate::theme::LINE_BODY))
                        .font_weight(FontWeight::BOLD)
                        .text_color(cx.theme().danger)
                        .child(err.title.clone()),
                )
                .when_some(dismiss_button, |header, btn| header.child(btn)),
        )
        .child(
            div()
                .pl(px(22.))
                .min_w_0()
                .text_size(px(crate::theme::FONT_BODY))
                .line_height(px(crate::theme::LINE_BODY))
                .text_color(cx.theme().foreground)
                .child(err.description.clone()),
        );

    if let Some((first, rest)) = recs.split_first() {
        let mut rec_box = div()
            .pl(px(22.))
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(crate::theme::SPACE_XS))
            .text_size(px(crate::theme::FONT_CAPTION))
            .line_height(px(crate::theme::LINE_CAPTION))
            .text_color(cx.theme().muted_foreground)
            .child(div().min_w_0().child(format!("{rec_prefix}{first}")));
        for extra in rest {
            rec_box = rec_box.child(div().min_w_0().child(format!("- {extra}")));
        }
        card = card.child(rec_box);
    }

    if let Some(action) = action_button {
        card = card.child(
            div()
                .pl(px(22.))
                .pt(px(crate::theme::SPACE_XS))
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(crate::theme::SPACE_SM))
                .child(action),
        );
    }

    card
}

/// Estimate wrapped vertical height of an error card for form sizing.
pub fn estimated_error_height(err: &StreamerzError, width: f32) -> f32 {
    let wrap_lines = |text: &str, line: f32, avail: f32| -> f32 {
        let chars_per_line = (avail / 6.8).max(1.);
        text.lines()
            .map(|l| line * (l.chars().count() as f32 / chars_per_line).ceil().max(1.))
            .sum::<f32>()
            .max(line)
    };
    let inner = (width - 44.).max(80.);
    let mut height = 22.0;
    height += wrap_lines(&err.title, 18., inner) + 4.;
    height += wrap_lines(&err.description, 18., inner) + 4.;
    for rec in err.all_recommendations() {
        height += wrap_lines(rec, 16., inner) + 3.;
    }
    height
}

/// A quiet note: an info icon and a sentence in the muted colour (why something is empty or greyed out).
pub fn info(message: &str, cx: &App) -> Div {
    div()
        .w_full()
        .min_w_0()
        .flex()
        .items_start()
        .gap(px(crate::theme::SPACE_SM))
        .text_size(px(crate::theme::FONT_BODY))
        .line_height(px(crate::theme::LINE_BODY))
        .text_color(cx.theme().muted_foreground)
        .child(icon_slot().child(Icon::empty().path("icons/info.svg").size_3p5()))
        .child(div().flex_1().min_w_0().child(message.to_owned()))
}

/// A visible warning banner: a warning icon and a sentence for broken or skipped options.
pub fn warn(id: impl Into<ElementId>, message: &str, cx: &App) -> impl IntoElement {
    div()
        .id(id)
        .test_support()
        .role(Role::Alert)
        .aria_label(message.to_owned())
        .flex()
        .flex_shrink_0()
        .items_start()
        .gap(px(crate::theme::SPACE_SM))
        .px(px(crate::theme::SPACE_MD))
        .py(px(crate::theme::SPACE_SM))
        .rounded(px(6.))
        .border_1()
        .border_color(cx.theme().warning.opacity(0.35))
        .bg(cx.theme().warning.opacity(0.12))
        .text_size(px(crate::theme::FONT_BODY))
        .line_height(px(crate::theme::LINE_BODY))
        .text_color(cx.theme().foreground)
        .child(
            icon_slot()
                .text_color(cx.theme().warning)
                .child(Icon::empty().path("icons/triangle-alert.svg").size_3p5()),
        )
        .child(div().flex_1().min_w_0().child(message.to_owned()))
}
