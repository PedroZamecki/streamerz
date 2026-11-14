//! Page composition. Reusable controls render models supplied by the core.
use crate::{
    controls::{
        actions::{ActionsPage, Opened},
        chat::{ChatView, Go},
        list::{List, ViewChange},
        log::{Filter, LogView},
        prompt::Prompt,
        sections::{Navigate, Sections},
    },
    i18n::tr,
    send, CoreRedo, CoreUndo,
};
use gpui_kit::base::Selectable;
use gpui_kit::component::{
    button::{Button, ButtonVariants},
    scroll::ScrollableElement,
    ActiveTheme, Disableable, Icon, Sizable, WindowExt,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use serde_json::{json, Value};

pub struct Frontend {
    pub navigation: Value,
    /// "Streamerz vX": the app name and its version, for the sidebar footer.
    version: String,
    status: Value,
    folded: bool,
    status_error: String,
    pub selected: String,
    history: Vec<String>,
    history_index: usize,
    pub data: Value,
    lists: Vec<Entity<List>>,
    list_subscriptions: Vec<Subscription>,
    /// The page's view state sent with every read: the shown tab and each table's sort and filter.
    view: Value,
    log: Option<Entity<LogView>>,
    chat: Option<Entity<ChatView>>,
    chat_subscription: Option<Subscription>,
    actions: Option<Entity<ActionsPage>>,
    actions_subscription: Option<Subscription>,
    log_filter: (usize, String),
    log_subscription: Option<Subscription>,
    sections: Option<Entity<Sections>>,
    sections_subscription: Option<Subscription>,
    error: String,
    prompt: Option<Entity<Prompt>>,
    prompt_id: Option<u64>,
    generation: u64,
    read_pending: bool,
    read_again: bool,
    last_bounds: Option<(i32, i32, i32, i32, bool)>,
    window_save_task: Option<Task<()>>,
}
impl Frontend {
    #[cfg(feature = "test-support")]
    pub fn page_ready(&self) -> bool {
        !self.read_pending
            && self.data["domain"].as_str() == Some(self.selected.to_lowercase().replace(' ', "_").as_str())
    }
    pub fn new(navigation: Value, version: String, tab: String) -> Self {
        let selected = if tab.is_empty() { "Chat".to_string() } else { tab };
        Self {
            navigation,
            version,
            status: Value::Null,
            folded: false,
            status_error: String::new(),
            selected: selected.clone(),
            history: vec![selected],
            history_index: 0,
            data: Value::Null,
            lists: Vec::new(),
            list_subscriptions: Vec::new(),
            view: json!({}),
            log: None,
            log_filter: (1, String::new()),
            log_subscription: None,
            chat: None,
            chat_subscription: None,
            actions: None,
            actions_subscription: None,
            sections: None,
            sections_subscription: None,
            error: String::new(),
            prompt: None,
            prompt_id: None,
            generation: 0,
            read_pending: false,
            read_again: false,
            last_bounds: None,
            window_save_task: None,
        }
    }
    pub fn request_read(&mut self) {
        if self.read_pending {
            self.read_again = true;
            return;
        }
        self.read_pending = true;
        let modelled = self
            .navigation
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|section| section["pages"].as_array().into_iter().flatten())
            .any(|page| page["id"] == self.selected.as_str() && page["view"] == true);
        let request = if modelled {
            "StreamerzCoreView"
        } else {
            "StreamerzCoreRead"
        };
        send(
            json!({"request": request, "domain": self.selected.to_lowercase().replace(' ', "_"), "id": format!("page:{}", self.generation),
                "level": self.log_filter.0, "search": self.log_filter.1.clone(), "view": self.view.clone()}),
        );
    }
    /// Remembers a table's sort or hidden filter options, or the shown tab, for the next read.
    fn change_view(&mut self, change: &ViewChange) {
        match change {
            ViewChange::Sort { kind, column, desc } => {
                self.view[kind]["sort"] = json!({"column": column, "desc": desc});
            }
            ViewChange::Hidden { kind, hidden } => self.view[kind]["hidden"] = json!(hidden),
            ViewChange::Tab(kind) => self.view["tab"] = kind.as_str().into(),
        }
    }
    /// The page's tabs go to the list that shows them as a "Show" combo before its search.
    fn show_tabs(&self, cx: &mut Context<Self>) {
        let page = self.selected.to_lowercase().replace(' ', "_");
        let mut tabs = self.data["tabs"].clone();
        if let Some(kind) = self.view["tab"].as_str() {
            tabs["selected"] = kind.into();
        }
        let shown = tabs.is_object() && self.data["domain"].as_str() == Some(page.as_str());
        for list in &self.lists {
            list.update(cx, |list, cx| list.set_show(shown.then(|| tabs.clone()), cx));
        }
    }
    /// The tables of the page now: with tabs, only the one selected.
    fn visible_lists(&self, cx: &App) -> Vec<Entity<List>> {
        let page = self.selected.to_lowercase().replace(' ', "_");
        let tabs = &self.data["tabs"];
        let shown = (self.data["domain"].as_str() == Some(page.as_str()) && tabs.is_object())
            .then(|| super::controls::tabs::kinds(tabs).0)
            .and_then(|kinds| {
                let wanted = self.view["tab"]
                    .as_str()
                    .or(tabs["selected"].as_str())
                    .map(str::to_owned);
                wanted.filter(|kind| kinds.contains(kind))
            });
        self.lists
            .iter()
            .filter(|list| shown.as_deref().is_none_or(|kind| list.read(cx).kind() == kind))
            .cloned()
            .collect()
    }
    /// Pictures the core downloaded reach an open form's picture pickers (no page read).
    pub fn images_for_forms(&mut self, topics: Option<&Vec<Value>>, cx: &mut Context<Self>) {
        if topics.is_some_and(|topics| topics.iter().any(|topic| topic == "images")) {
            if let Some(actions) = self.actions.clone() {
                actions.update(cx, |actions, cx| actions.images_arrived(cx));
            }
        }
    }
    /// The chat streams: its view asks the core for what is new, with no full page read. Returns
    /// whether the change was handled (settings changes still re-read the whole page).
    pub fn chat_changed(&mut self, topics: Option<&Vec<Value>>, cx: &mut Context<Self>) -> bool {
        let (Some(chat), Some(topics)) = (self.chat.clone(), topics) else {
            return false;
        };
        if self.selected != "Chat" || topics.iter().any(|topic| topic == "config") {
            return false;
        }
        for topic in topics {
            match topic.as_str() {
                Some("chat") => chat.update(cx, |chat, _| chat.refresh()),
                Some("images") => chat.update(cx, |chat, cx| chat.images_arrived(cx)),
                _ => {}
            }
        }
        true
    }
    pub fn request_status(&self) {
        send(json!({"request": "StreamerzCoreStatus", "id": "status"}));
    }
    fn show_connections(&mut self, connections: Value, window: &mut Window, cx: &mut Context<Self>) {
        let owner = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _, cx| {
            let mut list = div().flex().flex_col().gap_1().min_w(px(360.));
            let items = connections.as_array().cloned().unwrap_or_default();
            if items.is_empty() {
                list = list.child(
                    div()
                        .text_color(cx.theme().muted_foreground)
                        .child(tr("conns.empty_hint")),
                );
            }
            for (index, item) in items.iter().enumerate() {
                let page = item["page"].as_str().unwrap_or_default().to_owned();
                let owner = owner.clone();
                let key = item["key"].as_str().unwrap_or_default().to_owned();
                // Its state as an icon in its colour, a short word, and the rest on hover.
                let (icon, color) = crate::controls::health::appearance(item["health"].as_str().unwrap_or("off"), cx);
                let tip = item["tip"].as_str().unwrap_or_default().to_owned();
                let action = item["button"].as_str().unwrap_or_default().to_owned();
                let settings = tr("btn.settings");
                list = list.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .h(px(30.))
                        .child(
                            crate::controls::body::brand_logo(
                                ("conn-logo", index),
                                item["brand"].as_str().unwrap_or_default(),
                                px(16.),
                                cx,
                            ),
                        )
                        .child({
                            let name = item["name"].as_str().unwrap_or_default().to_owned();
                            let name_tip = name.clone();
                            div()
                                .id(("conn-name", index))
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .child(name)
                                .when(!name_tip.is_empty(), |el| {
                                    el.tooltip(move |window, cx| {
                                        gpui_kit::component::tooltip::Tooltip::new(name_tip.clone()).build(window, cx)
                                    })
                                })
                        })
                        .child(
                            div()
                                .id(("conn-state", index))
                                .flex()
                                .items_center()
                                .gap(px(6.))
                                .flex_shrink_0()
                                .text_color(color)
                                .child(Icon::empty().path(format!("icons/{icon}.svg")).size(px(14.)))
                                .child(item["state"].as_str().unwrap_or_default().to_owned())
                                .when(!tip.is_empty(), |state| {
                                    state.tooltip(move |window, cx| {
                                        gpui_kit::component::tooltip::Tooltip::new(tip.clone()).build(window, cx)
                                    })
                                }),
                        )
                        // A fixed slot, so the buttons line up whether or not a row has this one.
                        .child(div().w(px(30.)).flex_shrink_0().when(!action.is_empty(), |slot| {
                            let icon = if item["standby"] == true { "refresh-cw" } else { "plug" };
                            slot.child(
                                Button::new(("conn-retry", index))
                                    .small()
                                    .w(px(30.))
                                    .icon(Icon::empty().path(format!("icons/{icon}.svg")))
                                    .tooltip(action.clone())
                                    .accessibility_label(action.clone())
                                    .on_click(move |_, _, _| {
                                        send(json!({"request": "StreamerzCoreAction", "domain": "connections", "action": "retry", "itemId": key, "id": "retry"}));
                                    }),
                            )
                        }))
                        .child(
                            Button::new(("conn-settings", index))
                                .small()
                                .w(px(30.))
                                .icon(Icon::empty().path("icons/settings.svg"))
                                .tooltip(settings)
                                .accessibility_label(settings)
                                .on_click(move |_, window, cx| {
                                    let page = page.clone();
                                    let _ = owner.update(cx, |this, cx| this.select(&page, window, cx));
                                    window.close_dialog(cx);
                                }),
                        ),
                );
            }
            let close = Button::new("conns-close")
                .label(tr("dialogs.close"))
                .on_click(|_, window, cx| crate::controls::dialog::dismiss(window, cx));
            let footer = crate::controls::dialog::footer(None, close, cx);
            crate::controls::dialog::shell(dialog, tr("conns.title"), 460., cx)
                .child(crate::controls::dialog::body(list))
                .footer(footer)
        });
    }
    pub fn select(&mut self, page: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.navigate_to(page, true, window, cx);
    }
    fn navigate_to(&mut self, page: &str, record_history: bool, window: &mut Window, cx: &mut Context<Self>) {
        if page != self.selected {
            self.selected = page.to_owned();
            send(json!({
                "request": "StreamerzSelectPage",
                "page": self.selected,
            }));
            if record_history {
                self.history.truncate(self.history_index + 1);
                self.history.push(page.to_owned());
                self.history_index = self.history.len() - 1;
            }
            self.lists.clear();
            self.list_subscriptions.clear();
            self.view = json!({});
            self.sections = None;
            self.sections_subscription = None;
            self.log = None;
            self.log_subscription = None;
            self.chat = None;
            self.chat_subscription = None;
            self.actions = None;
            self.actions_subscription = None;
            self.generation += 1;
            self.read_pending = false;
            self.read_again = false;
            self.error.clear();
            window.close_dialog(cx);
        }
        self.request_read();
        cx.notify();
    }
    pub fn window_bounds_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (bounds, maximized) = match window.window_bounds() {
            WindowBounds::Maximized(b) => (b, true),
            WindowBounds::Windowed(b) => (b, false),
            WindowBounds::Fullscreen(b) => (b, false),
        };
        let (x, y, w, h) = (
            f32::from(bounds.origin.x) as i32,
            f32::from(bounds.origin.y) as i32,
            f32::from(bounds.size.width) as i32,
            f32::from(bounds.size.height) as i32,
        );
        if w < 400 || h < 300 {
            return;
        }
        let current = (x, y, w, h, maximized);
        if self.last_bounds == Some(current) {
            return;
        }
        self.last_bounds = Some(current);
        self.window_save_task = Some(cx.spawn(async move |_, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(250))
                .await;
            send(json!({
                "request": "StreamerzWindowState",
                "x": x,
                "y": y,
                "w": w,
                "h": h,
                "maximized": maximized,
            }));
        }));
    }
    pub fn go_back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.history_index > 0 {
            self.history_index -= 1;
            let target = self.history[self.history_index].clone();
            self.navigate_to(&target, false, window, cx);
        }
    }
    pub fn go_forward(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.history_index + 1 < self.history.len() {
            self.history_index += 1;
            let target = self.history[self.history_index].clone();
            self.navigate_to(&target, false, window, cx);
        }
    }
    pub fn apply(&mut self, value: Value, window: &mut Window, cx: &mut Context<Self>) {
        let id = value["id"].as_str().unwrap_or_default().to_owned();
        if id == "status" {
            self.status = value;
            cx.notify();
            return;
        }
        if id == "save" {
            if value["status"] == "error" {
                self.status_error = value["error"].as_str().unwrap_or_default().to_owned();
            } else {
                self.status_error.clear();
                window.push_notification(tr("ui.saved"), cx);
            }
            cx.notify();
            return;
        }
        if id == "prompts" {
            let prompts = value["prompts"].as_array();
            if let Some(id) = self.prompt_id {
                if prompts.is_some_and(|prompts| prompts.iter().any(|prompt| prompt["id"] == id)) {
                    return;
                }
                self.prompt = None;
                self.prompt_id = None;
                window.close_dialog(cx);
            }
            if let Some(model) = prompts.and_then(|prompts| prompts.first()) {
                let title = model["title"].as_str().unwrap_or_default().to_owned();
                self.prompt_id = model["id"].as_u64();
                let prompt = cx.new(|cx| Prompt::new(model.clone(), window, cx));
                self.prompt = Some(prompt.clone());
                window.open_dialog(cx, move |dialog, _, cx| {
                    crate::controls::dialog::shell(dialog, title.clone(), 448., cx)
                        .child(crate::controls::dialog::body(prompt.clone()))
                        .footer(prompt.read(cx).buttons_view())
                        .on_cancel({
                            let prompt = prompt.clone();
                            move |_, _, cx| {
                                prompt.update(cx, |prompt, cx| prompt.cancel(cx));
                                false
                            }
                        })
                });
            }
            cx.notify();
            return;
        }
        if id.strip_prefix("prompt:").and_then(|id| id.parse().ok()) == self.prompt_id && self.prompt_id.is_some() {
            if value["status"] == "error" {
                if let Some(prompt) = &self.prompt {
                    prompt.update(cx, |prompt, cx| {
                        prompt.pending = false;
                        prompt.error = value["error"].as_str().unwrap_or_default().to_owned();
                        cx.notify();
                    });
                }
            } else {
                self.prompt = None;
                self.prompt_id = None;
                window.close_dialog(cx);
                send(json!({"request":"StreamerzPrompts", "id":"prompts"}));
            }
            return;
        }
        if let Some(operation) = id.strip_prefix(&format!("actions:{}:", self.generation)) {
            if let Some(actions) = self.actions.clone() {
                if actions.update(cx, |actions, cx| actions.apply(operation, value, window, cx)) {
                    self.request_read();
                }
            }
            return;
        }
        if let Some(operation) = id.strip_prefix(&format!("sections:{}:", self.generation)) {
            if let Some(sections) = self.sections.clone() {
                if sections.update(cx, |sections, cx| sections.apply(operation, value, window, cx)) {
                    self.request_read();
                }
            }
            return;
        }
        if let Some(route) = id.strip_prefix(&format!("list:{}:", self.generation)) {
            if let Some((kind, operation)) = route.split_once(':') {
                if let Some(list) = self.lists.iter().find(|list| list.read(cx).kind() == kind) {
                    if list.update(cx, |list, cx| list.apply(operation, value, window, cx)) {
                        self.request_read();
                    }
                }
            }
            return;
        }
        if let Some(operation) = id.strip_prefix("chat:").filter(|_| self.chat.is_some()) {
            if let Some(chat) = self.chat.clone() {
                chat.update(cx, |chat, cx| chat.apply(operation, value, window, cx));
            }
            return;
        }
        if id != format!("page:{}", self.generation) {
            return;
        }
        self.read_pending = false;
        if value["status"] == "error" {
            self.error = value["error"].as_str().unwrap_or_default().to_owned();
        } else if value["domain"].as_str() == Some(self.selected.to_lowercase().replace(' ', "_").as_str()) {
            if value["log"].is_object() {
                match &self.log {
                    Some(log) => log.update(cx, |log, cx| log.update_model(value.clone(), window, cx)),
                    None => {
                        let log = cx.new(|cx| LogView::new(value.clone(), window, cx));
                        self.log_subscription = Some(cx.subscribe(&log, |this, _, filter: &Filter, cx| {
                            this.log_filter = (filter.level, filter.search.clone());
                            this.request_read();
                            cx.notify();
                        }));
                        self.log = Some(log);
                    }
                }
            }
            if value["actions"].is_object() {
                match &self.actions {
                    Some(actions) => actions.update(cx, |actions, cx| actions.update_model(value.clone(), window, cx)),
                    None => {
                        let route = format!("actions:{}", self.generation);
                        let actions = cx.new(|cx| ActionsPage::new(value.clone(), route, window, cx));
                        self.actions_subscription =
                            Some(cx.subscribe_in(&actions, window, |this, _, opened: &Opened, _, _| {
                                this.view["actionId"] = opened.0.clone().into();
                                this.request_read();
                            }));
                        self.actions = Some(actions);
                    }
                }
            }
            if value["chat"].is_object() {
                match &self.chat {
                    Some(chat) => chat.update(cx, |chat, cx| chat.update_model(value.clone(), window, cx)),
                    None => {
                        let chat = cx.new(|cx| ChatView::new(value.clone(), window, cx));
                        self.chat_subscription =
                            Some(cx.subscribe_in(&chat, window, |this, _, go: &Go, window, cx| {
                                this.select(&go.0, window, cx);
                            }));
                        self.chat = Some(chat);
                    }
                }
            }
            if value["sections"].is_array() {
                if let Some(appearance) = value["appearance"].as_object() {
                    crate::theme::apply(appearance["dark"].as_bool().unwrap_or(true), cx);
                    cx.set_reduce_motion(appearance["animations"] == false);
                }
                match &self.sections {
                    Some(sections) => {
                        sections.update(cx, |sections, cx| sections.update_model(value.clone(), window, cx))
                    }
                    None => {
                        let route = format!("sections:{}", self.generation);
                        let sections = cx.new(|cx| Sections::new(value.clone(), route, window, cx));
                        self.sections_subscription =
                            Some(
                                cx.subscribe_in(&sections, window, |this, _, event: &Navigate, window, cx| {
                                    this.select(&event.0, window, cx);
                                }),
                            );
                        self.sections = Some(sections);
                    }
                }
            }
            for model in value["tables"].as_array().into_iter().flatten() {
                let kind = model["kind"].as_str().unwrap_or_default();
                if let Some(list) = self.lists.iter().find(|list| list.read(cx).kind() == kind) {
                    list.update(cx, |list, cx| list.update_model(model.clone(), window, cx));
                } else {
                    let route = format!("list:{}:{kind}", self.generation);
                    let list = cx.new(|cx| List::new(model.clone(), route, window, cx));
                    self.list_subscriptions
                        .push(cx.subscribe(&list, |this, _, change: &ViewChange, cx| {
                            this.change_view(change);
                            this.request_read();
                            cx.notify();
                        }));
                    self.lists.push(list);
                }
            }
            self.data = value;
            self.show_tabs(cx);
        }
        if std::mem::take(&mut self.read_again) {
            self.request_read();
        }
        cx.notify();
    }
}
/// A development build (as `src/brand.rs` decides it: debug, or built without a release version).
const IS_DEV: bool = cfg!(debug_assertions) || option_env!("STREAMERZ_VERSION").is_none();
/// The inside of a bottom bar button: a 14px icon and its label (the label hides when the window is small).
fn bar_content(icon: &'static str, label: &'static str, small: bool, cx: &App) -> impl IntoElement {
    crate::controls::body::button_body(Some(icon), (!small).then(|| label.to_owned()), cx)
}
impl Render for Frontend {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The toolkit sets the rem to the 13px text size; sizes are designed for 16px.
        window.set_rem_size(px(16.));
        let small = f32::from(window.viewport_size().width) < 700.;
        let narrow = small || self.folded;
        let mut navigation = div()
            .id("navigation")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap_0()
            .overflow_y_scrollbar();
        // Fixed above the list: it stays in view while the pages scroll.
        let fold_button = div().flex().h(px(24.)).ml(px(5.)).child(
            Button::new("fold")
                .mt(px(-2.))
                .text_color(rgb(if cx.theme().is_dark() { 0xb4b4b5 } else { 0x4c4c4d }))
                .small()
                .ghost()
                .w(px(26.))
                .px(px(0.))
                .child(
                    Icon::empty()
                        .path(if narrow {
                            "icons/panel-left-open.svg"
                        } else {
                            "icons/panel-left-close.svg"
                        })
                        .size(px(17.)),
                )
                .accessibility_label(if narrow { tr("nav.unfold") } else { tr("nav.fold") })
                .tooltip(if narrow { tr("nav.unfold") } else { tr("nav.fold") })
                .disabled(small)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.folded = !this.folded;
                    cx.notify();
                })),
        );
        let mut title = self.selected.clone();
        let selected_bg: Hsla = rgb(if cx.theme().is_dark() { 0x2b2b2e } else { 0xdcdcdd }).into();
        for section in self.navigation.as_array().into_iter().flatten() {
            navigation = navigation.child(if narrow {
                // Folded: a line above each group in place of its label (the first one too).
                div().h(px(18.)).flex().justify_center().child(
                    div().w(px(24.)).h(px(1.)).mt(px(10.)).bg(rgb(if cx.theme().is_dark() {
                        0x2b2b2e
                    } else {
                        0xdcdcdd
                    })),
                )
            } else {
                div()
                    .h(px(18.))
                    .pt(px(1.))
                    .pl(px(8.))
                    .text_size(px(11.))
                    .text_color(crate::theme::quiet(cx))
                    .child(section["label"].as_str().unwrap_or_default().to_uppercase())
            });
            for page in section["pages"].as_array().into_iter().flatten() {
                let id = page["id"].as_str().unwrap_or_default().to_owned();
                let selected = self.selected == id;
                let label = page["label"].as_str().unwrap_or_default().to_owned();
                if selected {
                    title = label.clone();
                }
                navigation = navigation.child(
                    Button::new(SharedString::from(id.clone()))
                        .small()
                        .h(px(23.))
                        .mb(px(2.))
                        .px(px(10.))
                        .w_full()
                        .ghost()
                        .when(selected, |button| button.bg(selected_bg).font_weight(FontWeight::BOLD))
                        .justify_start()
                        .accessibility_label(label.clone())
                        .tooltip(label.clone())
                        .child(
                            div()
                                .w_full()
                                .flex()
                                .items_center()
                                .gap(px(10.))
                                .text_size(px(13.))
                                .text_color(if selected {
                                    cx.theme().foreground
                                } else {
                                    rgb(if cx.theme().is_dark() { 0xb4b4b5 } else { 0x4c4c4d }).into()
                                })
                                .when(selected, |row| row.font_weight(FontWeight::BOLD))
                                .child(
                                    Icon::empty()
                                        .path(format!("icons/{}.svg", page["icon"].as_str().unwrap_or_default()))
                                        .size_4(),
                                )
                                .when(!narrow, |row| {
                                    row.min_w_0().child(div().min_w_0().truncate().child(label))
                                }),
                        )
                        .selected(selected)
                        .on_click(cx.listener(move |this, _, window, cx| this.select(&id, window, cx))),
                );
            }
        }
        let (name, version) = self.version.split_once(' ').unwrap_or((self.version.as_str(), ""));
        let footer = div()
            .id("about")
            .flex()
            .items_center()
            .gap_2()
            .h(px(40.))
            .mx_1()
            .pl(px(2.))
            .border_t_1()
            .border_color(rgb(if cx.theme().is_dark() { 0x2b2b2e } else { 0xdcdcdd }))
            .child(
                // The logo of the look: the dark one on the dark theme; a development
                // build has its own (the tray's), so it is never taken for the installed app.
                img(if IS_DEV {
                    "brands/streamerz-dev.svg"
                } else if cx.theme().is_dark() {
                    "brands/streamerz-dark.svg"
                } else {
                    "brands/streamerz.svg"
                })
                .id("sidebar-logo")
                .size(px(24.))
                .flex_shrink_0(),
            )
            .when(!narrow, |footer| {
                footer.child(
                    div()
                        .flex()
                        .flex_col()
                        .min_w_0()
                        .child(
                            div()
                                .font_weight(FontWeight::BOLD)
                                .text_size(px(13.))
                                .child(name.to_owned()),
                        )
                        .child({
                            let ver = version.to_owned();
                            let ver_tip = ver.clone();
                            div()
                                .id("app-version")
                                .text_size(px(12.))
                                .text_color(crate::theme::quiet(cx))
                                .truncate()
                                .child(ver)
                                .when(!ver_tip.is_empty(), |el| {
                                    el.tooltip(move |window, cx| {
                                        gpui_kit::component::tooltip::Tooltip::new(ver_tip.clone()).build(window, cx)
                                    })
                                })
                        }),
                )
            });
        let navigation = div()
            .flex()
            .flex_col()
            .w(px(if narrow { 52. } else { 176. }))
            .flex_shrink_0()
            .h_full()
            .px(px(8.))
            .pt(px(8.))
            .bg(cx.theme().sidebar)
            .border_r_1()
            .border_color(rgb(if cx.theme().is_dark() { 0x2b2b2e } else { 0xd4d4d8 }))
            .child(fold_button)
            .child(navigation)
            .child(footer);
        let mut content = div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .px(px(10.))
            .pt(px(8.))
            .pb(px(10.))
            .gap_2()
            .when(
                self.lists.is_empty()
                    && self.sections.is_none()
                    && self.log.is_none()
                    && self.chat.is_none()
                    && self.actions.is_none(),
                |content| content.child(div().text_lg().child(title)),
            );
        if !self.error.is_empty() {
            content = content.child(div().text_color(cx.theme().danger).child(self.error.clone()));
        }
        let visible = self.visible_lists(cx);
        let page = self.selected.to_lowercase().replace(' ', "_");
        if visible.len() != 1 && self.data["tabs"].is_object() && self.data["domain"].as_str() == Some(page.as_str()) {
            let mut tabs = self.data["tabs"].clone();
            if let Some(kind) = self.view["tab"].as_str() {
                tabs["selected"] = kind.into();
            }
            let owner = cx.entity().downgrade();
            content = content.child(crate::controls::tabs::bar(&tabs, cx, move |kind, _, cx| {
                let _ = owner.update(cx, |this, cx| {
                    this.view["tab"] = kind.into();
                    this.request_read();
                    cx.notify();
                });
            }));
        }
        if let Some(log) = &self.log {
            content = content.child(div().flex_1().min_h_0().child(log.clone()));
        } else if let Some(chat) = &self.chat {
            content = content.child(div().flex_1().min_h_0().child(chat.clone()));
        } else if let Some(actions) = &self.actions {
            content = content.child(div().flex_1().min_h_0().child(actions.clone()));
        } else if let Some(sections) = &self.sections {
            content = content.child(
                div()
                    .id("page-scroll")
                    .flex_1()
                    .min_h_0()
                    .mt(px(-8.))
                    .mr(px(-10.))
                    .mb(px(-10.))
                    .pt(px(10.))
                    .pr(px(24.))
                    .overflow_y_scrollbar()
                    .child(sections.clone()),
            );
        } else if visible.len() == 1 {
            content = content.child(div().flex_1().min_h_0().child(visible[0].clone()));
        } else if !visible.is_empty() {
            let mut sections = div()
                .id("page-sections")
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .gap_1()
                // A page that ends in a read-only table fits without a scrollbar's room.
                .when(
                    visible.last().is_some_and(|list| list.read(cx).has_actions()),
                    |sections| sections.pr(px(14.)),
                )
                .overflow_y_scroll();
            // The first table's introduction sits above the sections, so they share the height evenly.
            let lead = visible.first().and_then(|list| {
                list.update(cx, |list, _| list.draw_intro_outside());
                list.read(cx).intro_block(cx)
            });
            if let Some(lead) = lead {
                content = content.child(lead);
            }
            for (index, list) in visible.iter().enumerate() {
                let last = index + 1 == visible.len();
                let section = if list.read(cx).has_inline() {
                    div().flex_shrink_0()
                } else if list.read(cx).is_settings() {
                    div().h(px(204.)).flex_shrink_0()
                } else if list.read(cx).is_connections() || list.read(cx).is_cards() {
                    div().flex_shrink_0().when(!last, |el| el.pb(px(12.)))
                } else if list.read(cx).has_connection() {
                    // Connection sections share the page evenly (page padding 20, footer 46, gaps 4).
                    let count = visible.len() as f32;
                    let each = (f32::from(window.viewport_size().height) - 66. - 4. * (count - 1.)) / count;
                    div().h(px(each.max(260.))).flex_shrink_0()
                } else if index + 1 == visible.len() && !list.read(cx).has_actions() {
                    // The last table (pending items) has a fixed height, the ones above take the rest.
                    div().h(px(247.)).flex_shrink_0()
                } else {
                    div()
                        .min_h(px(140.))
                        .flex_grow(1.)
                        .flex_shrink(1.)
                        .flex_basis(px(0.))
                        .overflow_hidden()
                };
                sections = sections.child(section.child(list.clone()));
            }
            // A list whose buttons come last on the page (MCP) draws them under the sections.
            for list in visible.iter().filter(|list| list.read(cx).buttons_last()) {
                sections = sections.child(list.read(cx).buttons_view());
            }
            content = content.child(sections);
        } else {
            content = content.child(div().flex_1());
        }
        let (icon, color) = crate::controls::health::appearance(self.status["health"].as_str().unwrap_or("off"), cx);
        let status_text = if self.status_error.is_empty() {
            self.status["text"].as_str().unwrap_or_default().to_owned()
        } else {
            self.status_error.clone()
        };
        let connections = self.status["connections"].clone();
        let bar = div()
            .flex()
            .items_center()
            .gap_2()
            .px(px(10.))
            .pt(px(6.))
            .pb(px(8.))
            .flex_shrink_0()
            .child(
                Button::new("save")
                    .when(small, |button| button.w(px(36.)))
                    .when(!small, |button| button.min_w(px(110.)))
                    .child(bar_content("save", tr("btn.save"), small, cx))
                    .accessibility_label(tr("btn.save"))
                    .tooltip(tr("ui.save_tooltip"))
                    .on_click(|_, _, _| send(json!({"request": "StreamerzCoreSave", "id": "save"}))),
            )
            .child(
                div()
                    .id("connections-status")
                    .flex()
                    .items_center()
                    .gap_2()
                    .flex_1()
                    .min_w_0()
                    .cursor_pointer()
                    .text_color(if self.status_error.is_empty() {
                        color
                    } else {
                        cx.theme().danger
                    })
                    .child(
                        div()
                            .ml(px(3.))
                            .child(Icon::empty().path(format!("icons/{icon}.svg")).small()),
                    )
                    .child({
                        let status_tip = status_text.clone();
                        div()
                            .id("status-text")
                            .truncate()
                            .child(status_text)
                            .when(!status_tip.is_empty(), |el| {
                                el.tooltip(move |window, cx| {
                                    gpui_kit::component::tooltip::Tooltip::new(status_tip.clone()).build(window, cx)
                                })
                            })
                    })
                    .on_click(
                        cx.listener(move |this, _, window, cx| this.show_connections(connections.clone(), window, cx)),
                    ),
            )
            .child(
                Button::new("hide")
                    .when(small, |button| button.w(px(36.)))
                    .when(!small, |button| button.min_w(px(110.)))
                    .child(bar_content("eye-off", tr("btn.hide"), small, cx))
                    .accessibility_label(tr("btn.hide"))
                    .on_click(|_, _, cx| cx.quit()),
            )
            .child(
                Button::new("quit")
                    .when(small, |button| button.w(px(36.)))
                    .when(!small, |button| button.min_w(px(110.)))
                    .child(bar_content("log-out", tr("btn.quit"), small, cx))
                    .accessibility_label(tr("btn.quit"))
                    .on_click(|_, _, _| send(json!({"request": "StreamerzCoreQuit", "id": "quit"}))),
            );
        div()
            .size_full()
            .flex()
            .flex_col()
            .font_family(cx.theme().font_family.clone())
            .text_size(cx.theme().font_size)
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .capture_any_mouse_down(
                cx.listener(|this, event: &MouseDownEvent, window, cx| match event.button {
                    MouseButton::Navigate(NavigationDirection::Back) => {
                        cx.stop_propagation();
                        this.go_back(window, cx);
                    }
                    MouseButton::Navigate(NavigationDirection::Forward) => {
                        cx.stop_propagation();
                        this.go_forward(window, cx);
                    }
                    _ => {}
                }),
            )
            .capture_any_mouse_up(|event, _, cx| {
                if matches!(event.button, MouseButton::Navigate(_)) {
                    cx.stop_propagation();
                }
            })
            .on_action(cx.listener(|_this, _: &CoreUndo, _window, _cx| {
                send(json!({"request": "StreamerzCoreAction", "domain": "actions", "action": "undo", "id": "undo"}));
            }))
            .on_action(cx.listener(|_this, _: &CoreRedo, _window, _cx| {
                send(json!({"request": "StreamerzCoreAction", "domain": "actions", "action": "redo", "id": "redo"}));
            }))
            .child(div().flex_1().min_h_0().flex().child(navigation).child(content))
            .child(bar)
    }
}

