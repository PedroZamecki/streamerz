//! Pages of sections of labelled rows, drawn from a core model. Every change is a core request.
use super::form::Form;
use crate::send;
use crate::theme::{
    self, CONTROL_HEIGHT, FONT_BODY, FONT_CAPTION, FONT_SECTION_TITLE, LINE_BODY, LINE_CAPTION, LINE_SECTION_TITLE,
    SECTION_GAP, SPACE_LG, SPACE_MD, SPACE_SM, SPACE_XS,
};
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

/// The quiet column of labels in an account's lines, and the width of a platform's name on its card.
const LABEL_WIDTH: f32 = 112.;
const PLATFORM_NAME_WIDTH: f32 = 96.;
/// A person's picture beside a value.
const PICTURE: f32 = 24.;

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
    search: Option<Entity<InputState>>,
    form: Option<Entity<Form>>,
    dialog: Option<Entity<Sections>>,
    /// The channel finder, while it is open.
    finder: Option<Entity<super::find::Finder>>,
    /// A button asked for a dialog: the next dialog view opens it (a refresh never does).
    opening: bool,
    /// That button's label: the dialog's title when its model has none (Chat settings once had none).
    opening_title: String,
    error: Option<crate::error::StreamerzError>,
    /// A narrow window: Settings stacks a row's control under its text.
    compact: bool,
    /// The title of the page these sections fill (set by the page; empty in a dialog or inside a list).
    page_title: String,
    subscriptions: Vec<Subscription>,
}

