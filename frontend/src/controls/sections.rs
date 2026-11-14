//! Pages of sections of labelled rows, drawn from a core model. Every change is a core request.
use super::form::Form;
use crate::send;
use gpui_kit::component::{
    button::Button,
    input::{Input, InputEvent, InputState},
    select::{Select, SelectEvent, SelectState},
    switch::Switch,
    ActiveTheme, Disableable, Icon, IndexPath, Sizable, WindowExt,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use serde_json::{json, Value};
use std::collections::HashMap;

/// Asks the page to show another page.
pub struct Navigate(pub String);
impl EventEmitter<Navigate> for Sections {}

pub struct Sections {
    model: Value,
    domain: String,
    /// Where responses to this page's requests come back, as an id prefix.
    route: String,
    selects: HashMap<String, Entity<SelectState<Vec<String>>>>,
    inputs: HashMap<String, Entity<InputState>>,
    form: Option<Entity<Form>>,
    dialog: Option<Entity<Sections>>,
    /// The channel finder, while it is open.
    finder: Option<Entity<super::find::Finder>>,
    /// A button asked for a dialog: the next dialog view opens it (a refresh never does).
    opening: bool,
    error: String,
    /// A narrow window: Settings stacks a row's control under its text.
    compact: bool,
    subscriptions: Vec<Subscription>,
}

impl Sections {
    pub fn new(model: Value, route: String, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut this = Self {
            domain: model["domain"].as_str().unwrap_or_default().to_owned(),
            model: Value::Null,
            route,
            selects: HashMap::new(),
            inputs: HashMap::new(),
            form: None,
            dialog: None,
            opening: false,
            finder: None,
            error: String::new(),
            compact: false,
            subscriptions: Vec::new(),
        };
        this.update_model(model, window, cx);
        this
    }
    pub fn update_model(&mut self, model: Value, window: &mut Window, cx: &mut Context<Self>) {
        for section in model["sections"].as_array().into_iter().flatten() {
            for row in section["rows"].as_array().into_iter().flatten() {
                let key = row["key"].as_str().unwrap_or_default().to_owned();
                let control = &row["control"];
                match control["type"].as_str() {
                    Some("select") => {
                        let options: Vec<String> = control["options"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(Value::as_str)
                            .map(str::to_owned)
                            .collect();
                        let selected = control["value"].as_u64().map(|index| IndexPath::new(index as usize));
                        match self.selects.get(&key) {
                            Some(select) => {
                                if select.read(cx).selected_index(cx) != selected {
                                    if let Some(selected) = selected {
                                        select.update(cx, |select, cx| {
                                            select.set_selected_index(Some(selected), window, cx)
                                        });
                                    }
                                }
                            }
                            None => {
                                let select = cx.new(|cx| SelectState::new(options, selected, window, cx));
                                let (domain, route, name) = (self.domain.clone(), self.route.clone(), key.clone());
                                self.subscriptions.push(cx.subscribe(&select, move |_, select, event: &SelectEvent<Vec<String>>, cx| {
                                    let SelectEvent::Confirm(_) = event;
                                    if let Some(index) = select.read(cx).selected_index(cx) {
                                        send(json!({"request": "StreamerzCoreSetting", "domain": domain, "key": name, "value": index.row, "id": format!("{route}:set")}));
                                    }
                                }));
                                self.selects.insert(key.clone(), select);
                            }
                        }
                    }
                    Some("text") if !self.inputs.contains_key(&key) => {
                        let input = cx.new(|cx| {
                            InputState::new(window, cx)
                                .default_value(control["value"].as_str().unwrap_or_default().to_owned())
                        });
                        let (domain, route, name) = (self.domain.clone(), self.route.clone(), key.clone());
                        // A field that restarts something saves when it is left or Enter is pressed, not per key.
                        let commit = control["commit"] == true;
                        self.subscriptions.push(cx.subscribe(&input, move |_, input, event: &InputEvent, cx| {
                            let sends = if commit {
                                matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. })
                            } else {
                                matches!(event, InputEvent::Change)
                            };
                            if sends {
                                let id = format!("{route}:{}", if commit { "commit" } else { "text" });
                                send(json!({"request": "StreamerzCoreSetting", "domain": domain, "key": name, "value": input.read(cx).value().to_string(), "id": id}));
                            }
                        }));
                        self.inputs.insert(key, input);
                    }
                    _ => {}
                }
            }
        }
        self.model = model;
        // An open dialog follows the core too (a login finishing, an extension connecting).
        if !window.has_active_dialog(cx) {
            self.dialog = None;
        }
        if let Some(dialog) = &self.dialog {
            let domain = dialog.read(cx).domain.clone();
            send(
                json!({"request": "StreamerzCoreView", "domain": domain, "id": format!("{}:dialog-view", self.route)}),
            );
        }
        cx.notify();
    }

    fn set(&self, key: &str, value: Value) {
        send(
            json!({"request": "StreamerzCoreSetting", "domain": self.domain, "key": key, "value": value, "id": format!("{}:set", self.route)}),
        );
    }

    fn press(&mut self, button: Value, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(page) = button["go"].as_str() {
            cx.emit(Navigate(page.to_owned()));
        } else if let Some(domain) = button["dialog"].as_str() {
            self.opening = true;
            send(
                json!({"request": "StreamerzCoreView", "domain": domain, "id": format!("{}:dialog-view", self.route)}),
            );
        } else if let Some(kind) = button["form"].as_str() {
            send(json!({"request": "StreamerzEntityForm", "kind": kind, "id": format!("{}:form", self.route)}));
        } else {
            if let Some(text) = button["copy"].as_str() {
                cx.write_to_clipboard(ClipboardItem::new_string(text.to_owned()));
            }
            send(
                json!({"request": "StreamerzCoreAction", "domain": self.domain, "action": button["id"], "id": format!("{}:act", self.route)}),
            );
        }
        let _ = window;
    }

    /// Returns whether the page should read the core again.
    pub fn apply(&mut self, operation: &str, value: Value, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if let Some(rest) = operation.strip_prefix("dialog:") {
            return match &self.dialog {
                Some(dialog) => dialog.update(cx, |dialog, cx| dialog.apply(rest, value, window, cx)),
                None => false,
            };
        }
        // A search's answer (or its error) is the finder's.
        if let Some(number) = operation.strip_prefix("find:") {
            if let Some(finder) = &self.finder {
                let number = number.parse().unwrap_or(0);
                finder.update(cx, |finder, cx| finder.found(number, value, cx));
            }
            return false;
        }
        if value["status"] == "error" {
            let error = value["error"].as_str().unwrap_or_default().to_owned();
            if matches!(operation, "save" | "json") {
                if let Some(form) = &self.form {
                    form.update(cx, |form, cx| {
                        form.pending = false;
                        form.error = error;
                        cx.notify();
                    });
                }
            } else {
                self.error = error;
            }
            cx.notify();
            return operation == "set";
        }
        self.error.clear();
        match operation {
            "dialog-view" => {
                if let Some(dialog) = &self.dialog {
                    if window.has_active_dialog(cx) {
                        if dialog.read(cx).domain == value["domain"].as_str().unwrap_or_default() {
                            dialog.update(cx, |dialog, cx| dialog.update_model(value, window, cx));
                        }
                        return false;
                    }
                }
                // A refresh that arrives after the dialog closed opens nothing; only a press does.
                self.dialog = None;
                if !std::mem::take(&mut self.opening) {
                    return false;
                }
                let title = value["sections"][0]["title"].as_str().unwrap_or_default().to_owned();
                let nested = cx.new(|cx| Sections::new(value, format!("{}:dialog", self.route), window, cx));
                self.dialog = Some(nested.clone());
                let owner = cx.entity().downgrade();
                window.open_dialog(cx, move |dialog, _, cx| {
                    let owner = owner.clone();
                    let close = crate::controls::dialog::close_footer(cx);
                    crate::controls::dialog::shell(dialog, title.clone(), 560., cx)
                        .child(crate::controls::dialog::body(nested.clone()))
                        .footer(close)
                        .on_cancel(move |_, _, cx| {
                            let _ = owner.update(cx, |sections, _| sections.dialog = None);
                            true
                        })
                });
                false
            }
            "form" => {
                let title = value["title"].as_str().unwrap_or_default().to_owned();
                let form = cx.new(|cx| Form::new(value, format!("{}:save", self.route), window, cx));
                self.subscriptions
                    .push(cx.subscribe(&form, |sections, _, _: &super::form::Cancelled, cx| {
                        sections.form = None;
                        cx.notify();
                    }));
                self.form = Some(form.clone());
                window.open_dialog(cx, move |dialog, _, cx| {
                    crate::controls::dialog::shell(dialog, title.clone(), 560., cx)
                        .child(crate::controls::dialog::body(form.clone()))
                        .footer(form.read(cx).buttons_view())
                });
                false
            }
            "json" => {
                if let Some(form) = &self.form {
                    form.update(cx, |form, cx| {
                        form.show_json(value["json"].as_str().unwrap_or_default(), window, cx)
                    });
                }
                false
            }
            "save" => {
                self.form = None;
                window.close_dialog(cx);
                true
            }
            "act" if value["finder"].is_object() => {
                let model = value["finder"].clone();
                let title = model["title"].as_str().unwrap_or_default().to_owned();
                let finder = cx.new(|cx| super::find::Finder::new(model, self.route.clone(), window, cx));
                self.finder = Some(finder.clone());
                let owner = cx.entity().downgrade();
                window.open_dialog(cx, move |dialog, _, cx| {
                    let owner = owner.clone();
                    crate::controls::dialog::shell(dialog, title.clone(), 560., cx)
                        .child(crate::controls::dialog::body(finder.clone()))
                        .footer(finder.read(cx).buttons_view())
                        .on_cancel(move |_, _, cx| {
                            let _ = owner.update(cx, |sections, _| sections.finder = None);
                            true
                        })
                });
                // The dialog takes the focus as it opens: the box gets it after.
                let focus = self.finder.clone();
                window.defer(cx, move |window, cx| {
                    if let Some(finder) = focus {
                        finder.update(cx, |finder, cx| finder.focus(window, cx));
                    }
                });
                false
            }
            "find-images" => {
                if let Some(finder) = &self.finder {
                    finder.update(cx, |finder, cx| finder.images(&value, cx));
                }
                false
            }
            "act" | "set" | "commit" => true,
            _ => false,
        }
    }

    fn control(&self, row: &Value, key: &str, cx: &mut Context<Self>) -> AnyElement {
        let control = &row["control"];
        let owner = cx.entity().downgrade();
        let roomy = self.domain == "settings";
        let compact = roomy && self.compact;
        // A narrow window folds the long buttons of an account's lines to their icon.
        let fold = !roomy && self.compact;
        let buttons = |list: &[Value], cx: &mut Context<Self>| {
            // Never wider than its place: buttons that don't fit on one line go on the next, at the right.
            let mut line = div()
                .flex()
                .flex_wrap()
                .justify_end()
                .gap_2()
                .flex_shrink_0()
                .max_w_full()
                .when(compact, |line| line.w_full());
            for (index, button) in list.iter().enumerate() {
                let spec = button.clone();
                let label = button["label"].as_str().unwrap_or_default().to_owned();
                let login = spec["id"]
                    .as_str()
                    .is_some_and(|id| id.ends_with("login") || id.ends_with("logout"));
                // A gear and the like are always just their icon (the label is its tooltip).
                let icon_only = (fold && !login && spec["icon"].is_string())
                    || spec["icon_only"] == true
                    || spec["iconOnly"] == true;
                let mut widget = Button::new((
                    SharedString::from(format!("{key}-{}", spec["id"].as_str().unwrap_or_default())),
                    index,
                ))
                .small()
                .h(px(26.))
                .font_weight(FontWeight::NORMAL)
                .min_w(px(if icon_only {
                    36.
                } else if label.chars().count() > 5 {
                    120.
                } else {
                    90.
                }))
                .when(icon_only, |button| button.w(px(36.)))
                // A column of equal buttons, each as wide as its label needs past that.
                .when(roomy && !compact, |button| {
                    button.min_w(px(if list.len() == 1 { 150. } else { 114. }))
                })
                .when(compact, |button| button.flex_1())
                .child(super::body::button_body(
                    spec["icon"].as_str(),
                    (!icon_only).then(|| label.clone()),
                    cx,
                ))
                .when(icon_only, |button| button.tooltip(label.clone()))
                .disabled(spec["enabled"] == false)
                .accessibility_label(label);
                if let Some(tip) = spec["tooltip"].as_str() {
                    widget = widget.tooltip(tip.to_owned());
                }
                line = line
                    .child(widget.on_click(
                        cx.listener(move |sections, _, window, cx| sections.press(spec.clone(), window, cx)),
                    ));
            }
            line
        };
        match control["type"].as_str() {
            Some("switch") => {
                let key = key.to_owned();
                let mut line = div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .flex_shrink_0()
                    .when(roomy, |line| line.px(px(4.)))
                    .when(roomy && control["button"].is_object(), |line| {
                        line.w(px(180.)).justify_start()
                    });
                line = line.child(
                    Switch::new(SharedString::from(format!("switch-{key}")))
                        .accessibility_label(row["label"].as_str().unwrap_or_default().to_owned())
                        .checked(control["value"] == true)
                        .on_click(cx.listener({
                            let key = key.clone();
                            move |sections, checked: &bool, _, _| sections.set(&key, json!(checked))
                        })),
                );
                if control["button"].is_object() {
                    line = line.child(buttons(std::slice::from_ref(&control["button"]), cx));
                }
                line.into_any_element()
            }
            Some("select") => {
                let mut line = div().flex().items_center().gap_2().flex_shrink_0();
                if let Some(select) = self.selects.get(key) {
                    line = line.child(
                        div()
                            .when(compact, |select| select.w_full())
                            .when(!compact, |select| select.w(px(if roomy { 200. } else { 230. })))
                            .child(
                                Select::new(select)
                                    .small()
                                    .icon(Icon::empty().path("icons/chevrons-up-down.svg").size(px(16.)))
                                    .when(roomy, |select| select.h(px(26.))),
                            ),
                    );
                }
                if control["button"].is_object() {
                    line = line.child(buttons(std::slice::from_ref(&control["button"]), cx));
                }
                line.into_any_element()
            }
            Some("text") => match self.inputs.get(key) {
                Some(input) => div()
                    .w(px(230.))
                    .flex_shrink_0()
                    .child(
                        Input::new(input)
                            .small()
                            .aria_label(row["label"].as_str().unwrap_or_default().to_owned()),
                    )
                    .into_any_element(),
                None => div().into_any_element(),
            },
            Some("buttons") => buttons(control["buttons"].as_array().map_or(&[], Vec::as_slice), cx).into_any_element(),
            _ => {
                let _ = owner;
                div().into_any_element()
            }
        }
    }
}