#[cfg(test)]
mod tests {
    use super::Frontend;
    use serde_json::json;

    #[test]
    fn history_navigation_tracks_pages_and_back_forward() {
        let mut frontend = Frontend::new(json!([]), "Streamerz".into(), "Chat".into());
        assert_eq!(frontend.selected, "Chat");
        assert_eq!(frontend.history, vec!["Chat".to_string()]);
        assert_eq!(frontend.history_index, 0);

        let custom = Frontend::new(json!([]), "Streamerz".into(), "Settings".into());
        assert_eq!(custom.selected, "Settings");
        assert_eq!(custom.history, vec!["Settings".to_string()]);

        // Select Actions
        frontend.history.truncate(frontend.history_index + 1);
        frontend.history.push("Actions".into());
        frontend.history_index = frontend.history.len() - 1;
        frontend.selected = "Actions".into();

        // Select Servers
        frontend.history.truncate(frontend.history_index + 1);
        frontend.history.push("Servers".into());
        frontend.history_index = frontend.history.len() - 1;
        frontend.selected = "Servers".into();

        assert_eq!(frontend.history, vec!["Chat", "Actions", "Servers"]);
        assert_eq!(frontend.history_index, 2);

        // Go back -> Actions
        frontend.history_index -= 1;
        frontend.selected = frontend.history[frontend.history_index].clone();
        assert_eq!(frontend.selected, "Actions");
        assert_eq!(frontend.history_index, 1);

        // Go back -> Chat
        frontend.history_index -= 1;
        frontend.selected = frontend.history[frontend.history_index].clone();
        assert_eq!(frontend.selected, "Chat");
        assert_eq!(frontend.history_index, 0);

        // Go forward -> Actions
        frontend.history_index += 1;
        frontend.selected = frontend.history[frontend.history_index].clone();
        assert_eq!(frontend.selected, "Actions");
        assert_eq!(frontend.history_index, 1);

        // Select Settings while in middle of history -> truncates forward history
        frontend.history.truncate(frontend.history_index + 1);
        frontend.history.push("Settings".into());
        frontend.history_index = frontend.history.len() - 1;
        frontend.selected = "Settings".into();

        assert_eq!(frontend.history, vec!["Chat", "Actions", "Settings"]);
        assert_eq!(frontend.history_index, 2);
    }
}
