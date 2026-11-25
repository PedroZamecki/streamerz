//! A page's "Show" selector: the core lists the tables a page can show (`tabs`), one is drawn at a time.
use crate::theme;
use gpui_kit::component::{
    tab::{Tab, TabBar},
    ActiveTheme, Sizable,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use serde_json::Value;

/// The kinds of the tabs, in order, and the one selected.
pub fn kinds(tabs: &Value) -> (Vec<String>, usize) {
    let kinds: Vec<String> = tabs["options"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|option| option["id"].as_str().map(str::to_owned))
        .collect();
    let selected = kinds
        .iter()
        .position(|kind| tabs["selected"] == kind.as_str())
        .unwrap_or(0);
    (kinds, selected)
}

/// About how wide the label and the segmented bar are on one line: a page puts the bar in its toolbar
/// when this and the rest of the toolbar fit.
pub fn inline_width(tabs: &Value) -> f32 {
    let label = tabs["label"].as_str().unwrap_or_default().chars().count() as f32 * 7.4;
    let bar: f32 = tabs["options"].as_array().into_iter().flatten().map(option_width).sum();
    label + theme::SPACE_SM + bar
}

fn option_width(option: &Value) -> f32 {
    28. + option["label"].as_str().unwrap_or_default().chars().count() as f32 * 7.4
}

/// The label and a segmented bar (or unified wrapping tray when many options wrap); `select` gets the kind that was picked.
pub fn bar(
    tabs: &Value,
    window: &Window,
    cx: &App,
    select: impl Fn(String, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let (kinds, selected) = kinds(tabs);
    let options = tabs["options"].as_array().map_or(&[][..], Vec::as_slice);
    let select = std::rc::Rc::new(select);
    // Reserve the sidebar, page insets and the inline "Show:" label beside the segmented bar.
    let viewport = f32::from(window.viewport_size().width);
    let label_w = tabs["label"].as_str().unwrap_or_default().chars().count() as f32 * 7.4 + theme::SPACE_SM;
    let room = (viewport - if viewport < 700. { 96. } else { 230. } - label_w).max(150.);
    let widths: Vec<_> = options.iter().map(option_width).collect();
    let ranges = wrap_ranges(&widths, room);
    let control = if ranges.len() <= 1 {
        let callback = select.clone();
        let kinds = kinds.clone();
        div()
            .h(px(theme::CONTROL_HEIGHT))
            .flex()
            .items_center()
            .flex_shrink_0()
            .child(
                TabBar::new("page-tabs-0")
                    .segmented()
                    .small()
                    .selected_index(if selected < options.len() { selected } else { usize::MAX })
                    .children(options.iter().map(|option| {
                        let label = option["label"].as_str().unwrap_or_default().to_owned();
                        Tab::new().aria_label(label.clone()).label(label)
                    }))
                    .on_click(move |index, window, cx| {
                        if let Some(kind) = kinds.get(*index) {
                            callback(kind.clone(), window, cx);
                        }
                    }),
            )
            .into_any_element()
    } else {
        let mut tray = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(theme::SPACE_XS))
            // A segmented control's own inner padding: its height is the 28 of a button when on one line.
            .p(px(2.))
            .w_full()
            .min_w_0()
            .rounded(px(8.))
            .bg(cx.theme().sidebar)
            .border_1()
            .border_color(cx.theme().border);
        for (index, option) in options.iter().enumerate() {
            let label = option["label"].as_str().unwrap_or_default().to_owned();
            let active = index == selected;
            let callback = select.clone();
            let kind = kinds.get(index).cloned().unwrap_or_default();
            tray = tray.child(
                div()
                    .id(("page-tab", index))
                    .test_support()
                    .role(Role::Tab)
                    .aria_label(label.clone())
                    .cursor_pointer()
                    .h(px(theme::CONTROL_HEIGHT - 6.))
                    .flex()
                    .items_center()
                    .px(px(theme::SPACE_SM))
                    .rounded(px(6.))
                    .text_size(px(theme::FONT_CAPTION))
                    .line_height(px(theme::LINE_CAPTION))
                    .when(active, |pill| {
                        pill.bg(cx.theme().background)
                            .border_1()
                            .border_color(cx.theme().border)
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme::strong(cx))
                    })
                    .when(!active, |pill| {
                        pill.border_1()
                            .border_color(gpui_kit::transparent_black())
                            .text_color(theme::quiet(cx))
                            .hover(|el| el.bg(cx.theme().muted).text_color(cx.theme().foreground))
                    })
                    .child(label)
                    .on_click(move |_, window, cx| {
                        if !kind.is_empty() {
                            callback(kind.clone(), window, cx);
                        }
                    }),
            );
        }
        tray.into_any_element()
    };
    let label = div()
        .flex_shrink_0()
        .whitespace_nowrap()
        .text_size(px(theme::FONT_CAPTION))
        .line_height(px(theme::LINE_CAPTION))
        .font_weight(FontWeight::BOLD)
        .text_color(theme::quiet(cx))
        .child(tabs["label"].as_str().unwrap_or_default().to_owned());
    // One line: the label then the bar, a toolbar's height. Many options wrap in a tray under the label.
    if ranges.len() <= 1 {
        div()
            .flex()
            .items_center()
            .gap(px(theme::SPACE_SM))
            .min_w_0()
            .flex_shrink_0()
            .child(label)
            .child(control)
    } else {
        div()
            .flex()
            .flex_col()
            .gap(px(theme::SPACE_XS))
            .min_w_0()
            .w_full()
            .child(label)
            .child(control)
    }
}

fn wrap_ranges(widths: &[f32], room: f32) -> Vec<std::ops::Range<usize>> {
    let mut chunks = Vec::new();
    let mut start = 0;
    let mut used = 0.;
    for (index, width) in widths.iter().enumerate() {
        if index > start && used + width > room {
            chunks.push(start..index);
            start = index;
            used = 0.;
        }
        used += width;
    }
    if start < widths.len() {
        chunks.push(start..widths.len());
    }
    chunks
}

#[cfg(test)]
mod tests {
    #[test]
    fn long_translated_tabs_wrap_without_hiding_the_last_view() {
        assert_eq!(super::wrap_ranges(&[95., 160., 120.], 280.), vec![0..2, 2..3]);
        assert_eq!(super::wrap_ranges(&[500., 100.], 200.), vec![0..1, 1..2]);
        assert!(super::wrap_ranges(&[], 200.).is_empty());
    }

    #[test]
    fn six_sounds_tabs_wrap_at_narrow_540px_width() {
        let tabs = serde_json::json!({
            "label": "Show:",
            "selected": "sounds_playing",
            "options": [
                {"id": "sounds_playing", "label": "Playing"},
                {"id": "sounds_library", "label": "Library"},
                {"id": "sounds_board", "label": "Board"},
                {"id": "sounds_voices", "label": "Voices"},
                {"id": "sounds_voice_catalog", "label": "Find voices"},
                {"id": "sounds_output", "label": "Output"},
            ]
        });
        let label_w = "Show:".chars().count() as f32 * 7.4 + crate::theme::SPACE_SM;
        let room = (540. - 96. - label_w).max(150.);
        let widths: Vec<_> = tabs["options"]
            .as_array()
            .unwrap()
            .iter()
            .map(super::option_width)
            .collect();
        assert!(super::wrap_ranges(&widths, room).len() > 1);
    }
}
