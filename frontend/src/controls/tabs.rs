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
pub fn bar(tabs: &Value, cx: &App, select: impl Fn(String, &mut Window, &mut App) + 'static) -> impl IntoElement {
    let (kinds, selected) = kinds(tabs);
    div()
        .flex()
        .items_center()
        .gap_2()
        .child(
            div()
                .text_color(cx.theme().muted_foreground)
                .child(tabs["label"].as_str().unwrap_or_default().to_owned()),
        )
        .child(
            TabBar::new("page-tabs")
                .segmented()
                .small()
                .selected_index(selected)
                .children(tabs["options"].as_array().into_iter().flatten().map(|option| {
                    let label = option["label"].as_str().unwrap_or_default().to_owned();
                    Tab::new().aria_label(label.clone()).label(label)
                }))
                .on_click(move |index, window, cx| {
                    if let Some(kind) = kinds.get(*index) {
                        select(kind.clone(), window, cx);
                    }
                }),
        )
}