/// A card's frame: rounded, bordered, a shade apart from the page.
fn card_frame(index: usize, content: Div, cx: &App) -> Stateful<Div> {
    div()
        .id(("card", index))
        .flex()
        .flex_col()
        .p_4()
        .flex_1()
        .min_w_0()
        .rounded(px(8.))
        .border_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().sidebar)
        .child(content)
}

/// A small rounded tag: a category, a stream tag.
fn chip(icon: Option<&str>, text: String, cx: &App) -> Div {
    div()
        .flex()
        .items_center()
        .gap_1()
        .h(px(22.))
        .px_2()
        .rounded(px(11.))
        .bg(cx.theme().muted)
        .text_size(px(12.))
        .text_color(cx.theme().foreground)
        .max_w(px(260.))
        .when_some(icon, |chip, icon| {
            chip.child(
                Icon::empty()
                    .path(format!("icons/{icon}.svg"))
                    .size(px(12.))
                    .text_color(super::body::icon_tone(cx))
                    .flex_shrink_0(),
            )
        })
        .child({
            let tip = text.clone();
            div()
                .id(SharedString::from(format!("chip-{text}")))
                .min_w_0()
                .truncate()
                .child(text)
                .when(!tip.is_empty(), |el| {
                    el.tooltip(move |window, cx| {
                        gpui_kit::component::tooltip::Tooltip::new(tip.clone()).build(window, cx)
                    })
                })
        })
}

