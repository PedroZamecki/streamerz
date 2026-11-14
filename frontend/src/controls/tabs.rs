//! A page's "Show" selector: the core lists the tables a page can show (`tabs`), one is drawn at a time.
use gpui_kit::component::{
    tab::{Tab, TabBar},
    ActiveTheme, Sizable,
};
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

/// The label and a segmented bar; `select` gets the kind that was picked.
pub fn bar(
    tabs: &Value,
    window: &Window,
    cx: &App,
    select: impl Fn(String, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let (kinds, selected) = kinds(tabs);
    let options = tabs["options"].as_array().map_or(&[][..], Vec::as_slice);
    let select = std::rc::Rc::new(select);
    // Reserve the sidebar and page insets. Each chunk is still the same segmented tab control.
    let viewport = f32::from(window.viewport_size().width);
    let room = (viewport - if viewport < 700. { 110. } else { 260. }).max(150.);
    let widths: Vec<_> = options
        .iter()
        .map(|option| 30. + option["label"].as_str().unwrap_or_default().chars().count() as f32 * 7.)
        .collect();
    let mut bars = div().flex().flex_wrap().items_center().gap_2().min_w_0();
    for (chunk, range) in wrap_ranges(&widths, room).into_iter().enumerate() {
        let offset = range.start;
        let callback = select.clone();
        let kinds = kinds.clone();
        bars = bars.child(
            TabBar::new(SharedString::from(format!("page-tabs-{chunk}")))
                .segmented()
                .small()
                .selected_index(if range.contains(&selected) {
                    selected - offset
                } else {
                    usize::MAX
                })
                .children(options[range].iter().map(|option| {
                    let label = option["label"].as_str().unwrap_or_default().to_owned();
                    Tab::new().aria_label(label.clone()).label(label)
                }))
                .on_click(move |index, window, cx| {
                    if let Some(kind) = kinds.get(offset + *index) {
                        callback(kind.clone(), window, cx);
                    }
                }),
        );
    }
    div()
        .flex()
        .flex_col()
        .gap_1()
        .min_w_0()
        .child(
            div()
                .text_color(cx.theme().muted_foreground)
                .child(tabs["label"].as_str().unwrap_or_default().to_owned()),
        )
        .child(bars)
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
}
