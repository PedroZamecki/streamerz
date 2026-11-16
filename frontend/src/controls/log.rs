//! The log page: recent lines, filtered by the core for a level and some words.
use crate::theme::{CONTROL_HEIGHT, FONT_BODY, LINE_BODY, SPACE_MD, SPACE_SM, SPACE_XS};
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
        let mut bar = div().flex().items_center().gap(px(SPACE_SM)).flex_shrink_0();
        if !narrow {
            bar = bar.child(
                div()
                    .min_w(px(44.))
                    .flex_shrink_0()
                    .whitespace_nowrap()
                    .child(log["show"].as_str().unwrap_or_default().to_owned()),
            );
        }
        if let Some(level) = &self.level {
            bar = bar.child(
                div().w(px(if narrow { 120. } else { 160. })).child(
                    Select::new(level)
                        .small()
                        .icon(Icon::empty().path("icons/chevrons-up-down.svg").size(px(16.)))
                        .h(px(CONTROL_HEIGHT)),
                ),
            );
        }
        bar = bar.child(div().flex_1().min_w_0().child(Input::new(&self.search).small().h(px(CONTROL_HEIGHT)).aria_label(log["searchHint"].as_str().unwrap_or_default().to_owned()))).child(
            Button::new("open-logs")
                .small()
                .h(px(CONTROL_HEIGHT))
                .when(!narrow, |button| button.min_w(px(140.)))
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
            .px(px(SPACE_MD))
            .py(px(SPACE_XS))
            .rounded(cx.theme().radius)
            .border_1()
            .border_color(cx.theme().input)
            .bg(cx.theme().table)
            .text_size(px(FONT_BODY))
            .line_height(px(LINE_BODY));
        for line in log["lines"].as_array().into_iter().flatten() {
            let color: Hsla = match line["level"].as_str().unwrap_or_default() {
                "debug" => crate::theme::quiet(cx),
                "warn" => cx.theme().warning,
                "error" | "fatal" => cx.theme().danger,
                _ => cx.theme().info,
            };
            lines = lines.child(
                div()
                    .flex()
                    .gap(px(SPACE_SM))
                    .child(
                        div()
                            .flex_shrink_0()
                            .min_w(px(72.))
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
        div()
            .size_full()
            .flex()
            .flex_col()
            .gap(px(SPACE_SM))
            .child(bar)
            .child(lines)
    }
}

#[cfg(all(test, feature = "test-support"))]
mod tests {
    use super::{Filter, LogView};
    use gpui_kit::test::{TestEventExt, TestWindowExt};
    use gpui_kit::{
        div, px, size, AppContext, Bounds, Entity, IntoElement, ParentElement, Render, Styled, TestAppContext, Window,
        WindowBounds, WindowOptions,
    };
    use serde_json::json;

    struct LogHost(Entity<LogView>);
    impl Render for LogHost {
        fn render(&mut self, _: &mut Window, _: &mut gpui_kit::Context<Self>) -> impl IntoElement {
            div().size_full().p_4().child(self.0.clone())
        }
    }

    #[gpui_kit::test]
    fn search_and_level_changes_emit_filter_events(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::theme::apply(true, cx);
        });
        let model = json!({
            "log": {
                "show": "Show",
                "levels": ["Debug", "Info", "Warn", "Error"],
                "level": 1,
                "searchHint": "Filter logs",
                "folder": "Open folder",
                "lines": []
            }
        });
        let slot: std::rc::Rc<std::cell::RefCell<Option<Entity<LogView>>>> =
            std::rc::Rc::new(std::cell::RefCell::new(None));
        let emitted: std::rc::Rc<std::cell::RefCell<Vec<(usize, String)>>> =
            std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let s = slot.clone();
        let e = emitted.clone();
        let window = cx.update(|cx| {
            let bounds = Bounds {
                origin: gpui_kit::Point::default(),
                size: size(px(800.), px(500.)),
            };
            let (window, _) = gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                cx,
                move |window, cx| {
                    let view = cx.new(|cx| LogView::new(model, window, cx));
                    *s.borrow_mut() = Some(view.clone());
                    let e = e.clone();
                    let sub = cx.subscribe(&view, move |_, filter: &Filter, _| {
                        e.borrow_mut().push((filter.level, filter.search.clone()));
                    });
                    std::mem::forget(sub);
                    cx.new(|_| LogHost(view))
                },
            )
            .expect("open test window");
            window.downcast::<gpui_kit::base::Root>().expect("root window")
        });
        let view = slot.borrow().clone().expect("log view created");

        cx.update_window(window.into(), |_, window, cx| {
            window.render_frame(cx);
            let search = view.read(cx).search.clone();
            search.update(cx, |input, cx| input.focus(window, cx));
            window.input("twitch", cx);
        })
        .unwrap();
        cx.run_until_parked();
        assert_eq!(emitted.borrow().last(), Some(&(1, "twitch".to_owned())));

        cx.update_window(window.into(), |_, window, cx| {
            window.render_frame(cx);
            let level = view.read(cx).level.clone().expect("level select");
            let id = level.entity_id();
            window.within(("select", id)).click("input", cx);
            window.render_frame(cx);
            window.key_down("down", false, cx);
            window.key_up("down", cx);
            window.key_down("enter", false, cx);
            window.key_up("enter", cx);
        })
        .unwrap();
        cx.run_until_parked();
        assert_eq!(emitted.borrow().last(), Some(&(2, "twitch".to_owned())));
    }
}