impl Sections {
    /// A section drawn as a card (`"card": true`): its logo, title and short state (the reason on hover)
    /// on top, then a headline, chips and quiet lines, and its buttons at the bottom under a line.
    fn card(&self, section: &Value, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let state = &section["state"];
        let (icon, color) = super::health::appearance(state["health"].as_str().unwrap_or_default(), cx);
        let tip = state["tip"]
            .as_str()
            .filter(|tip| !tip.is_empty())
            .map(|tip| SharedString::from(tip.to_owned()));
        let mut header = div()
            .flex()
            .items_center()
            .gap_2()
            .h(px(24.))
            .when_some(section["brand"].as_str(), |header, brand| {
                header.child(
                    img(format!("brands/{brand}.svg"))
                        .id(SharedString::from(format!("card-brand-{index}")))
                        .size(px(20.))
                        .flex_shrink_0(),
                )
            })
            .child(
                div()
                    .font_weight(FontWeight::BOLD)
                    .text_size(px(15.))
                    .text_color(crate::theme::strong(cx))
                    .child(section["title"].as_str().unwrap_or_default().to_owned()),
            )
            .child(div().flex_1());
        if state.is_object() {
            header = header.child(
                div()
                    .id(("card-state", index))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .flex_shrink_0()
                    .text_size(px(12.5))
                    .text_color(color)
                    .child(Icon::empty().path(format!("icons/{icon}.svg")).size(px(14.)))
                    .child(state["text"].as_str().unwrap_or_default().to_owned())
                    .when_some(tip, |state, tip| {
                        state.tooltip(move |window, cx| {
                            gpui_kit::component::tooltip::Tooltip::new(tip.clone()).build(window, cx)
                        })
                    }),
            );
        }
        let mut body = div().flex().flex_col().gap_2().flex_1();
        if let Some(headline) = section["headline"].as_str() {
            let muted = section["headlineMuted"] == true;
            body = body.child(
                div()
                    .text_size(px(15.))
                    .line_height(px(20.))
                    .line_clamp(2)
                    .text_color(if muted {
                        crate::theme::quiet(cx)
                    } else {
                        crate::theme::strong(cx)
                    })
                    .child(headline.to_owned()),
            );
        }
        let chips = section["chips"].as_array().cloned().unwrap_or_default();
        if !chips.is_empty() {
            body = body.child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(6.))
                    .children(chips.iter().map(|chip_spec| {
                        chip(
                            chip_spec["icon"].as_str(),
                            chip_spec["text"].as_str().unwrap_or_default().to_owned(),
                            cx,
                        )
                    })),
            );
        }
        for line in section["lines"].as_array().into_iter().flatten() {
            let muted = line["muted"] == true;
            body = body.child(
                div()
                    .flex()
                    .items_start()
                    .gap_2()
                    .text_size(px(12.5))
                    .line_height(px(17.))
                    .text_color(if muted {
                        crate::theme::quiet(cx)
                    } else {
                        theme.foreground
                    })
                    .when_some(line["icon"].as_str(), |line, icon| {
                        line.child(
                            Icon::empty()
                                .path(format!("icons/{icon}.svg"))
                                .size(px(14.))
                                .text_color(super::body::icon_tone(cx))
                                .mt(px(1.5))
                                .flex_shrink_0(),
                        )
                    })
                    .child(
                        div()
                            .min_w_0()
                            .child(line["text"].as_str().unwrap_or_default().to_owned()),
                    ),
            );
        }
        let mut footer = div()
            .flex()
            .flex_wrap()
            .gap_2()
            .pt_3()
            .border_t_1()
            .border_color(theme.border);
        for (button_index, spec) in section["buttons"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .enumerate()
        {
            let label = spec["label"].as_str().unwrap_or_default().to_owned();
            let id = spec["id"].as_str().unwrap_or_default().to_owned();
            let mut widget = Button::new((SharedString::from(format!("card-{index}-{id}")), button_index))
                .small()
                .h(px(26.))
                .font_weight(FontWeight::NORMAL)
                .child(super::body::button_body(spec["icon"].as_str(), Some(label.clone()), cx))
                .disabled(spec["enabled"] == false)
                .accessibility_label(label);
            if let Some(tooltip) = spec["tooltip"].as_str() {
                widget = widget.tooltip(tooltip.to_owned());
            }
            footer =
                footer
                    .child(widget.on_click(
                        cx.listener(move |sections, _, window, cx| sections.press(spec.clone(), window, cx)),
                    ));
        }
        card_frame(
            index,
            div()
                .flex()
                .flex_col()
                .flex_1()
                .gap_3()
                .child(header)
                .child(body)
                .child(footer),
            cx,
        )
        .into_any_element()
    }
}