impl Sections {
    pub fn new(model: Value, route: String, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = model["searchHint"]
            .as_str()
            .map(|hint| cx.new(|cx| InputState::new(window, cx).placeholder(hint.to_owned())));
        let mut this = Self {
            domain: model["domain"].as_str().unwrap_or_default().to_owned(),
            model: Value::Null,
            route,
            selects: HashMap::new(),
            inputs: HashMap::new(),
            search,
            form: None,
            dialog: None,
            opening: false,
            opening_title: String::new(),
            finder: None,
            error: None,
            compact: false,
            page_title: String::new(),
            subscriptions: Vec::new(),
        };
        if let Some(search) = this.search.clone() {
            this.subscriptions
                .push(cx.subscribe(&search, |_, _, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Change) {
                        cx.notify();
                    }
                }));
        }
        this.update_model(model, window, cx);
        this
    }
    /// The page these sections fill: a heading shows only when there is more than one section, and never
    /// repeats the page's title.
    pub fn set_page(&mut self, title: String) {
        if self.page_title != title {
            self.page_title = title;
        }
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
            self.opening_title = button["label"].as_str().unwrap_or_default().to_owned();
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
        if let Some(err) = crate::error::StreamerzError::from_response(&value) {
            if matches!(operation, "save" | "json") {
                if let Some(form) = &self.form {
                    form.update(cx, |form, cx| {
                        form.pending = false;
                        form.error = Some(err);
                        cx.notify();
                    });
                }
            } else {
                self.error = Some(err);
            }
            cx.notify();
            return operation == "set";
        }
        self.error = None;
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
                let title = value["sections"][0]["title"]
                    .as_str()
                    .filter(|title| !title.is_empty())
                    .map_or_else(|| std::mem::take(&mut self.opening_title), str::to_owned);
                let nested = cx.new(|cx| Sections::new(value, format!("{}:dialog", self.route), window, cx));
                self.subscriptions
                    .push(cx.subscribe(&nested, |this, _, event: &Navigate, cx| {
                        this.dialog = None;
                        cx.emit(Navigate(event.0.clone()));
                    }));
                self.dialog = Some(nested.clone());
                let owner = cx.entity().downgrade();
                window.open_dialog(cx, move |dialog, window, cx| {
                    let owner = owner.clone();
                    let close = crate::controls::dialog::close_footer(cx);
                    let max_body = crate::controls::dialog::fit_height(180., 180., 680., window);
                    crate::controls::dialog::shell(dialog, title.clone(), 560., window, cx)
                        .child(crate::controls::dialog::body(
                            div()
                                .id("dialog-sections")
                                .w_full()
                                .max_h(px(max_body))
                                .overflow_y_scroll()
                                .pr(px(crate::theme::SPACE_XS))
                                .child(nested.clone()),
                        ))
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
                self.form = Some(form.clone());
                let owner = cx.entity().downgrade();
                window.open_dialog(cx, move |dialog, window, cx| {
                    crate::controls::dialog::shell(dialog, title.clone(), 560., window, cx)
                        .child(crate::controls::dialog::body(form.clone()))
                        .footer(form.read(cx).buttons_view())
                        .on_cancel({
                            let (form, owner) = (form.clone(), owner.clone());
                            move |_, _, cx| {
                                if form.read(cx).pending {
                                    return false;
                                }
                                let _ = owner.update(cx, |sections, cx| {
                                    sections.form = None;
                                    cx.notify();
                                });
                                true
                            }
                        })
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
                window.open_dialog(cx, move |dialog, window, cx| {
                    let owner = owner.clone();
                    crate::controls::dialog::shell(dialog, title.clone(), 560., window, cx)
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
                let icon_only = (fold && !login && spec["icon"].is_string() && spec["keepLabel"] != true)
                    || spec["icon_only"] == true
                    || spec["iconOnly"] == true;
                let mut widget = Button::new((
                    SharedString::from(format!("{key}-{}", spec["id"].as_str().unwrap_or_default())),
                    index,
                ))
                .small()
                .h(px(CONTROL_HEIGHT))
                .font_weight(FontWeight::NORMAL)
                .min_w(px(if icon_only {
                    CONTROL_HEIGHT
                } else if label.chars().count() > 5 {
                    120.
                } else {
                    90.
                }))
                .when(icon_only, |button| button.w(px(CONTROL_HEIGHT)))
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
                    .when(roomy, |line| line.px(px(SPACE_XS)))
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
                let mut line = div().flex().flex_wrap().items_center().gap_2().min_w_0().max_w_full();
                if let Some(select) = self.selects.get(key) {
                    line = line.child(
                        div()
                            .when(compact, |select| select.w_full())
                            .when(!compact, |select| select.w(px(if roomy { 200. } else { 230. })))
                            .child(
                                Select::new(select)
                                    .small()
                                    .icon(Icon::empty().path("icons/chevrons-up-down.svg").size(px(16.)))
                                    .h(px(CONTROL_HEIGHT)),
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
                            .min_h(px(CONTROL_HEIGHT))
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
        .p(px(SPACE_LG))
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
        .gap(px(SPACE_XS))
        .h(px(LINE_CAPTION + SPACE_SM))
        .px(px(SPACE_SM))
        .rounded(px((LINE_CAPTION + SPACE_SM) / 2.))
        .bg(cx.theme().muted)
        .text_size(px(FONT_CAPTION))
        .line_height(px(LINE_CAPTION))
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
            .gap(px(SPACE_SM))
            .h(px(LINE_SECTION_TITLE))
            .when_some(section["brand"].as_str(), |header, brand| {
                header.child(super::body::brand_logo(
                    SharedString::from(format!("card-brand-{index}")),
                    brand,
                    px(20.),
                    cx,
                ))
            })
            .child(
                div()
                    .font_weight(FontWeight::BOLD)
                    .text_size(px(FONT_SECTION_TITLE))
                    .line_height(px(LINE_SECTION_TITLE))
                    .text_color(theme::strong(cx))
                    .child(section["title"].as_str().unwrap_or_default().to_owned()),
            )
            .child(div().flex_1());
        if state.is_object() {
            header = header.child(
                div()
                    .id(("card-state", index))
                    .flex()
                    .items_center()
                    .gap(px(SPACE_XS))
                    .flex_shrink_0()
                    .text_size(px(FONT_CAPTION))
                    .line_height(px(LINE_CAPTION))
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
        let mut body = div().flex().flex_col().gap(px(SPACE_SM)).flex_1();
        if let Some(headline) = section["headline"].as_str() {
            let muted = section["headlineMuted"] == true;
            body = body.child(
                div()
                    .text_size(px(FONT_SECTION_TITLE))
                    .line_height(px(LINE_SECTION_TITLE))
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
                    .gap(px(SPACE_SM))
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
            let (size, height) = if muted {
                (FONT_CAPTION, LINE_CAPTION)
            } else {
                (FONT_BODY, LINE_BODY)
            };
            body = body.child(
                div()
                    .flex()
                    .items_start()
                    .gap(px(SPACE_SM))
                    .text_size(px(size))
                    .line_height(px(height))
                    .text_color(if muted {
                        crate::theme::quiet(cx)
                    } else {
                        theme.foreground
                    })
                    .when_some(line["icon"].as_str(), |line, icon| {
                        line.child(
                            div().h(px(height)).flex().items_center().flex_shrink_0().child(
                                Icon::empty()
                                    .path(format!("icons/{icon}.svg"))
                                    .size(px(14.))
                                    .text_color(super::body::icon_tone(cx)),
                            ),
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
            .gap(px(SPACE_SM))
            .pt(px(SPACE_MD))
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
                .h(px(CONTROL_HEIGHT))
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
                .gap(px(SPACE_MD))
                .child(header)
                .child(body)
                .child(footer),
            cx,
        )
        .into_any_element()
    }
}

/// The parts of a page, to set the gap above each one (docs/UI.md, "Page frame").
#[derive(Clone, Copy, PartialEq, Debug)]
enum Part {
    /// A message from the core, above everything.
    Error,
    /// A row of buttons or a search box under the guide.
    Toolbar,
    /// What the page is for, right under its header.
    Guide,
    /// A section or a grid of cards.
    Content,
    /// A quiet note under the content.
    Note,
}

/// The gap above `part` when `before` comes right above it: 12 under a guide and above a note, 8 under a
/// toolbar or a message, 16 between sections.
fn gap_above(before: Part, part: Part) -> f32 {
    match (before, part) {
        (_, Part::Note) | (Part::Guide, _) => SPACE_MD,
        (Part::Error | Part::Toolbar, _) => SPACE_SM,
        _ => SECTION_GAP,
    }
}

/// A row that is only a few words of text (an introduction), maybe with buttons beside it.
fn only_text(row: &Value) -> bool {
    row["about"].as_str().is_some_and(|about| !about.is_empty())
        && row["label"].as_str().is_none_or(str::is_empty)
        && row["value"].is_null()
        && row["status"].is_null()
}

/// Text on lines of its own, in `size` and `height`, quiet.
fn quiet_lines(text: &str, size: f32, height: f32, cx: &App) -> Div {
    div()
        .flex()
        .flex_col()
        .min_w_0()
        .text_size(px(size))
        .line_height(px(height))
        .text_color(theme::quiet(cx))
        .children(text.split('\n').map(|line| div().child(line.to_owned())))
}

/// `el` in `block`, `gap` px under what is there.
fn stack<P: ParentElement>(mut block: P, gap: f32, el: impl IntoElement) -> P {
    block.extend([div().mt(px(gap)).w_full().child(el).into_any_element()]);
    block
}

impl Render for Sections {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.compact = f32::from(window.viewport_size().width) < 700.;
        // Settings is laid out roomier than the other pages.
        let roomy = self.domain == "settings";
        let mut parts: Vec<(Part, AnyElement)> = Vec::new();
        if let Some(err) = &self.error {
            parts.push((Part::Error, super::notice::error(err, cx).into_any_element()));
        }
        let mut sections = self.model["sections"].as_array().cloned().unwrap_or_default();
        if let Some(search) = &self.search {
            parts.push((
                Part::Toolbar,
                Input::new(search)
                    .small()
                    .min_h(px(CONTROL_HEIGHT))
                    .prefix(Icon::empty().path("icons/search.svg").small())
                    .aria_label(self.model["searchHint"].as_str().unwrap_or_default().to_owned())
                    .into_any_element(),
            ));
            let query = search.read(cx).value().trim().to_lowercase();
            if !query.is_empty() {
                for section in &mut sections {
                    let title_matches = section["title"]
                        .as_str()
                        .is_some_and(|title| title.to_lowercase().contains(&query));
                    if !title_matches {
                        if let Some(rows) = section["rows"].as_array_mut() {
                            rows.retain(|row| {
                                ["label", "about", "tip"].iter().any(|key| {
                                    row[key]
                                        .as_str()
                                        .is_some_and(|text| text.to_lowercase().contains(&query))
                                })
                            });
                        }
                    }
                }
                sections.retain(|section| section["rows"].as_array().is_some_and(|rows| !rows.is_empty()));
                if sections.is_empty() {
                    let msg = crate::i18n::tr("lists.no_matches");
                    parts.push((
                        Part::Content,
                        div()
                            .id("sections-no-matches")
                            .test_support()
                            .aria_label(msg)
                            .child(super::notice::info(msg, cx))
                            .into_any_element(),
                    ));
                }
            }
        }
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
            let mut grid = div().flex().flex_col().gap(px(SPACE_MD)).w_full();
            let mut drained: Vec<AnyElement> = std::mem::take(cards);
            while !drained.is_empty() {
                let rest = drained.split_off(drained.len().min(columns));
                let row_cards = std::mem::replace(&mut drained, rest);
                // A row short of cards keeps their width: empty places beside them.
                let short = columns - row_cards.len();
                let row = div()
                    .flex()
                    .gap(px(SPACE_MD))
                    .w_full()
                    .children(
                        row_cards
                            .into_iter()
                            .map(|card| div().flex().flex_1().min_w_0().child(card)),
                    )
                    .children((0..short).map(|_| div().flex_1()));
                grid = grid.child(row);
            }
            grid.into_any_element()
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
                parts.push((Part::Content, grid(&mut cards, &mut wide)));
            }
            let title = section["title"].as_str().unwrap_or_default().to_owned();
            let titled = !title.is_empty();
            // A platform's section: its brand and state on the heading's line.
            let plat = section["inline"] == true && section["brand"].is_string();
            // A heading shows only on a page with more than one section, and never repeats the page's
            // title; a platform's (with its state) always does.
            let carries_state = section["inline"] == true && section["rows"][0]["status"].is_object();
            let framed = !self.page_title.is_empty();
            let heading_shown = !framed
                || plat
                || carries_state
                || (sections.len() > 1 && title.to_lowercase() != self.page_title.to_lowercase());
            // An inline section puts its first row's state and button on the heading's line.
            let inline = section["inline"] == true;
            let mut rows = section["rows"].as_array().map_or(&[][..], Vec::as_slice);
            // The page's introduction goes under its header as the guide; its buttons, as a toolbar.
            if section_index == 0 && !titled && !boxed && rows.first().is_some_and(only_text) {
                let lead = &rows[0];
                parts.push((
                    Part::Guide,
                    quiet_lines(lead["about"].as_str().unwrap_or_default(), FONT_BODY, LINE_BODY, cx)
                        .into_any_element(),
                ));
                if lead["control"].is_object() {
                    let key = format!("~{section_index}.0");
                    parts.push((
                        Part::Toolbar,
                        div().flex().child(self.control(lead, &key, cx)).into_any_element(),
                    ));
                }
                rows = &rows[1..];
                if rows.is_empty() {
                    continue;
                }
            }
            // A note under the cards stays quiet and small.
            if after_cards
                && !titled
                && !boxed
                && rows.len() == 1
                && only_text(&rows[0])
                && !rows[0]["control"].is_object()
            {
                parts.push((
                    Part::Note,
                    quiet_lines(
                        rows[0]["about"].as_str().unwrap_or_default(),
                        FONT_CAPTION,
                        LINE_CAPTION,
                        cx,
                    )
                    .into_any_element(),
                ));
                continue;
            }
            let mut block = div().flex().flex_col().w_full();
            // Whether anything is in the block yet, and whether only its heading is.
            let (mut placed, mut after_heading) = (false, false);
            let mut heading = div()
                .flex()
                .items_center()
                .gap(px(SPACE_SM))
                .h(px(LINE_SECTION_TITLE))
                .font_weight(FontWeight::BOLD)
                .text_size(px(FONT_SECTION_TITLE))
                .line_height(px(LINE_SECTION_TITLE))
                .text_color(theme::strong(cx));
            if let Some(brand) = section["brand"].as_str() {
                heading = heading.child(
                    img(format!("brands/{brand}.svg"))
                        .id(SharedString::from(format!("brand-{brand}")))
                        .size(px(LINE_SECTION_TITLE))
                        .flex_shrink_0(),
                );
            } else if let Some(icon) = section["icon"].as_str().filter(|icon| !icon.is_empty()) {
                heading = heading.child(Icon::empty().path(format!("icons/{icon}.svg")).small());
            }
            if !title.is_empty() && heading_shown && !self.route.ends_with(":dialog") {
                if let (true, Some(first)) = (inline, rows.first().filter(|row| row["status"].is_object())) {
                    let (icon, color) =
                        super::health::appearance(first["status"]["health"].as_str().unwrap_or_default(), cx);
                    heading = heading
                        .h(px(CONTROL_HEIGHT))
                        .child(
                            div()
                                .min_w(px(if plat { PLATFORM_NAME_WIDTH } else { LABEL_WIDTH }))
                                .flex_shrink_0()
                                .child(title),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(SPACE_SM))
                                .flex_1()
                                .font_weight(FontWeight::NORMAL)
                                .text_size(px(FONT_BODY))
                                .line_height(px(LINE_BODY))
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
                                .text_size(px(FONT_BODY))
                                .line_height(px(LINE_BODY))
                                .child(self.control(first, &key, cx)),
                        );
                    }
                    rows = &rows[1..];
                } else {
                    heading = heading.child(title);
                }
                block = block.child(heading);
                (placed, after_heading) = (true, true);
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
                            .gap(px(SPACE_SM))
                            .flex_1()
                            .min_w_0()
                            .child(div().flex_shrink_0().child(label))
                            .child(
                                div().flex_1().min_w_0().child(
                                    Input::new(input)
                                        .small()
                                        .min_h(px(CONTROL_HEIGHT))
                                        .aria_label(row["label"].as_str().unwrap_or_default().to_owned()),
                                ),
                            )
                            .into_any_element(),
                    );
                    continue;
                }
                if !flowing.is_empty() {
                    let line = div()
                        .flex()
                        .gap(px(SPACE_MD))
                        .w_full()
                        .min_h(px(CONTROL_HEIGHT))
                        .children(flowing.drain(..));
                    block = stack(block, if placed { SPACE_SM } else { 0. }, line);
                    (placed, after_heading) = (true, false);
                }
                // An introduction alone in its row: a note under its heading sits close to it.
                let about_only = !about.is_empty()
                    && (!titled || inline)
                    && label.is_empty()
                    && row["value"].is_null()
                    && row["status"].is_null()
                    && !row["control"].is_object();
                let logo = row["logo"] == true;
                let mut text = div().flex().flex_col().flex_1().min_w_0();
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
                            .gap(px(SPACE_MD))
                            .child(
                                div()
                                    .w(px(LABEL_WIDTH))
                                    .flex_shrink_0()
                                    .text_right()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(label),
                            )
                            .child(
                                Button::new((SharedString::from(format!("{key}-pick")), 0usize))
                                    .small()
                                    .text_size(px(FONT_BODY))
                                    .h(px(CONTROL_HEIGHT))
                                    .w(px(220.))
                                    .when(face.is_none(), |button| {
                                        button.icon(Icon::empty().path("icons/search.svg")).label(value.clone())
                                    })
                                    .when_some(face, |button, face| {
                                        button.child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap(px(SPACE_SM))
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
                                    .text_size(px(FONT_CAPTION))
                                    .line_height(px(LINE_CAPTION))
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
                            .gap(px(SPACE_MD))
                            .child(
                                div()
                                    .w(px(LABEL_WIDTH))
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
                                    .gap(px(SPACE_SM))
                                    .when(
                                        plat && (key.ends_with(".account") || key.ends_with(".bot"))
                                            && row["picture"].is_null(),
                                        // No picture (yet): a person in its place; none when nobody's logged in.
                                        |line| {
                                            line.child(
                                                div()
                                                    .size(px(PICTURE))
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
                                                .size(px(if plat { PICTURE } else { 20. }))
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
                            .gap(px(SPACE_MD))
                            .child(
                                div()
                                    .w(px(LABEL_WIDTH))
                                    .flex_shrink_0()
                                    .text_right()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(label),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(SPACE_SM))
                                    .flex_1()
                                    .min_w_0()
                                    .when(row["picture"].is_null(), |line| line.pl(px(PICTURE + SPACE_SM)))
                                    .when_some(row["picture"].as_str(), |line, file| {
                                        line.child(
                                            img(std::path::PathBuf::from(file))
                                                .id(SharedString::from(format!("picture-{key}")))
                                                .size(px(PICTURE))
                                                .rounded_full()
                                                .flex_shrink_0()
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
                            .gap(px(SPACE_SM))
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
                            .when(logo, |label| {
                                label
                                    .font_weight(FontWeight::BOLD)
                                    .text_size(px(FONT_SECTION_TITLE))
                                    .line_height(px(LINE_SECTION_TITLE))
                            })
                            .child(label),
                    );
                }
                if !about.is_empty() && !channel {
                    text = text.child(
                        quiet_lines(&about, FONT_CAPTION, LINE_CAPTION, cx)
                            .when(!about_only, |about| about.mt(px(SPACE_XS))),
                    );
                }
                let mut line = div()
                    .flex()
                    .items_center()
                    .gap(px(if roomy { SPACE_LG } else { SPACE_MD }))
                    .min_h(px(CONTROL_HEIGHT))
                    .w_full();
                // Narrow: a select or buttons go under the text, a switch stays beside it.
                let stacked = roomy && self.compact && row["control"].is_object() && row["control"]["type"] != "switch";
                if stacked {
                    line = line.flex_col().items_stretch().gap(px(SPACE_SM));
                }
                if roomy && logo {
                    text = text.self_start();
                }
                if logo {
                    line = line
                        .min_h(px(64.))
                        .child(img("brands/streamerz.svg").id("app-logo").size(px(56.)).flex_shrink_0());
                } else if let Some(brand) = row["brand"].as_str().filter(|b| !b.is_empty()) {
                    line = line.child(super::body::brand_logo(
                        SharedString::from(format!("logo-{key}")),
                        brand,
                        px(CONTROL_HEIGHT),
                        cx,
                    ));
                } else if let Some(icon) = row["icon"].as_str().filter(|i| !i.is_empty()) {
                    line = line.child(super::body::brand_logo(
                        SharedString::from(format!("logo-{key}")),
                        icon,
                        px(CONTROL_HEIGHT),
                        cx,
                    ));
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
                let gap = match (placed, after_heading && about_only) {
                    (false, _) => 0.,
                    (true, true) => SPACE_XS,
                    (true, false) => SPACE_SM,
                };
                block = stack(block, gap, line);
                (placed, after_heading) = (true, false);
            }
            if !flowing.is_empty() {
                let line = div()
                    .flex()
                    .gap(px(SPACE_MD))
                    .w_full()
                    .min_h(px(CONTROL_HEIGHT))
                    .children(flowing.drain(..));
                block = stack(block, if placed { SPACE_SM } else { 0. }, line);
            }
            if boxed {
                wide = true;
                cards.push(card_frame(section_index, block, cx).into_any_element());
            } else {
                parts.push((Part::Content, block.into_any_element()));
            }
        }
        if !cards.is_empty() {
            parts.push((Part::Content, grid(&mut cards, &mut wide)));
        }
        let mut page = div()
            .id(SharedString::from(format!("sections-{}", self.domain)))
            .flex()
            .flex_col()
            .w_full();
        let mut before = None;
        for (part, el) in parts {
            let gap = before.map_or(0., |before| gap_above(before, part));
            page = stack(page, gap, el);
            before = Some(part);
        }
        page
    }
}

#[cfg(test)]
mod tests {
    use super::{gap_above, only_text, Part};
    use serde_json::json;

    #[test]
    fn gaps_follow_the_page_pattern() {
        // A guide is 12 from what follows, a toolbar 8, sections 16 apart, a note 12 under its content.
        assert_eq!(gap_above(Part::Guide, Part::Content), 12.);
        assert_eq!(gap_above(Part::Guide, Part::Toolbar), 12.);
        assert_eq!(gap_above(Part::Toolbar, Part::Content), 8.);
        assert_eq!(gap_above(Part::Error, Part::Content), 8.);
        assert_eq!(gap_above(Part::Content, Part::Content), 16.);
        assert_eq!(gap_above(Part::Content, Part::Note), 12.);
    }

    #[test]
    fn an_introduction_is_a_row_of_only_text() {
        assert!(only_text(&json!({"about": "Pick whose chat to read."})));
        assert!(only_text(&json!({"about": "Hint", "control": {"type": "buttons"}})));
        assert!(!only_text(&json!({"about": "Hint", "label": "Theme"})));
        assert!(!only_text(&json!({"about": "Hint", "value": "x"})));
        assert!(!only_text(&json!({"label": "Theme"})));
    }

    #[cfg(feature = "test-support")]
    mod ui {
        use super::super::Sections;
        use gpui_kit::test::{TestEventExt, TestQueryExt, TestWindowExt};
        use gpui_kit::{
            div, px, size, AppContext, Bounds, Entity, IntoElement, ParentElement, Render, Styled, TestAppContext,
            Window, WindowBounds, WindowOptions,
        };
        use serde_json::json;

        struct SectionsHost(Option<Entity<Sections>>);
        impl Render for SectionsHost {
            fn render(&mut self, _: &mut Window, _: &mut gpui_kit::Context<Self>) -> impl IntoElement {
                div().size_full().p_4().children(self.0.clone())
            }
        }

        #[gpui_kit::test]
        fn search_filters_sections_by_title_label_about_tip_and_shows_no_matches(cx: &mut TestAppContext) {
            cx.update(|cx| {
                gpui_kit::init(cx);
                crate::theme::apply(true, cx);
            });
            let model = json!({
                "domain": "settings",
                "searchHint": "Search settings",
                "sections": [
                    {
                        "title": "Appearance",
                        "rows": [
                            {"key": "theme", "label": "Theme", "about": "Dark or light", "tip": "Restart not needed"}
                        ]
                    },
                    {
                        "title": "Audio",
                        "rows": [
                            {"key": "device", "label": "Microphone", "about": "Input device", "tip": "USB audio supported"}
                        ]
                    }
                ]
            });
            let host_slot: std::rc::Rc<std::cell::RefCell<Option<Entity<SectionsHost>>>> =
                std::rc::Rc::new(std::cell::RefCell::new(None));
            let s = host_slot.clone();
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
                    move |_, cx| {
                        let host = cx.new(|_| SectionsHost(None));
                        *s.borrow_mut() = Some(host.clone());
                        host
                    },
                )
                .expect("open test window");
                window.downcast::<gpui_kit::base::Root>().expect("root window")
            });
            let host = host_slot.borrow().clone().expect("host created");
            let sections = cx
                .update_window(window.into(), |_, window, cx| {
                    let sections = cx.new(|cx| Sections::new(model, "settings".into(), window, cx));
                    host.update(cx, |host, cx| {
                        host.0 = Some(sections.clone());
                        cx.notify();
                    });
                    sections
                })
                .unwrap();
            let no_matches = crate::i18n::tr("lists.no_matches");

            let type_query = |text: &str, sections: &Entity<Sections>, window: &mut Window, cx: &mut gpui_kit::App| {
                let search = sections.read(cx).search.clone().expect("search input");
                search.update(cx, |input, cx| input.focus(window, cx));
                window.key_down("ctrl-a", false, cx);
                window.key_up("ctrl-a", cx);
                if text.is_empty() {
                    window.key_down("backspace", false, cx);
                    window.key_up("backspace", cx);
                } else {
                    window.input(text, cx);
                }
            };

            cx.update_window(window.into(), |_, window, cx| {
                window.render_frame(cx);
                assert!(window.try_find_by_label(no_matches).is_none());
                // Search by row tip keeps Audio (no no-matches notice).
                type_query("USB audio", &sections, window, cx);
            })
            .unwrap();
            cx.run_until_parked();
            cx.update_window(window.into(), |_, window, cx| {
                window.render_frame(cx);
                assert!(window.try_find_by_label(no_matches).is_none());

                // Unmatched query renders the shared no-matches notice.
                type_query("zzz-nomatch", &sections, window, cx);
            })
            .unwrap();
            cx.run_until_parked();
            cx.update_window(window.into(), |_, window, cx| {
                window.render_frame(cx);
                assert!(window.try_find_by_label(no_matches).is_some());

                // Clearing search restores sections and hides no-matches notice.
                type_query("", &sections, window, cx);
            })
            .unwrap();
            cx.run_until_parked();
            cx.update_window(window.into(), |_, window, cx| {
                window.render_frame(cx);
                assert!(window.try_find_by_label(no_matches).is_none());
            })
            .unwrap();
        }
    }
}
