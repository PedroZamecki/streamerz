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
                    if dialog.read(cx).domain == value["domain"].as_str().unwrap_or_default() {
                        dialog.update(cx, |dialog, cx| dialog.update_model(value, window, cx));
                        return false;
                    }
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
        // A narrow window folds the long buttons of an account's lines to their icon (1.6.0).
        let fold = !roomy && self.compact;
        let buttons = |list: &[Value], cx: &mut Context<Self>| {
            let mut line = div().flex().gap_2().flex_shrink_0().when(compact, |line| line.w_full());
            for (index, button) in list.iter().enumerate() {
                let spec = button.clone();
                let label = button["label"].as_str().unwrap_or_default().to_owned();
                let login = spec["id"]
                    .as_str()
                    .is_some_and(|id| id.ends_with("login") || id.ends_with("logout"));
                // A gear and the like are always just their icon (the label is its tooltip).
                let icon_only = (fold && !login && spec["icon"].is_string()) || spec["icon_only"] == true;
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
                .when(roomy && !compact, |button| {
                    button.w(px(if list.len() == 1 { 150. } else { 114. }))
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
                    .when(
                        roomy && (control["button"].is_object() || key == "memory_saver"),
                        |line| line.w(px(180.)).justify_start(),
                    );
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

impl Render for Sections {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.compact = f32::from(window.viewport_size().width) < 700.;
        // Settings is laid out roomier than the other pages, as in 1.6.0.
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
        for (section_index, section) in sections.iter().enumerate() {
            let title = section["title"].as_str().unwrap_or_default().to_owned();
            let titled = !title.is_empty();
            // A platform's section (brand and inline state): 1.6.0's account layout.
            let plat = section["inline"] == true && section["brand"].is_string();
            let mut heading = div()
                .flex()
                .items_center()
                .gap_2()
                .h(px(if roomy { 34. } else { 24. }))
                .font_weight(FontWeight::BOLD)
                .text_size(px(15.))
                .text_color(crate::theme::strong(cx))
                .when(plat, |heading| {
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
                                .w(px(if plat { 83. } else { 110. }))
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
                                .child(
                                    div()
                                        .min_w_0()
                                        .truncate()
                                        .child(first["status"]["text"].as_str().unwrap_or_default().to_owned()),
                                ),
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
                page = page.child(heading);
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
                    page = page.child(div().flex().gap_3().w_full().min_h(px(30.)).children(flowing.drain(..)));
                }
                // An introduction alone in its row (1.6.0 gives it a two line box).
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
                // A platform's channel is one wide button (1.6.0): it shows the channel and opens the picker.
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
                            // Whose channel it is, beside the button (1.6.0).
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(about.clone()),
                            ),
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
                                        |line| line.child(div().size(px(24.)).flex_shrink_0()),
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
                                    .child(
                                        div()
                                            .min_w_0()
                                            .truncate()
                                            .when(muted, |v| v.text_color(cx.theme().muted_foreground))
                                            .child(value),
                                    ),
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
                                    .child(
                                        div()
                                            .min_w_0()
                                            .truncate()
                                            .child(status["text"].as_str().unwrap_or_default().to_owned()),
                                    ),
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
                            .when(about_only && !roomy && section_index > 0, |about| {
                                about.mt(px(4.)).min_h(px(141.)).line_height(px(17.))
                            })
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
                let stacked = roomy
                    && self.compact
                    && row["control"].is_object()
                    && (row["control"]["type"] != "switch" || key == "memory_saver");
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
                line = line.child(text);
                if row["control"].is_object() && !channel {
                    line = line.child(self.control(row, &key, cx));
                }
                page = page.child(line);
            }
            if !flowing.is_empty() {
                page = page.child(div().flex().gap_3().w_full().min_h(px(30.)).children(flowing.drain(..)));
            }
        }
        page.pb_4()
    }
}