impl Render for Sections {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.compact = f32::from(window.viewport_size().width) < 700.;
        // Settings is laid out roomier than the other pages.
        let roomy = self.domain == "settings";
        let mut page = div()
            .id(SharedString::from(format!("sections-{}", self.domain)))
            .flex()
            .flex_col()
            .gap(px(if roomy { 2. } else { 4. }))
            .w_full();
        if !self.error.is_empty() {
            page = page.child(super::notice::error(&self.error, cx));
        }
        let sections = self.model["sections"].as_array().cloned().unwrap_or_default();
        // Cards sit side by side, two to a row when there is room.
        let width = f32::from(window.viewport_size().width);
        let columns = if width >= 1000. { 2 } else { 1 };
        // Cards of rows go two to a row only on a very wide window: their lines need the room.
        let wide_columns = if width >= 1600. { 2 } else { 1 };
        let mut cards: Vec<AnyElement> = Vec::new();
        // Cards holding rows (an account's) are wider than the others.
        let mut wide = false;
        let grid = |cards: &mut Vec<AnyElement>, wide: &mut bool| {
            let columns = if std::mem::take(wide) { wide_columns } else { columns };
            let mut grid = div().flex().flex_col().gap_3().w_full().pt_2();
            let mut drained: Vec<AnyElement> = std::mem::take(cards);
            while !drained.is_empty() {
                let rest = drained.split_off(drained.len().min(columns));
                let row_cards = std::mem::replace(&mut drained, rest);
                // A row short of cards keeps their width: empty places beside them.
                let short = columns - row_cards.len();
                let row = div()
                    .flex()
                    .gap_3()
                    .w_full()
                    .children(
                        row_cards
                            .into_iter()
                            .map(|card| div().flex().flex_1().min_w_0().child(card)),
                    )
                    .children((0..short).map(|_| div().flex_1()));
                grid = grid.child(row);
            }
            grid
        };
        for (section_index, section) in sections.iter().enumerate() {
            // A card of rows draws its section as usual, inside the card's frame.
            let boxed = section["card"] == true && section["rows"].is_array();
            let after_cards = section_index > 0 && sections[section_index - 1]["card"] == true;
            if section["card"] == true && !boxed {
                cards.push(self.card(section, section_index, cx));
                continue;
            }
            if !cards.is_empty() && !boxed {
                page = page.child(grid(&mut cards, &mut wide));
            }
            let mut block = div().flex().flex_col().gap(px(if roomy { 2. } else { 4. })).w_full();
            let title = section["title"].as_str().unwrap_or_default().to_owned();
            let titled = !title.is_empty();
            // A platform's section: its brand and state on the heading's line.
            let plat = section["inline"] == true && section["brand"].is_string();
            let mut heading = div()
                .flex()
                .items_center()
                .gap_2()
                .h(px(if roomy { 34. } else { 24. }))
                .font_weight(FontWeight::BOLD)
                .text_size(px(15.))
                .text_color(crate::theme::strong(cx))
                .when(plat && !boxed, |heading| {
                    heading.when(section_index > 0, |heading| heading.mt(px(18.)))
                });
            if let Some(brand) = section["brand"].as_str() {
                heading = heading.child(
                    img(format!("brands/{brand}.svg"))
                        .id(SharedString::from(format!("brand-{brand}")))
                        .size(px(if plat {
                            18.
                        } else if section["inline"] == true {
                            22.
                        } else {
                            20.
                        }))
                        .flex_shrink_0(),
                );
            } else if let Some(icon) = section["icon"].as_str().filter(|icon| !icon.is_empty()) {
                heading = heading.child(Icon::empty().path(format!("icons/{icon}.svg")).small());
            }
            // An inline section puts its first row's state and button on the heading's line.
            let inline = section["inline"] == true;
            let mut rows = section["rows"].as_array().map_or(&[][..], Vec::as_slice);
            if !title.is_empty() && !self.route.ends_with(":dialog") {
                if let (true, Some(first)) = (inline, rows.first().filter(|row| row["status"].is_object())) {
                    let (icon, color) =
                        super::health::appearance(first["status"]["health"].as_str().unwrap_or_default(), cx);
                    heading = heading
                        .h(px(26.))
                        .text_size(cx.theme().font_size)
                        .child(
                            div()
                                .min_w(px(if plat { 83. } else { 110. }))
                                .flex_shrink_0()
                                .when(plat, |name| name.text_size(px(14.)))
                                .child(title),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .flex_1()
                                .font_weight(FontWeight::NORMAL)
                                .text_size(px(13.5))
                                .text_color(color)
                                .min_w_0()
                                .overflow_hidden()
                                .child(Icon::empty().path(format!("icons/{icon}.svg")).small().flex_shrink_0())
                                .child({
                                    let st = first["status"]["text"].as_str().unwrap_or_default().to_owned();
                                    let st_tip = st.clone();
                                    div()
                                        .id(SharedString::from(format!("card-st-{section_index}")))
                                        .min_w_0()
                                        .truncate()
                                        .child(st)
                                        .when(!st_tip.is_empty(), |el| {
                                            el.tooltip(move |window, cx| {
                                                gpui_kit::component::tooltip::Tooltip::new(st_tip.clone())
                                                    .build(window, cx)
                                            })
                                        })
                                }),
                        );
                    if first["control"].is_object() {
                        let key = format!("~{section_index}.0");
                        heading = heading.child(
                            div()
                                .flex()
                                .font_weight(FontWeight::NORMAL)
                                .child(self.control(first, &key, cx)),
                        );
                    }
                    rows = &rows[1..];
                } else {
                    heading = heading.child(title);
                }
                block = block.child(heading);
            }
            // A flow section lays its inputs side by side on one line, each with its label.
            let flow = section["flow"] == true;
            let mut flowing: Vec<AnyElement> = Vec::new();
            for (row_index, row) in rows.iter().enumerate() {
                // A row without a key still needs its own element ids (two Edit buttons must not share one).
                let key = match row["key"].as_str().filter(|key| !key.is_empty()) {
                    Some(key) => key.to_owned(),
                    None => format!("~{section_index}.{row_index}"),
                };
                let label = row["label"].as_str().unwrap_or_default().to_owned();
                let about = row["about"].as_str().unwrap_or_default().to_owned();
                if let (true, Some(input)) = (flow, self.inputs.get(&key).filter(|_| row["control"]["type"] == "text"))
                {
                    flowing.push(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .flex_1()
                            .min_w_0()
                            .child(div().flex_shrink_0().child(label))
                            .child(
                                div().flex_1().min_w_0().child(
                                    Input::new(input)
                                        .small()
                                        .aria_label(row["label"].as_str().unwrap_or_default().to_owned()),
                                ),
                            )
                            .into_any_element(),
                    );
                    continue;
                }
                if !flowing.is_empty() {
                    block = block.child(div().flex().gap_3().w_full().min_h(px(30.)).children(flowing.drain(..)));
                }
                // An introduction alone in its row gets a two line box.
                let about_only = !about.is_empty()
                    && (!titled || inline)
                    && label.is_empty()
                    && row["value"].is_null()
                    && row["status"].is_null()
                    && !row["control"].is_object();
                let mut text = div().flex().flex_col().flex_1().min_w_0().gap_0p5();
                if row["logo"] == true {
                    text = text.font_weight(FontWeight::BOLD).text_size(px(15.));
                }
                let value = row["value"].as_str().map(str::to_owned);
                // A platform's channel is one wide button: it shows the channel and opens the picker.
                let pick = row["control"]["buttons"][0].clone();
                let channel = plat && key.ends_with(".channel") && pick.is_object();
                if let (true, Some(value)) = (channel, value.clone()) {
                    let spec = pick.clone();
                    // The channel's picture when it is known, else the search icon.
                    let face = row["picture"]
                        .as_str()
                        .filter(|file| !file.is_empty())
                        .map(std::path::PathBuf::from);
                    text = text.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(11.))
                            .child(
                                div()
                                    .w(px(107.))
                                    .flex_shrink_0()
                                    .text_right()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(label),
                            )
                            .child(
                                Button::new((SharedString::from(format!("{key}-pick")), 0usize))
                                    .small()
                                    .text_size(px(13.))
                                    .h(px(26.))
                                    .w(px(220.))
                                    .when(face.is_none(), |button| {
                                        button.icon(Icon::empty().path("icons/search.svg")).label(value.clone())
                                    })
                                    .when_some(face, |button, face| {
                                        button.child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap_2()
                                                .child(
                                                    img(face).size(px(18.)).rounded_full().object_fit(ObjectFit::Cover),
                                                )
                                                .child(value.clone()),
                                        )
                                    })
                                    .accessibility_label(value)
                                    .on_click(cx.listener(move |sections, _, window, cx| {
                                        sections.press(spec.clone(), window, cx)
                                    })),
                            )
                            // Whose channel it is, beside the button.
                            .child({
                                let about_tip = about.clone();
                                div()
                                    .id(SharedString::from(format!("about-{key}")))
                                    .min_w_0()
                                    .truncate()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(about.clone())
                                    .when(!about_tip.is_empty(), |el| {
                                        el.tooltip(move |window, cx| {
                                            gpui_kit::component::tooltip::Tooltip::new(about_tip.clone())
                                                .build(window, cx)
                                        })
                                    })
                            }),
                    );
                } else if let Some(value) = value {
                    // A labelled value (an account, a channel): the label stays in a quiet column.
                    let muted = row["muted"] == true;
                    text = text.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(if plat { 11. } else { 12. }))
                            .child(
                                div()
                                    .w(px(if plat { 107. } else { 110. }))
                                    .flex_shrink_0()
                                    .when(plat, |label| label.text_right())
                                    .text_color(cx.theme().muted_foreground)
                                    .child(label),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .when(
                                        plat && (key.ends_with(".account") || key.ends_with(".bot"))
                                            && row["picture"].is_null(),
                                        // No picture (yet): a person in its place; none when nobody's logged in.
                                        |line| {
                                            line.child(
                                                div()
                                                    .size(px(24.))
                                                    .flex_shrink_0()
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .when(!muted, |place| {
                                                        place.rounded_full().bg(cx.theme().muted).child(
                                                            Icon::empty()
                                                                .path("icons/user-round.svg")
                                                                .size(px(14.))
                                                                .text_color(super::body::icon_tone(cx)),
                                                        )
                                                    }),
                                            )
                                        },
                                    )
                                    .when_some(row["picture"].as_str(), |line, file| {
                                        line.child(
                                            img(std::path::PathBuf::from(file))
                                                .id(SharedString::from(format!("picture-{key}")))
                                                .size(px(if plat { 24. } else { 20. }))
                                                .rounded_full()
                                                .flex_shrink_0()
                                                .object_fit(ObjectFit::Cover),
                                        )
                                    })
                                    .child({
                                        let val_tip = value.clone();
                                        div()
                                            .id(SharedString::from(format!("val-{key}")))
                                            .min_w_0()
                                            .truncate()
                                            .when(muted, |v| v.text_color(cx.theme().muted_foreground))
                                            .child(value)
                                            .when(!val_tip.is_empty(), |el| {
                                                el.tooltip(move |window, cx| {
                                                    gpui_kit::component::tooltip::Tooltip::new(val_tip.clone())
                                                        .build(window, cx)
                                                })
                                            })
                                    }),
                            ),
                    );
                } else if let (true, Some(status)) = (plat && !label.is_empty(), row["status"].as_object()) {
                    // A state beside its label, in the same columns as an account's value.
                    let (icon, color) = super::health::appearance(status["health"].as_str().unwrap_or_default(), cx);
                    text = text.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(11.))
                            .child(
                                div()
                                    .w(px(107.))
                                    .flex_shrink_0()
                                    .text_right()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(label),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(7.))
                                    .flex_1()
                                    .min_w_0()
                                    .when(row["picture"].is_null(), |line| line.pl(px(34.)))
                                    .when_some(row["picture"].as_str(), |line, file| {
                                        line.child(
                                            img(std::path::PathBuf::from(file))
                                                .id(SharedString::from(format!("picture-{key}")))
                                                .size(px(24.))
                                                .rounded_full()
                                                .flex_shrink_0()
                                                .mr(px(3.))
                                                .object_fit(ObjectFit::Cover),
                                        )
                                    })
                                    .text_color(color)
                                    .child(Icon::empty().path(format!("icons/{icon}.svg")).small().flex_shrink_0())
                                    .child({
                                        let st = status["text"].as_str().unwrap_or_default().to_owned();
                                        let st_tip = st.clone();
                                        div()
                                            .id(SharedString::from(format!("st-{key}")))
                                            .min_w_0()
                                            .truncate()
                                            .child(st)
                                            .when(!st_tip.is_empty(), |el| {
                                                el.tooltip(move |window, cx| {
                                                    gpui_kit::component::tooltip::Tooltip::new(st_tip.clone())
                                                        .build(window, cx)
                                                })
                                            })
                                    }),
                            ),
                    );
                } else if let Some(status) = row["status"].as_object() {
                    if !label.is_empty() {
                        text = text.child(div().font_weight(FontWeight::BOLD).child(label));
                    }
                    let (icon, color) = super::health::appearance(status["health"].as_str().unwrap_or_default(), cx);
                    text = text.child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .text_size(px(13.5))
                            .text_color(color)
                            .child(Icon::empty().path(format!("icons/{icon}.svg")).small().flex_shrink_0())
                            .child(
                                div()
                                    .min_w_0()
                                    .flex_1()
                                    .child(status["text"].as_str().unwrap_or_default().to_owned()),
                            ),
                    );
                } else if !label.is_empty() {
                    text = text.child(
                        div()
                            .when(roomy, |label| label.h(px(19.)).line_height(px(15.)))
                            .child(label),
                    );
                }
                if !about.is_empty() && !channel {
                    text = text.child(
                        div()
                            .when(!roomy, |about| about.px(px(3.)).text_size(px(13.4)))
                            .when(about_only && !roomy, |about| {
                                about.min_h(px(36.)).flex().flex_col().justify_center()
                            })
                            .when(about_only && !roomy && section_index > 0 && !after_cards, |about| {
                                about.mt(px(4.)).min_h(px(141.)).line_height(px(17.))
                            })
                            // Under cards: a quiet note, close to them.
                            .when(about_only && after_cards, |about| about.mt(px(6.)).line_height(px(17.)))
                            .when(roomy, |about| {
                                about.relative().top(px(-1.)).text_size(px(12.)).line_height(px(14.))
                            })
                            .text_color(crate::theme::quiet(cx))
                            .children(about.split('\n').map(|line| div().child(line.to_owned()))),
                    );
                }
                let mut line = div()
                    .flex()
                    .items_center()
                    .gap(px(if roomy { 16. } else { 12. }))
                    .min_h(px(26.))
                    .w_full();
                // Narrow: a select or buttons go under the text, a switch stays beside it.
                let stacked = roomy && self.compact && row["control"].is_object() && row["control"]["type"] != "switch";
                if stacked {
                    line = line.flex_col().items_stretch().gap(px(0.)).pb(px(14.));
                }
                if roomy {
                    text = text.pb(px(if stacked { 7. } else { 12. }));
                    if row["logo"] == true {
                        text = text.self_start();
                    }
                }
                if row["logo"] == true {
                    line = line
                        .min_h(px(64.))
                        .child(img("brands/streamerz.svg").id("app-logo").size(px(56.)).flex_shrink_0());
                } else if let Some(brand) = row["brand"].as_str().filter(|b| !b.is_empty()) {
                    line = line.child(img(format!("brands/{brand}.svg")).size(px(28.)).flex_shrink_0());
                } else if let Some(icon) = row["icon"].as_str().filter(|i| !i.is_empty()) {
                    line = line.child(img(format!("brands/{icon}.svg")).size(px(28.)).flex_shrink_0());
                }
                // Text keeps a readable width beside its buttons: when both don't fit, the buttons go under
                // it, at the right, instead of squeezing it to a word per line.
                let beside = row["control"].is_object() && !channel;
                let has_text = row["label"].as_str().is_some_and(|label| !label.is_empty())
                    || !about.is_empty()
                    || row["value"].is_string()
                    || row["status"].is_object();
                if beside && has_text && !roomy {
                    line = line.flex_wrap();
                    text = text.min_w(px(200.));
                }
                line = line.child(text);
                if beside {
                    line = line.child(div().ml_auto().max_w_full().child(self.control(row, &key, cx)));
                }
                block = block.child(line);
            }
            if !flowing.is_empty() {
                block = block.child(div().flex().gap_3().w_full().min_h(px(30.)).children(flowing.drain(..)));
            }
            if boxed {
                wide = true;
                cards.push(card_frame(section_index, block, cx).into_any_element());
            } else {
                page = page.child(block);
            }
        }
        if !cards.is_empty() {
            page = page.child(grid(&mut cards, &mut wide));
        }
        page.pb_4()
    }
}
