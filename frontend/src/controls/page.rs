//! Shared page chrome: consistent H1 header, divider, and guide typography across all pages.
use crate::theme;
use gpui_kit::component::{ActiveTheme, Icon};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

/// The standard H1 page header drawn at the top of every page except Chat: the page's icon and title, a
/// line under them, and the page-wide actions (Add, Pause all) at the right.
pub fn header(icon: Option<&str>, title: String, actions: Option<AnyElement>, cx: &App) -> Div {
    div()
        .w_full()
        .min_w_0()
        .pb(px(theme::SPACE_SM))
        .border_b_1()
        .border_color(cx.theme().border.opacity(0.6))
        .flex_shrink_0()
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(theme::SPACE_SM))
                .min_h(px(theme::CONTROL_HEIGHT))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(theme::SPACE_SM))
                        .min_w_0()
                        .when_some(icon.filter(|name| !name.is_empty()), |row, icon| {
                            row.child(
                                Icon::empty()
                                    .path(format!("icons/{icon}.svg"))
                                    .size_4()
                                    .text_color(theme::icon_tone(cx))
                                    .flex_shrink_0(),
                            )
                        })
                        .child(
                            div()
                                .min_w_0()
                                .truncate()
                                .text_size(px(theme::FONT_PAGE_TITLE))
                                .line_height(px(theme::LINE_PAGE_TITLE))
                                .font_weight(FontWeight::BOLD)
                                .text_color(theme::strong(cx))
                                .child(title),
                        ),
                )
                .when_some(actions, |row, actions| row.child(div().flex_shrink_0().child(actions))),
        )
}

/// Quiet page or section guide copy with uniform body sizing and line height.
pub fn guide(text: &str, cx: &App) -> Div {
    div()
        .min_w_0()
        .text_size(px(theme::FONT_BODY))
        .line_height(px(theme::LINE_BODY))
        .text_color(theme::quiet(cx))
        .child(text.to_owned())
}
