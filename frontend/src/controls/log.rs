//! The log page: recent lines, filtered by the core for a level and some words.
use crate::{i18n::tr, send};
use gpui_kit::component::{
    button::Button,
    input::{Input, InputEvent, InputState},
    select::{Select, SelectEvent, SelectState},
    ActiveTheme, Icon, IndexPath, Sizable,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use serde_json::{json, Value};

/// The filters changed: the page asks the core again.
pub struct Filter {
    pub level: usize,
    pub search: String,
}
impl EventEmitter<Filter> for LogView {}

pub struct LogView {
    model: Value,
    level: Option<Entity<SelectState<Vec<String>>>>,
    search: Entity<InputState>,
    scroll: ScrollHandle,
    stick: bool,
    _subscriptions: Vec<Subscription>,
}
impl LogView {
    pub fn new(model: Value, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let hint = model["log"]["searchHint"].as_str().unwrap_or_default().to_owned();
        let search = cx.new(|cx| InputState::new(window, cx).placeholder(hint));
        let subscriptions = vec![cx.subscribe(&search, |view, input, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                view.changed(input.read(cx).value().to_string(), cx);
            }
        })];
        let mut view = Self {
            model: Value::Null,
            level: None,
            search,
            scroll: ScrollHandle::new(),
            stick: true,
            _subscriptions: subscriptions,
        };
        view.update_model(model, window, cx);
        view
    }
    fn changed(&mut self, search: String, cx: &mut Context<Self>) {
        let level = self
            .level
            .as_ref()
            .and_then(|level| level.read(cx).selected_index(cx))
            .map_or(1, |index| index.row);
        cx.emit(Filter { level, search });
    }
    pub fn update_model(&mut self, model: Value, window: &mut Window, cx: &mut Context<Self>) {
        if self.level.is_none() {
            let options: Vec<String> = model["log"]["levels"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect();
            let level = cx.new(|cx| {
                SelectState::new(
                    options,
                    Some(IndexPath::new(model["log"]["level"].as_u64().unwrap_or(1) as usize)),
                    window,
                    cx,
                )
            });
            self._subscriptions
                .push(cx.subscribe(&level, |view, _, event: &SelectEvent<Vec<String>>, cx| {
                    let SelectEvent::Confirm(_) = event;
                    let search = view.search.read(cx).value().to_string();
                    view.changed(search, cx);
                }));
            self.level = Some(level);
        }
        self.model = model;
        self.stick = true;
        cx.notify();
    }
}
impl Render for LogView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let narrow = f32::from(window.viewport_size().width) < 600.;
        let log = &self.model["log"];
        let mut bar = div().flex().items_center().gap(px(6.)).flex_shrink_0().mt(px(2.));
        if !narrow {
            bar = bar.child(
                div()
                    .w(px(44.))
                    .child(log["show"].as_str().unwrap_or_default().to_owned()),
            );
        }
        if let Some(level) = &self.level {
            bar = bar.child(
                div().w(px(if narrow { 120. } else { 160. })).child(
                    Select::new(level)
                        .small()
                        .icon(Icon::empty().path("icons/chevrons-up-down.svg").size(px(16.)))
                        .h(px(26.)),
                ),
            );
        }
        bar = bar.child(div().flex_1().min_w_0().child(Input::new(&self.search).small().h(px(26.)).aria_label(log["searchHint"].as_str().unwrap_or_default().to_owned()))).child(
            Button::new("open-logs")
                .small()
                .h(px(26.))
                .when(!narrow, |button| button.w(px(140.)))
                .child(super::body::button_body(
                    Some("folder-open"),
                    (!narrow).then(|| log["folder"].as_str().unwrap_or_default().to_owned()),
                    cx,
                ))
                .accessibility_label(log["folder"].as_str().unwrap_or_default().to_owned())
                .on_click(|_, _, _| send(json!({"request": "StreamerzCoreAction", "domain": "log", "action": "open_folder", "id": "log:open"}))),
        );
        let mut lines = div()
            .id("log-lines")
            .track_scroll(&self.scroll)
            .overflow_y_scroll()
            .flex_1()
            .min_h_0()
            .p(px(5.))
            .rounded(px(6.))
            .border_1()
            .border_color(rgb(if cx.theme().is_dark() { 0x040405 } else { 0xc0c0c1 }))
            .bg(cx.theme().table)
            .text_size(px(13.))
            .line_height(px(19.));
        for line in log["lines"].as_array().into_iter().flatten() {
            let dark = cx.theme().is_dark();
            let color: Hsla = match line["level"].as_str().unwrap_or_default() {
                "debug" => crate::theme::quiet(cx),
                "warn" => rgb(if dark { 0xfbbf24 } else { 0xb45309 }).into(),
                "error" | "fatal" => rgb(if dark { 0xf87171 } else { 0xdc2626 }).into(),
                _ => rgb(if dark { 0x60a5fa } else { 0x2563eb }).into(),
            };
            lines = lines.child(
                div()
                    .flex()
                    .child(
                        div()
                            .flex_shrink_0()
                            .min_w(px(49.))
                            .whitespace_nowrap()
                            .text_color(crate::theme::quiet(cx))
                            .child(line["time"].as_str().unwrap_or_default().to_owned()),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .w(px(47.))
                            .font_weight(FontWeight::BOLD)
                            .text_color(color)
                            .child(line["tag"].as_str().unwrap_or_default().to_owned()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(line["text"].as_str().unwrap_or_default().to_owned()),
                    ),
            );
        }
        if std::mem::take(&mut self.stick) {
            self.scroll.scroll_to_bottom();
        }
        let _ = tr("log.show");
        div().size_full().flex().flex_col().gap_2().child(bar).child(lines)
    }
}
