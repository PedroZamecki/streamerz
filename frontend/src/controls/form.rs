//! Shared schema-driven forms. Validation and type conversion stay in the core.
use super::gallery::Gallery;
use crate::theme::{CONTROL_HEIGHT, FONT_BODY, FONT_CAPTION, LINE_BODY, LINE_CAPTION, SPACE_MD, SPACE_SM, SPACE_XS};
use crate::{i18n::tr, send};
use gpui_kit::component::{
    button::{Button, ButtonVariants},
    checkbox::Checkbox,
    color_picker::{ColorPicker, ColorPickerEvent, ColorPickerState},
    input::{Input, InputEvent, InputState, Textarea, TextareaState},
    menu::{DropdownMenu, PopupMenuItem},
    select::{SearchableVec, Select, SelectEvent, SelectItem, SelectState},
    slider::{Slider, SliderEvent, SliderState},
    ActiveTheme, Disableable, IndexPath, Sizable,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use serde_json::{json, Value};

#[derive(Clone, Debug, PartialEq)]
struct ChoiceItem {
    index: usize,
    label: String,
}

impl SelectItem for ChoiceItem {
    type Value = usize;

    fn title(&self) -> SharedString {
        self.label.clone().into()
    }

    fn value(&self) -> &Self::Value {
        &self.index
    }
}

enum Field {
    Text(Entity<InputState>),
    Multi(Entity<TextareaState>),
    Bool(bool),
    Pick(Entity<SelectState<Vec<String>>>),
    /// A field that names something else: its ids and names come from the core. With an input it
    /// also takes typed text; without one it takes only an id of the list (or none).
    Choice {
        select: Entity<SelectState<SearchableVec<ChoiceItem>>>,
        input: Option<Entity<InputState>>,
        ids: Vec<String>,
        /// The list ends with "Other": the box to type a new one shows only when it is picked.
        other: bool,
    },
    /// Lines of text with a list to add from: picking a name adds it as a line.
    Lines {
        area: Entity<TextareaState>,
        select: Entity<SelectState<SearchableVec<ChoiceItem>>>,
    },
    /// A field picked by its picture (TikTok's gifts): see `gallery.rs`.
    Gallery(Entity<Gallery>),
    /// A colour, picked from a palette or mixed with sliders: `#rrggbb`, or `#rrggbbaa` when see-through.
    Color(Entity<ColorPickerState>),
    /// A number dragged between two ends.
    Slider(Entity<SliderState>),
    /// A font of this computer, found by typing: the list starts with the system's (an empty name).
    Font {
        select: Entity<SelectState<SearchableVec<String>>>,
        /// What the first line of the list says (the system's font): it stands for no name.
        system: String,
    },
    /// A few boxes to tick on one line (the platforms): its value is the list of the ticked ones' ids.
    Checks {
        ids: Vec<String>,
        labels: Vec<String>,
        brands: Vec<Option<String>>,
        on: Vec<bool>,
    },
}

fn finite_f32(value: Option<f64>, default: f32) -> f32 {
    match value {
        Some(n) if n.is_finite() => {
            let v = n as f32;
            if v.is_finite() {
                v
            } else {
                default
            }
        }
        _ => default,
    }
}

/// Normalizes slider bounds, step, and initial value so `SliderState` never panics on `min > 100`,
/// `min > max`, non-positive steps, or out-of-range/non-finite numbers.
pub(crate) fn sanitize_slider(
    min: Option<f64>,
    max: Option<f64>,
    step: Option<f64>,
    value: Option<f64>,
) -> (f32, f32, f32, f32) {
    let lo = finite_f32(min, 0.0);
    let hi = finite_f32(max, 100.0).max(lo);
    let raw_step = finite_f32(step, 1.0);
    let step = if raw_step > 0.0 { raw_step } else { 1.0 };
    let val = finite_f32(value, lo).clamp(lo, hi);
    (lo, hi, step, val)
}

/// A colour as the core keeps it: `#rrggbb`, with `aa` only when it is see-through.
fn hex(color: Hsla) -> String {
    let c = Rgba::from(color);
    let byte = |part: f32| (part.clamp(0., 1.) * 255.).round() as u8;
    let (r, g, b, a) = (byte(c.r), byte(c.g), byte(c.b), byte(c.a));
    if a == 255 {
        format!("#{r:02x}{g:02x}{b:02x}")
    } else {
        format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
    }
}

/// `#rrggbb` or `#rrggbbaa` as a colour.
fn color_of(text: &str) -> Option<Hsla> {
    let hex = text.trim().strip_prefix('#')?;
    if !matches!(hex.len(), 6 | 8) || !hex.is_ascii() {
        return None;
    }
    let part = |at: usize| {
        u8::from_str_radix(&hex[at..at + 2], 16)
            .ok()
            .map(|v| f32::from(v) / 255.)
    };
    let a = if hex.len() == 8 { part(6)? } else { 1. };
    Some(Hsla::from(Rgba {
        r: part(0)?,
        g: part(2)?,
        b: part(4)?,
        a,
    }))
}

/// A slider's number without the noise of its 32 bits (0.85, not 0.8500000238).
fn tidy(value: f32) -> f64 {
    (f64::from(value) * 1e4).round() / 1e4
}
pub struct Form {
    pub model: Value,
    fields: Vec<Field>,
    route: String,
    json_editor: Option<Entity<TextareaState>>,
    pub error: Option<crate::error::StreamerzError>,
    pub pending: bool,
    buttons_view: Entity<FormButtons>,
    /// A file picked with Browse, for its field (set into the box on the next draw, which has the window).
    picked: Option<(usize, String)>,
    autofocus: bool,
    /// Section headings toggle their fields without changing form values.
    section_toggles: std::collections::HashSet<usize>,
    _subscriptions: Vec<Subscription>,
}
impl Form {
    pub fn new(model: Value, route: String, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut subscriptions = Vec::new();
        let fields = model["fields"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
            .map(|(index, field)| {
                let value = &field["value"];
                let kind = field["widget"]["kind"].as_str().unwrap_or_default();
                if field["gallery"].is_object() {
                    let route = route.replace(":save", &format!(":gallery:{index}"));
                    let gallery = cx.new(|cx| Gallery::new(field, route, window, cx));
                    subscriptions.push(cx.observe(&gallery, |_, _, cx| cx.notify()));
                    return Field::Gallery(gallery);
                }
                if let Some(choices) = field["choices"].as_array() {
                    let strict = field["strict"] == true;
                    let current = value.as_str().unwrap_or_default();
                    let mut ids: Vec<String> = Vec::new();
                    let mut names: Vec<String> = Vec::new();
                    if strict {
                        ids.push(String::new());
                        names.push(tr("forms.none").to_owned());
                    }
                    for choice in choices {
                        ids.push(choice["id"].as_str().unwrap_or_default().to_owned());
                        names.push(choice["label"].as_str().unwrap_or_default().to_owned());
                    }
                    let other = !strict && field["other"] == true;
                    if other {
                        ids.push(String::new());
                        names.push(tr("forms.other_type").to_owned());
                    }
                    // A stored id the list no longer has stays selectable, as itself.
                    if strict && !ids.iter().any(|id| id == current) {
                        ids.push(current.to_owned());
                        names.push(current.to_owned());
                    }
                    let selected = ids
                        .iter()
                        .position(|id| id == current && !(other && id.is_empty()))
                        .or((other && !current.is_empty() && !ids.iter().any(|id| id == current))
                            .then(|| ids.len() - 1))
                        .map(IndexPath::new);
                    // A list that also takes typed text (TikTok's gifts, OBS scenes) or a long one can be searched.
                    let searchable = !strict || names.len() > 8;
                    let items = SearchableVec::new(
                        names
                            .iter()
                            .enumerate()
                            .map(|(index, label)| ChoiceItem {
                                index,
                                label: label.clone(),
                            })
                            .collect::<Vec<_>>(),
                    );
                    let select = cx.new(|cx| SelectState::new(items, selected, window, cx).searchable(searchable));
                    let hint = field["placeholder"].as_str().unwrap_or_default().to_owned();
                    let input = (!strict).then(|| {
                        cx.new(|cx| {
                            InputState::new(window, cx)
                                .default_value(current.to_owned())
                                .placeholder(hint)
                        })
                    });
                    if let Some(input) = input.clone() {
                        // A typed id picks its name in the list (or none when the list doesn't have it).
                        let (chosen, list) = (ids.clone(), select.clone());
                        subscriptions.push(cx.subscribe_in(
                            &input,
                            window,
                            move |_, input, event: &InputEvent, window, cx| {
                                if !matches!(event, InputEvent::Change) {
                                    return;
                                }
                                let typed = input.read(cx).value().trim().to_owned();
                                let at = chosen
                                    .iter()
                                    .position(|id| *id == typed && !(other && id.is_empty()))
                                    .or_else(|| (other && !chosen.is_empty()).then(|| chosen.len() - 1));
                                if list.read(cx).selected_value().copied() != at {
                                    list.update(cx, |list, cx| match at {
                                        Some(at) => list.set_selected_value(&at, window, cx),
                                        None => list.set_selected_index(None, window, cx),
                                    });
                                }
                            },
                        ));
                    }
                    if let Some(input) = input.clone() {
                        let chosen = ids.clone();
                        subscriptions.push(cx.subscribe_in(
                            &select,
                            window,
                            move |_, _, event: &SelectEvent<SearchableVec<ChoiceItem>>, window, cx| {
                                if let SelectEvent::Confirm(Some(idx)) = event {
                                    if let Some(id) = chosen.get(*idx).cloned() {
                                        input.update(cx, |input, cx| input.set_value(id, window, cx));
                                    }
                                    cx.notify();
                                }
                            },
                        ));
                    } else {
                        subscriptions.push(
                            cx.subscribe(&select, |_, _, _: &SelectEvent<SearchableVec<ChoiceItem>>, cx| {
                                cx.notify()
                            }),
                        );
                    }
                    let _ = index;
                    return Field::Choice {
                        select,
                        input,
                        ids,
                        other,
                    };
                }
                match kind {
                    "font" => {
                        let mut names = cx.text_system().all_font_names();
                        names.sort_by_key(|name| name.to_lowercase());
                        names.dedup();
                        // A dot at the start marks a font the system keeps for itself.
                        names.retain(|name| !name.starts_with('.'));
                        let chosen = value.as_str().unwrap_or_default();
                        // One typed by hand before (in the JSON) that is not installed stays in the list.
                        if !chosen.is_empty() && !names.iter().any(|name| name == chosen) {
                            names.insert(0, chosen.to_owned());
                        }
                        let system = tr("forms.font.system").to_owned();
                        let at = names.iter().position(|name| name == chosen).map_or(0, |at| at + 1);
                        names.insert(0, system.clone());
                        // A list that narrows as its name is typed: there are hundreds of fonts.
                        let select = cx.new(|cx| {
                            SelectState::new(SearchableVec::new(names), Some(IndexPath::new(at)), window, cx)
                                .searchable(true)
                        });
                        subscriptions.push(
                            cx.subscribe(&select, |_, _, _: &SelectEvent<SearchableVec<String>>, cx| cx.notify()),
                        );
                        Field::Font { select, system }
                    }
                    "color" => {
                        let color = value.as_str().and_then(color_of);
                        let state = cx.new(|cx| {
                            let state = ColorPickerState::new(window, cx);
                            match color {
                                Some(color) => state.default_value(color),
                                None => state,
                            }
                        });
                        subscriptions.push(cx.subscribe(&state, |_, _, _: &ColorPickerEvent, cx| cx.notify()));
                        Field::Color(state)
                    }
                    "slider" => {
                        let (lo, hi, step, val) = sanitize_slider(
                            field["widget"]["min"].as_f64(),
                            field["widget"]["max"].as_f64(),
                            field["widget"]["step"].as_f64(),
                            value.as_f64(),
                        );
                        let state = cx.new(|_| {
                            SliderState::new()
                                .max(hi.max(100.))
                                .min(lo)
                                .max(hi)
                                .step(step)
                                .default_value(val)
                        });
                        subscriptions.push(cx.subscribe(&state, |_, _, _: &SliderEvent, cx| cx.notify()));
                        Field::Slider(state)
                    }
                    "bool" | "bit" => Field::Bool(value.as_bool().unwrap_or(false)),
                    "checks" => {
                        let mut ids = Vec::new();
                        let mut labels = Vec::new();
                        let mut brands = Vec::new();
                        for option in field["widget"]["options"].as_array().into_iter().flatten() {
                            if let Some(id) = option["id"].as_str() {
                                ids.push(id.to_owned());
                                labels.push(option["label"].as_str().unwrap_or_default().to_owned());
                                brands.push(option["brand"].as_str().map(str::to_owned));
                            }
                        }
                        let ticked: Vec<&str> = value
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(Value::as_str)
                            .collect();
                        let on = ids.iter().map(|id| ticked.contains(&id.as_str())).collect();
                        Field::Checks {
                            ids,
                            labels,
                            brands,
                            on,
                        }
                    }
                    "pick" => {
                        let options = field["widget"]["options"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(Value::as_str)
                            .map(str::to_owned)
                            .collect::<Vec<_>>();
                        let selected = value
                            .as_u64()
                            .and_then(|value| usize::try_from(value).ok())
                            .filter(|index| *index < options.len())
                            .map(IndexPath::new);
                        let state = cx.new(|cx| SelectState::new(options, selected, window, cx));
                        // Another field may show or hide by what is picked here.
                        subscriptions.push(cx.subscribe(&state, |_, _, _: &SelectEvent<Vec<String>>, cx| cx.notify()));
                        Field::Pick(state)
                    }
                    "multi" | "lines" | "map" => {
                        let hint = field["placeholder"].as_str().unwrap_or_default().to_owned();
                        let area = cx.new(|cx| {
                            TextareaState::new(window, cx)
                                .default_value(value.as_str().unwrap_or_default().to_owned())
                                .placeholder(hint)
                        });
                        let names: Vec<String> = field["suggest"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(|pick| pick["label"].as_str().map(str::to_owned))
                            .collect();
                        if names.is_empty() {
                            return Field::Multi(area);
                        }
                        let items = SearchableVec::new(
                            names
                                .iter()
                                .enumerate()
                                .map(|(index, label)| ChoiceItem {
                                    index,
                                    label: label.clone(),
                                })
                                .collect::<Vec<_>>(),
                        );
                        let select = cx.new(|cx| SelectState::new(items, None, window, cx).searchable(true));
                        let (target, list) = (area.clone(), names);
                        subscriptions.push(cx.subscribe_in(
                            &select,
                            window,
                            move |_, select, event: &SelectEvent<SearchableVec<ChoiceItem>>, window, cx| {
                                let SelectEvent::Confirm(Some(idx)) = event else { return };
                                let Some(name) = list.get(*idx).cloned() else {
                                    return;
                                };
                                let text = target.read(cx).value().to_string();
                                if !text.lines().any(|line| line.trim().eq_ignore_ascii_case(&name)) {
                                    let joined = if text.trim().is_empty() {
                                        name
                                    } else {
                                        format!("{}\n{name}", text.trim_end())
                                    };
                                    target.update(cx, |area, cx| area.set_value(joined, window, cx));
                                }
                                select.update(cx, |select, cx| select.set_selected_index(None, window, cx));
                            },
                        ));
                        Field::Lines { area, select }
                    }
                    _ => {
                        // A hint in the empty box: an example, or where the value goes.
                        let hint = field["placeholder"].as_str().unwrap_or_default().to_owned();
                        let input = cx.new(|cx| {
                            InputState::new(window, cx)
                                .default_value(value.as_str().unwrap_or_default().to_owned())
                                .masked(kind == "secret")
                                .placeholder(hint)
                        });
                        let browse_image = field["browse"] == "image";
                        subscriptions.push(cx.subscribe(&input, move |form, _, event: &InputEvent, cx| {
                            match event {
                                // A picked image shows as it's typed.
                                InputEvent::Change if browse_image => cx.notify(),
                                InputEvent::PressEnter { .. } if !form.pending => form.submit(false, cx),
                                _ => {}
                            }
                        }));
                        Field::Text(input)
                    }
                }
            })
            .collect();
        // A type that has no form opens as JSON.
        let json_editor = (model["jsonOnly"] == true).then(|| {
            cx.new(|cx| {
                TextareaState::new(window, cx).default_value(model["jsonText"].as_str().unwrap_or_default().to_owned())
            })
        });
        Self {
            model,
            fields,
            route,
            json_editor,
            error: None,
            pending: false,
            buttons_view: {
                let weak = cx.entity().downgrade();
                cx.new(|_| FormButtons(weak))
            },
            picked: None,
            autofocus: true,
            section_toggles: std::collections::HashSet::new(),
            _subscriptions: subscriptions,
        }
    }
    pub fn show_json(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.json_editor = Some(cx.new(|cx| TextareaState::new(window, cx).default_value(text.to_owned())));
        self.autofocus = true;
        self.pending = false;
        self.error = None;
        cx.notify();
    }
    pub(super) fn submit(&mut self, preview: bool, cx: &mut Context<Self>) {
        let changes = self.model["commitChanges"].clone();
        self.submit_action(preview, &changes, cx);
    }

    fn submit_action(&mut self, preview: bool, overrides: &Value, cx: &mut Context<Self>) {
        let mut request = json!({"request": if preview { "StreamerzEntityDraft" } else { "StreamerzEditEntity" }, "kind": self.model["kind"], "itemId": self.model["item"]["id"].as_str().unwrap_or_default(), "id": if preview { self.route.replace(":save", ":json") } else { self.route.clone() }});
        if let Some(editor) = &self.json_editor {
            request["json"] = editor.read(cx).value().to_string().into();
        } else {
            let mut changes = self.changes(cx);
            if !preview {
                for change in overrides.as_array().into_iter().flatten() {
                    changes.retain(|existing| existing["field"] != change["field"]);
                    changes.push(change.clone());
                }
            }
            request["changes"] = json!(changes);
        }
        send(request);
        self.pending = true;
        self.error = None;
        cx.notify();
    }
    /// What a field holds now, as the core takes it (none: a list with nothing picked).
    fn value(&self, index: usize, cx: &App) -> Option<Value> {
        Some(match self.fields.get(index)? {
            Field::Bool(value) => json!(value),
            Field::Text(input) => json!(input.read(cx).value().to_string()),
            Field::Multi(input) | Field::Lines { area: input, .. } => json!(input.read(cx).value().to_string()),
            Field::Pick(input) => json!(input.read(cx).selected_index(cx)?.row),
            Field::Choice { input: Some(input), .. } => json!(input.read(cx).value().to_string()),
            Field::Gallery(gallery) => json!(gallery.read(cx).selected),
            Field::Color(state) => json!(hex(state.read(cx).value()?)),
            Field::Slider(state) => json!(tidy(state.read(cx).value().start())),
            Field::Checks { ids, on, .. } => json!(ids
                .iter()
                .zip(on)
                .filter(|(_, on)| **on)
                .map(|(id, _)| id.as_str())
                .collect::<Vec<_>>()),
            Field::Font { select, system } => match select.read(cx).selected_value() {
                Some(name) if name != system => json!(name),
                _ => json!(""),
            },
            Field::Choice { select, ids, .. } => {
                json!(select
                    .read(cx)
                    .selected_value()
                    .and_then(|&idx| ids.get(idx))
                    .cloned()
                    .unwrap_or_default())
            }
        })
    }

    /// Whether a field is drawn: not hidden by the core, and the field it depends on (`showIf`) has
    /// the value that calls for it.
    fn shown(&self, index: usize, cx: &App) -> bool {
        let field = &self.model["fields"][index];
        if field["hidden"] == true {
            return false;
        }
        let when = &field["showIf"];
        match when["field"].as_u64() {
            Some(other) => self
                .value(other as usize, cx)
                .is_some_and(|value| value == when["equals"]),
            None => true,
        }
    }

    fn changes(&self, cx: &App) -> Vec<Value> {
        (0..self.fields.len())
            .filter_map(|index| {
                let value = self.value(index, cx)?;
                (value != self.model["fields"][index]["value"]).then(|| json!({"field": index, "value": value}))
            })
            .collect()
    }
}
impl Form {
    /// A reply for field `index`'s picture picker: its pictures (`images`) or its reloaded list (`choices`).
    pub fn gallery_reply(&mut self, index: usize, what: &str, reply: &Value, cx: &mut Context<Self>) {
        if let Some(Field::Gallery(gallery)) = self.fields.get(index) {
            gallery.update(cx, |gallery, cx| match what {
                "images" => gallery.images(reply, cx),
                _ => gallery.set_choices(reply, cx),
            });
        }
    }

    /// The core downloaded pictures: the picture pickers ask for theirs again.
    pub fn images_arrived(&mut self, cx: &mut Context<Self>) {
        for field in &self.fields {
            if let Field::Gallery(gallery) = field {
                gallery.update(cx, |gallery, _| gallery.images_arrived());
            }
        }
    }

    /// Opens the system's file dialog for field `index`; the file picked goes in its box.
    fn browse(&mut self, index: usize, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        });
        cx.spawn(async move |form, cx| {
            let picked = match paths.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                Ok(Err(error)) => {
                    let _ = form.update(cx, |form, cx| {
                        form.error = Some(crate::error::StreamerzError::from_message(&error.to_string()));
                        cx.notify();
                    });
                    None
                }
                _ => None,
            };
            if let Some(path) = picked {
                let _ = form.update(cx, |form, cx| {
                    form.picked = Some((index, path.display().to_string()));
                    cx.notify();
                });
            }
        })
        .detach();
    }
}
/// Leave room for wrapped text, including explicit line breaks.
fn wrapped(text: &str, line: f32, width: f32) -> f32 {
    text.lines()
        .map(|text| line * (text.chars().count() as f32 / (width / 7.).max(1.)).ceil().max(1.))
        .sum::<f32>()
        .max(line)
}
/// A form's fix button asks for another page.
impl EventEmitter<super::sections::Navigate> for Form {}

impl Render for Form {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.autofocus {
            self.autofocus = false;
            if let Some(editor) = &self.json_editor {
                // The old field leaves the focus tree when JSON replaces it. Keep dismissal and
                // keyboard input routed through the open dialog.
                editor.update(cx, |editor, cx| editor.focus(window, cx));
            } else {
                let index = self.model["focusField"]
                    .as_u64()
                    .map(|index| index as usize)
                    .or_else(|| {
                        self.fields.iter().enumerate().position(|(index, field)| {
                            self.shown(index, cx)
                                && self.model["fields"][index]["value"].as_str().is_some_and(str::is_empty)
                                && matches!(field, Field::Text(_) | Field::Multi(_))
                        })
                    })
                    .or_else(|| {
                        self.fields.iter().enumerate().position(|(index, field)| {
                            self.shown(index, cx) && matches!(field, Field::Text(_) | Field::Multi(_))
                        })
                    });
                match index.and_then(|index| self.fields.get(index)) {
                    Some(Field::Text(input)) => input.update(cx, |input, cx| input.focus(window, cx)),
                    Some(Field::Multi(input)) => input.update(cx, |input, cx| input.focus(window, cx)),
                    _ => {}
                }
            }
        }
        if let Some((index, path)) = self.picked.take() {
            match self.fields.get(index) {
                Some(Field::Text(input)) => input.update(cx, |input, cx| input.set_value(path, window, cx)),
                // A list that also takes typed text: the file picked is typed, under "Other".
                Some(Field::Choice {
                    select,
                    input: Some(input),
                    ids,
                    ..
                }) => {
                    if let Some(last) = ids.len().checked_sub(1) {
                        select.update(cx, |select, cx| select.set_selected_value(&last, window, cx));
                    }
                    input.update(cx, |input, cx| input.set_value(path, window, cx));
                }
                _ => {}
            }
        }
        let mut body = div()
            .id("form-fields")
            .flex()
            .flex_col()
            .gap(px(SPACE_SM))
            .overflow_y_scroll()
            .min_h_0()
            // The JSON editor fills the form; fields take the room they need and scroll past the cap.
            .when(self.json_editor.is_some(), |body| body.flex_1());
        let note_width = (f32::from(window.viewport_size().width).min(560.) - 80.).max(100.);
        let mut wanted = 0.;
        if let Some(hint) = self.model["hint"].as_str().filter(|hint| !hint.is_empty()) {
            body = body.child(
                div()
                    .flex_shrink_0()
                    .text_size(px(FONT_CAPTION))
                    .line_height(px(LINE_CAPTION))
                    .text_color(cx.theme().muted_foreground)
                    .child(hint.to_owned()),
            );
            wanted += wrapped(hint, LINE_CAPTION, note_width) + SPACE_SM;
        }
        if let Some(err) = &self.error {
            body = body.child(super::notice::error(err, cx));
            wanted += super::notice::estimated_error_height(err, note_width) + 8.;
        }
        for (i, warning) in self.model["warnings"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .enumerate()
        {
            body = body.child(super::notice::warn(("form-warning", i), warning, cx));
            wanted += wrapped(warning, 18., note_width) + 18.;
        }
        // What fixes it, one click away (a login or a permission: the Accounts page).
        if let Some(go) = self.model["fix"]["go"].as_str() {
            let go = go.to_owned();
            let icon = self.model["fix"]["icon"].as_str().unwrap_or("arrow-right").to_owned();
            body = body.child(
                div().flex_shrink_0().child(
                    Button::new("form-fix")
                        .small()
                        .h(px(CONTROL_HEIGHT))
                        .icon(gpui_kit::component::Icon::empty().path(format!("icons/{icon}.svg")))
                        .label(self.model["fix"]["label"].as_str().unwrap_or_default().to_owned())
                        .on_click(cx.listener(move |_, _, window, cx| {
                            crate::controls::dialog::dismiss(window, cx);
                            cx.emit(super::sections::Navigate(go.clone()));
                        })),
                ),
            );
            wanted += CONTROL_HEIGHT + SPACE_SM;
        }
        // Why part of the form is unavailable, in quiet text.
        for note in self.model["notes"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            body = body.child(super::notice::info(note, cx).flex_shrink_0());
            wanted += wrapped(note, 18., note_width) + 4.;
        }
        if let Some(editor) = &self.json_editor {
            wanted += 440.;
            body = body.child(Textarea::new(editor).aria_label(tr("btn.edit_json")).h_full());
        } else {
            let mut index = 0;
            while index < self.fields.len() {
                // A heading above the field that starts a part of the form.
                if let Some(title) = self.model["fields"][index]["section"].as_str() {
                    let collapsed =
                        (self.model["fields"][index]["collapsed"] == true) ^ self.section_toggles.contains(&index);
                    let heading = index;
                    body = body.child(
                        Button::new(("form-section", index))
                            .small()
                            .h(px(CONTROL_HEIGHT))
                            .ghost()
                            .when(index > 0, |button| button.mt(px(SPACE_SM)))
                            .w_full()
                            .justify_start()
                            .icon(gpui_kit::component::Icon::empty().path(if collapsed {
                                "icons/chevron-right.svg"
                            } else {
                                "icons/chevron-down.svg"
                            }))
                            .label(title.to_owned())
                            .on_click(cx.listener(move |form, _, _, cx| {
                                if !form.section_toggles.remove(&heading) {
                                    form.section_toggles.insert(heading);
                                }
                                cx.notify();
                            })),
                    );
                    wanted += CONTROL_HEIGHT + SPACE_SM + if index > 0 { SPACE_SM } else { 0. };
                    if collapsed {
                        index += 1;
                        while index < self.fields.len() && !self.model["fields"][index]["section"].is_string() {
                            index += 1;
                        }
                        continue;
                    }
                }
                if !self.shown(index, cx) {
                    index += 1;
                    continue;
                }
                if matches!(self.fields[index], Field::Bool(_)) {
                    let long = self.model["fields"][index]["label"]
                        .as_str()
                        .is_some_and(|label| label.chars().count() > 28);
                    let mut row = div()
                        .flex()
                        .items_center()
                        .gap(px(SPACE_SM))
                        .min_h(px(CONTROL_HEIGHT))
                        .flex_shrink_0();
                    for place in 0..2 {
                        let Some(Field::Bool(checked)) = self.fields.get(index) else {
                            break;
                        };
                        // The second box of a line may be one that is not shown now: it is skipped next.
                        if place > 0 && !self.shown(index, cx) {
                            break;
                        }
                        // A box that starts a part of the form goes on a line of its own, under the heading.
                        if place > 0 && self.model["fields"][index]["section"].is_string() {
                            break;
                        }
                        if place > 0
                            && (long
                                || self.model["fields"][index]["label"]
                                    .as_str()
                                    .is_some_and(|label| label.chars().count() > 28))
                        {
                            break;
                        }
                        let field_index = index;
                        let label = self.model["fields"][index]["label"]
                            .as_str()
                            .unwrap_or_default()
                            .to_owned();
                        // A platform's check box carries its logo; a disabled one says why on hover.
                        let brand = self.model["fields"][index]["brand"].as_str().map(str::to_owned);
                        let tip = self.model["fields"][index]["tip"]
                            .as_str()
                            .map(|tip| SharedString::from(tip.to_owned()));
                        row = row.child(
                            div()
                                .id(("field-box", index))
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .items_center()
                                .gap_2()
                                .when_some(brand, |box_, brand| {
                                    box_.child(
                                        img(format!("brands/{brand}.svg"))
                                            .id(("field-brand", index))
                                            .size_4()
                                            .flex_shrink_0(),
                                    )
                                })
                                .when_some(tip, |box_, tip| {
                                    box_.tooltip(move |window, cx| {
                                        gpui_kit::component::tooltip::Tooltip::new(tip.clone()).build(window, cx)
                                    })
                                })
                                .child(
                                    Checkbox::new(("field", index))
                                        .small()
                                        .w_full()
                                        .min_w_0()
                                        .label(label)
                                        .checked(*checked)
                                        .disabled(self.pending || self.model["fields"][index]["disabled"] == true)
                                        .on_click(cx.listener(move |form, checked, _, cx| {
                                            form.fields[field_index] = Field::Bool(*checked);
                                            cx.notify();
                                        })),
                                ),
                        );
                        index += 1;
                    }
                    body = body.child(row);
                    wanted += CONTROL_HEIGHT + SPACE_SM;
                    continue;
                }
                let label = self.model["fields"][index]["label"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned();
                let (editor, height): (AnyElement, f32) = match &self.fields[index] {
                    Field::Gallery(gallery) => {
                        (Gallery::field(gallery, label.clone(), self.pending, cx), CONTROL_HEIGHT)
                    }
                    Field::Choice {
                        select,
                        input,
                        ids,
                        other,
                    } => {
                        let mut row = div().flex().gap(px(SPACE_SM)).items_center();
                        let typing = !*other || select.read(cx).selected_value().is_some_and(|&at| at + 1 == ids.len());
                        // With "Other" in the list, the box shows only once it is picked.
                        if let Some(input) = input.as_ref().filter(|_| typing || ids.len() <= 1) {
                            row = row.child(
                                div().flex_1().min_w_0().child(boxed(
                                    Input::new(input)
                                        .small()
                                        .appearance(false)
                                        .aria_label(label.clone())
                                        .disabled(self.pending),
                                    CONTROL_HEIGHT,
                                    cx,
                                )),
                            );
                        }
                        // A field that also takes typed text shows the list only when something was found.
                        if (input.is_none() || !ids.is_empty()) && !(*other && ids.len() <= 1) {
                            let picker = Select::new(select)
                                .small()
                                .h(px(CONTROL_HEIGHT))
                                .placeholder(tr("forms.pick_from_list"))
                                .search_placeholder(tr("lists.search"))
                                .disabled(self.pending);
                            row = row.child(div().flex_1().min_w_0().child(picker));
                        }
                        // A file of the overlay, or one to bring in from this computer.
                        if self.model["fields"][index]["browse"].is_string() {
                            let field_index = index;
                            row = row.child(
                                Button::new(("browse", index))
                                    .small()
                                    .h(px(CONTROL_HEIGHT))
                                    .icon(gpui_kit::component::Icon::empty().path("icons/folder-open.svg"))
                                    .label(tr("btn.browse"))
                                    .disabled(self.pending)
                                    .on_click(cx.listener(move |form, _, _, cx| form.browse(field_index, cx))),
                            );
                        }
                        (row.into_any_element(), CONTROL_HEIGHT)
                    }
                    Field::Checks {
                        ids,
                        labels,
                        brands,
                        on,
                    } => {
                        let field_index = index;
                        // Action and permission lists can contain any number of long names.
                        let width = (f32::from(window.viewport_size().width).min(560.) - 230.).max(90.);
                        let mut height = 0.;
                        let mut row = div().w_full().min_w_0().flex().flex_col().gap(px(SPACE_XS));
                        if ids.is_empty() {
                            row = row.child(tr("forms.none"));
                            height = CONTROL_HEIGHT;
                        }
                        for (at, _) in ids.iter().enumerate() {
                            let name = labels.get(at).cloned().unwrap_or_default();
                            let brand = brands.get(at).cloned().flatten();
                            let checked = on.get(at).copied().unwrap_or(false);
                            let check_id = (index << 16) + at;
                            height += wrapped(&name, CONTROL_HEIGHT, width - 24.) + SPACE_XS;
                            row = row.child(
                                div()
                                    .w_full()
                                    .min_w_0()
                                    .flex()
                                    .flex_shrink_0()
                                    .items_center()
                                    .gap(px(SPACE_XS))
                                    .min_h(px(CONTROL_HEIGHT))
                                    .when_some(brand, |box_, brand| {
                                        box_.child(
                                            img(format!("brands/{brand}.svg"))
                                                .id(("check-brand", check_id))
                                                .size_4()
                                                .flex_shrink_0(),
                                        )
                                    })
                                    .child(
                                        Checkbox::new(("check", check_id))
                                            .small()
                                            .w_full()
                                            .min_w_0()
                                            .label(name)
                                            .checked(checked)
                                            .disabled(self.pending)
                                            .on_click(cx.listener(move |form, checked: &bool, _, cx| {
                                                if let Some(Field::Checks { on, .. }) = form.fields.get_mut(field_index)
                                                {
                                                    if let Some(slot) = on.get_mut(at) {
                                                        *slot = *checked;
                                                    }
                                                }
                                                cx.notify();
                                            })),
                                    ),
                            );
                        }
                        (row.into_any_element(), height)
                    }
                    Field::Text(state) => {
                        let input = Input::new(state)
                            .small()
                            .appearance(false)
                            .aria_label(label.clone())
                            .disabled(self.pending);
                        let input = if self.model["fields"][index]["widget"]["kind"] == "secret" {
                            input.mask_toggle()
                        } else {
                            input
                        };
                        let browse = self.model["fields"][index]["browse"].as_str().unwrap_or_default();
                        if browse.is_empty() {
                            (boxed(input, CONTROL_HEIGHT, cx), CONTROL_HEIGHT)
                        } else {
                            // A file's box with Browse beside it; an image shows under it once it's found.
                            let field_index = index;
                            let line = div()
                                .flex()
                                .gap(px(SPACE_SM))
                                .child(div().flex_1().min_w_0().child(boxed(input, CONTROL_HEIGHT, cx)))
                                .child(
                                    Button::new(("browse", index))
                                        .small()
                                        .h(px(CONTROL_HEIGHT))
                                        .icon(gpui_kit::component::Icon::empty().path("icons/folder-open.svg"))
                                        .label(tr("btn.browse"))
                                        .disabled(self.pending)
                                        .on_click(cx.listener(move |form, _, _, cx| form.browse(field_index, cx))),
                                );
                            let file = state.read(cx).value().trim().trim_matches('"').to_owned();
                            let image = browse == "image" && !file.is_empty() && std::path::Path::new(&file).is_file();
                            if image {
                                let preview = img(std::path::PathBuf::from(file))
                                    .id(("browse-preview", index))
                                    .h(px(126.))
                                    .w(px(224.))
                                    .rounded(px(4.))
                                    .object_fit(ObjectFit::Contain);
                                (
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap_2()
                                        .child(line)
                                        .child(preview)
                                        .into_any_element(),
                                    160.,
                                )
                            } else {
                                (line.into_any_element(), CONTROL_HEIGHT)
                            }
                        }
                    }
                    Field::Multi(input) => (
                        boxed(
                            Textarea::new(input)
                                .appearance(false)
                                .aria_label(label.clone())
                                .h(px(78.))
                                .disabled(self.pending),
                            80.,
                            cx,
                        ),
                        80.,
                    ),
                    Field::Lines { area, select, .. } => {
                        let picker = Select::new(select)
                            .small()
                            .h(px(CONTROL_HEIGHT))
                            .placeholder(tr("forms.pick_from_list"))
                            .search_placeholder(tr("lists.search"))
                            .disabled(self.pending);
                        let area = boxed(
                            Textarea::new(area)
                                .appearance(false)
                                .aria_label(label.clone())
                                .h(px(78.))
                                .disabled(self.pending),
                            80.,
                            cx,
                        );
                        (
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(SPACE_SM))
                                .child(picker)
                                .child(area)
                                .into_any_element(),
                            CONTROL_HEIGHT + SPACE_SM + 80.,
                        )
                    }
                    Field::Pick(input) => (
                        Select::new(input)
                            .small()
                            .h(px(CONTROL_HEIGHT))
                            .when_some(
                                self.model["fields"][index]["placeholder"].as_str().map(str::to_owned),
                                |select, hint| select.placeholder(hint),
                            )
                            .disabled(self.pending)
                            .into_any_element(),
                        CONTROL_HEIGHT,
                    ),
                    Field::Font { select, .. } => (
                        Select::new(select)
                            .small()
                            .h(px(CONTROL_HEIGHT))
                            .search_placeholder(tr("lists.search"))
                            .disabled(self.pending)
                            .into_any_element(),
                        CONTROL_HEIGHT,
                    ),
                    Field::Color(state) => {
                        let shown = state.read(cx).value().map(hex).unwrap_or_default();
                        (
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(ColorPicker::new(state).small())
                                .child(div().text_color(crate::theme::quiet(cx)).child(shown))
                                .into_any_element(),
                            CONTROL_HEIGHT,
                        )
                    }
                    Field::Slider(state) => {
                        let shown = tidy(state.read(cx).value().start()).to_string();
                        (
                            div()
                                .id(("slider-field", index))
                                .test_support()
                                .role(Role::Slider)
                                .aria_label(label.clone())
                                .flex()
                                .items_center()
                                .gap_3()
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .child(Slider::new(state).disabled(self.pending)),
                                )
                                .child(
                                    div()
                                        .w(px(52.))
                                        .flex_shrink_0()
                                        .text_color(crate::theme::quiet(cx))
                                        .child(shown),
                                )
                                .into_any_element(),
                            CONTROL_HEIGHT,
                        )
                    }
                    Field::Bool(_) => unreachable!(),
                };
                // What the field is for, when its maker said: shown over its label.
                let about = self.model["fields"][index]["tip"].as_str().map(str::to_owned);
                let variables = self.model["variables"].as_array().cloned().unwrap_or_default();
                let insertable =
                    !variables.is_empty() && matches!(self.fields[index], Field::Text(_) | Field::Multi(_));
                let height = if insertable { height.max(54.0_f32) } else { height };
                let owner = cx.entity().downgrade();
                body = body.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(SPACE_SM))
                        .h(px(height))
                        .flex_shrink_0()
                        .child({
                            let label_tip = about.unwrap_or_else(|| label.clone());
                            div()
                                .id(("form-lbl", index))
                                .w(px(150.))
                                .flex_shrink_0()
                                .flex()
                                .flex_col()
                                .gap(px(SPACE_XS))
                                .child(div().truncate().child(label))
                                .when(insertable, |label| {
                                    label.child(
                                        Button::new(("insert-variable", index))
                                            .small()
                                            .ghost()
                                            .label(tr("forms.insert_variable"))
                                            .disabled(self.pending)
                                            .dropdown_menu(move |mut menu, _, _| {
                                                for name in variables.iter().filter_map(Value::as_str) {
                                                    let value = format!("%{name}%");
                                                    let owner = owner.clone();
                                                    menu = menu.item(PopupMenuItem::new(value.clone()).on_click(
                                                        move |_, window, cx| {
                                                            let _ = owner.update(cx, |form, cx| {
                                                                match form.fields.get(index) {
                                                                    Some(Field::Text(input)) => {
                                                                        input.update(cx, |input, cx| {
                                                                            input.insert(value.clone(), window, cx);
                                                                            input.focus(window, cx);
                                                                        })
                                                                    }
                                                                    Some(Field::Multi(input)) => {
                                                                        input.update(cx, |input, cx| {
                                                                            input.insert(value.clone(), window, cx);
                                                                            input.focus(window, cx);
                                                                        })
                                                                    }
                                                                    _ => {}
                                                                }
                                                            });
                                                        },
                                                    ));
                                                }
                                                menu
                                            }),
                                    )
                                })
                                .when(!label_tip.is_empty(), |el| {
                                    el.tooltip(move |window, cx| {
                                        gpui_kit::component::tooltip::Tooltip::new(label_tip.clone()).build(window, cx)
                                    })
                                })
                        })
                        .child(div().flex_1().min_w_0().child(editor)),
                );
                wanted += height + SPACE_SM;
                index += 1;
            }
        }
        let cap = (f32::from(window.viewport_size().height) - 220.).clamp(120., 560.);
        // Every field counted the gap under it, and the last one has none.
        let height = (wanted - SPACE_SM).max(0.).min(cap);
        let json = self.json_editor.is_some();
        div()
            .id("editor-form")
            .test_support()
            .role(Role::Group)
            .aria_label(self.model["title"].as_str().unwrap_or_default().to_owned())
            .text_size(px(FONT_BODY))
            .line_height(px(LINE_BODY))
            .flex()
            .flex_col()
            .gap(px(SPACE_MD))
            .when(json, |form| form.h(px(height)))
            .when(!json, |form| form.max_h(px(cap)))
            .child(body)
    }
}

impl Form {
    /// The form's buttons, for the dialog's footer: Edit JSON at the left, OK and Cancel at the right.
    pub fn buttons(&self, cx: &mut Context<Self>) -> Div {
        let left = (self.json_editor.is_none() && self.model["json"] != false).then(|| {
            Button::new("edit-json")
                .h(px(CONTROL_HEIGHT))
                .ghost()
                .icon(gpui_kit::component::Icon::empty().path("icons/braces.svg"))
                .label(tr("btn.edit_json"))
                .disabled(self.pending)
                .on_click(cx.listener(|form, _, _, cx| form.submit(true, cx)))
                .into_any_element()
        });
        let right = div()
            .flex()
            .gap(px(SPACE_SM))
            .child(
                Button::new("cancel-form")
                    .h(px(CONTROL_HEIGHT))
                    .label(tr("dialogs.cancel"))
                    .disabled(self.pending)
                    .on_click(cx.listener(|_, _, window, cx| super::dialog::dismiss(window, cx))),
            )
            .children(
                (self.json_editor.is_none() && self.model["alternateSubmit"].is_object()).then(|| {
                    let action = self.model["alternateSubmit"].clone();
                    Button::new("alternate-submit")
                        .h(px(CONTROL_HEIGHT))
                        .label(action["label"].as_str().unwrap_or_default().to_owned())
                        .disabled(self.pending)
                        .on_click(cx.listener(move |form, _, _, cx| form.submit_action(false, &action["changes"], cx)))
                }),
            )
            .child(
                Button::new("save-form")
                    .h(px(CONTROL_HEIGHT))
                    .primary()
                    .label(
                        if self.json_editor.is_some() {
                            Some(tr("btn.save"))
                        } else {
                            self.model["okLabel"].as_str()
                        }
                        .map_or_else(|| tr("dialogs.ok").to_owned(), str::to_owned),
                    )
                    .disabled(self.pending)
                    .on_click(cx.listener(|form, _, _, cx| form.submit(false, cx))),
            );
        super::dialog::footer(left, right, cx)
    }

    /// The buttons as their own view, for a dialog's footer slot.
    pub fn buttons_view(&self) -> Entity<FormButtons> {
        self.buttons_view.clone()
    }
}

/// A form's buttons, drawn where its dialog puts them (the footer).
pub struct FormButtons(WeakEntity<Form>);
impl Render for FormButtons {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        match self.0.upgrade() {
            Some(form) => form.update(cx, |form, cx| form.buttons(cx)).into_any_element(),
            None => div().into_any_element(),
        }
    }
}

/// A text field is a dark box with a quiet border.
fn boxed(field: impl IntoElement, height: f32, cx: &App) -> AnyElement {
    use gpui_kit::component::ActiveTheme;
    div()
        .h(px(height))
        .bg(cx.theme().table)
        .border_1()
        .border_color(cx.theme().input)
        .rounded(px(4.))
        .child(field)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::sanitize_slider;
    use gpui_kit::component::slider::SliderState;

    #[test]
    fn slider_sanitization_prevents_min_above_hundred_or_inverted_range_panics() {
        for (min, max, step, val) in [
            (Some(200.0), Some(400.0), Some(10.0), Some(250.0)),
            (Some(500.0), Some(100.0), Some(5.0), Some(50.0)),
            (Some(f64::NAN), Some(f64::INFINITY), Some(0.0), Some(f64::NEG_INFINITY)),
            (Some(-50.0), Some(-10.0), Some(-2.0), Some(99.0)),
            (None, None, None, None),
        ] {
            let (lo, hi, step, val) = sanitize_slider(min, max, step, val);
            assert!(lo.is_finite() && hi.is_finite() && lo <= hi);
            assert!(step.is_finite() && step > 0.0);
            assert!(val.is_finite() && val >= lo && val <= hi);
            let _state = SliderState::new()
                .max(hi.max(100.))
                .min(lo)
                .max(hi)
                .step(step)
                .default_value(val);
        }
    }

    #[cfg(feature = "test-support")]
    mod ui {
        use super::super::{Field, Form};
        use gpui_kit::test::{TestEventExt, TestQueryExt, TestWindowExt};
        use gpui_kit::{
            div, px, size, AppContext, Bounds, Entity, IntoElement, ParentElement, Render, Styled, TestAppContext,
            Window, WindowBounds, WindowOptions,
        };
        use serde_json::json;

        struct FormHost(Entity<Form>);
        impl Render for FormHost {
            fn render(&mut self, _: &mut Window, _: &mut gpui_kit::Context<Self>) -> impl IntoElement {
                div().size_full().p_4().child(self.0.clone())
            }
        }

        fn open_form(
            model: serde_json::Value,
            cx: &mut TestAppContext,
        ) -> (gpui_kit::WindowHandle<gpui_kit::base::Root>, Entity<Form>) {
            cx.update(|cx| {
                gpui_kit::init(cx);
                crate::theme::apply(true, cx);
            });
            let form_slot: std::rc::Rc<std::cell::RefCell<Option<Entity<Form>>>> =
                std::rc::Rc::new(std::cell::RefCell::new(None));
            let slot = form_slot.clone();
            let window = cx.update(|cx| {
                let bounds = Bounds {
                    origin: gpui_kit::Point::default(),
                    size: size(px(640.), px(720.)),
                };
                let (window, _) = gpui_kit::open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(bounds)),
                        ..Default::default()
                    },
                    cx,
                    move |window, cx| {
                        let form = cx.new(|cx| Form::new(model, "test:form".into(), window, cx));
                        *slot.borrow_mut() = Some(form.clone());
                        cx.new(|_| FormHost(form))
                    },
                )
                .expect("open test window");
                window.downcast::<gpui_kit::base::Root>().expect("root window")
            });
            let form = form_slot.borrow().clone().expect("form created");
            (window, form)
        }

        #[gpui_kit::test]
        fn choice_search_filters_dropdown_and_selects_filtered_item(cx: &mut TestAppContext) {
            let model = json!({
                "title": "Edit stream on Twitch",
                "fields": [
                    {
                        "key": "category",
                        "label": "Category",
                        "value": "Art",
                        "other": true,
                        "choices": [
                            {"id": "ACE COMBAT 8: WINGS OF THEVE", "label": "ACE COMBAT 8: WINGS OF THEVE"},
                            {"id": "AION 2", "label": "AION 2"},
                            {"id": "Albion Online", "label": "Albion Online"},
                            {"id": "Art", "label": "Art"},
                            {"id": "ASMR", "label": "ASMR"},
                            {"id": "Battlefield 6", "label": "Battlefield 6"},
                            {"id": "Just Chatting", "label": "Just Chatting"},
                            {"id": "Music", "label": "Music"},
                            {"id": "Software and Game Development", "label": "Software and Game Development"}
                        ]
                    },
                    {
                        "key": "language",
                        "label": "Language",
                        "value": "en",
                        "strict": true,
                        "choices": [
                            {"id": "en", "label": "English"},
                            {"id": "es", "label": "Spanish"},
                            {"id": "fr", "label": "French"},
                            {"id": "de", "label": "German"},
                            {"id": "it", "label": "Italian"},
                            {"id": "ja", "label": "Japanese"},
                            {"id": "ko", "label": "Korean"},
                            {"id": "pt", "label": "Portuguese"},
                            {"id": "ru", "label": "Russian"},
                            {"id": "zh", "label": "Chinese"}
                        ]
                    }
                ]
            });
            let (window, form) = open_form(model, cx);
            cx.update_window(window.into(), |_, window, cx| {
                window.render_frame(cx);
                assert_eq!(window.find_all_by_role(gpui_kit::Role::TextInput).len(), 0);
                let cat_id = match &form.read(cx).fields[0] {
                    Field::Choice { select, .. } => select.entity_id(),
                    _ => unreachable!(),
                };

                // Click the Category select trigger to open its searchable dropdown.
                window.within(("select", cat_id)).click("input", cx);
                window.render_frame(cx);
                let open_text_inputs = window.find_all_by_role(gpui_kit::Role::TextInput).len();
                assert_eq!(
                    open_text_inputs, 1,
                    "dropdown search input should appear when Select opens"
                );

                // Typing into the open dropdown's search input should filter items to only "Just Chatting",
                // so clicking the first row below the search box selects "Just Chatting" rather than
                // the unfiltered first item ("ACE COMBAT 8: WINGS OF THEVE").
                window.input("Just", cx);
                window.render_frame(cx);
                let inputs = window.find_all_by_role(gpui_kit::Role::TextInput);
                assert_eq!(inputs[0].value(), Some("Just"));
                let search_bounds = inputs[0].bounds();
                let first_row = gpui_kit::point(search_bounds.center().x, search_bounds.bottom() + px(16.));
                use gpui_kit::test::TestEventExt;
                window.pointer_down(first_row, gpui_kit::MouseButton::Left, cx);
                window.pointer_up(first_row, gpui_kit::MouseButton::Left, cx);
            })
            .unwrap();
            cx.run_until_parked();
            cx.update_window(window.into(), |_, window, cx| {
                window.render_frame(cx);
                assert_eq!(form.read(cx).value(0, cx), Some(json!("Just Chatting")));
            })
            .unwrap();
        }

        #[gpui_kit::test]
        fn lines_suggest_search_filters_dropdown_and_adds_filtered_tag(cx: &mut TestAppContext) {
            let model = json!({
                "title": "Edit stream on Twitch",
                "fields": [
                    {
                        "key": "tags",
                        "label": "Tags, one per line",
                        "value": "English",
                        "widget": {"kind": "lines"},
                        "suggest": [
                            {"id": "English", "label": "English"},
                            {"id": "Art", "label": "Art"},
                            {"id": "Cozy", "label": "Cozy"},
                            {"id": "Portuguese", "label": "Portuguese"},
                            {"id": "Speedrun", "label": "Speedrun"}
                        ]
                    }
                ]
            });
            let (window, form) = open_form(model, cx);
            cx.update_window(window.into(), |_, window, cx| {
                window.render_frame(cx);
                let tags_select_id = match &form.read(cx).fields[0] {
                    Field::Lines { select, .. } => select.entity_id(),
                    _ => unreachable!(),
                };

                // Open the Tags suggestion dropdown, search for "Port", and click the first row.
                window.within(("select", tags_select_id)).click("input", cx);
                window.render_frame(cx);
                window.input("Port", cx);
                window.render_frame(cx);
                let inputs = window.find_all_by_role(gpui_kit::Role::TextInput);
                let search_bounds = inputs
                    .into_iter()
                    .find(|el| el.value() == Some("Port"))
                    .expect("dropdown search input should hold typed 'Port'")
                    .bounds();
                let first_row = gpui_kit::point(search_bounds.center().x, search_bounds.bottom() + px(16.));
                use gpui_kit::test::TestEventExt;
                window.pointer_down(first_row, gpui_kit::MouseButton::Left, cx);
                window.pointer_up(first_row, gpui_kit::MouseButton::Left, cx);
            })
            .unwrap();
            cx.run_until_parked();
            cx.update_window(window.into(), |_, window, cx| {
                window.render_frame(cx);
                assert_eq!(form.read(cx).value(0, cx), Some(json!("English\nPortuguese")));
                // Select resets after adding the tag so another tag can be picked cleanly.
                let selected = match &form.read(cx).fields[0] {
                    Field::Lines { select, .. } => select.read(cx).selected_value().copied(),
                    _ => unreachable!(),
                };
                assert_eq!(selected, None);
            })
            .unwrap();
        }

        #[gpui_kit::test]
        fn strict_long_choice_search_filters_and_selects_original_id(cx: &mut TestAppContext) {
            let model = json!({
                "title": "Edit stream on Twitch",
                "fields": [
                    {
                        "key": "language",
                        "label": "Language",
                        "value": "en",
                        "strict": true,
                        "choices": [
                            {"id": "en", "label": "English"},
                            {"id": "es", "label": "Spanish"},
                            {"id": "fr", "label": "French"},
                            {"id": "de", "label": "German"},
                            {"id": "it", "label": "Italian"},
                            {"id": "ja", "label": "Japanese"},
                            {"id": "ko", "label": "Korean"},
                            {"id": "pt", "label": "Portuguese"},
                            {"id": "ru", "label": "Russian"},
                            {"id": "zh", "label": "Chinese"}
                        ]
                    },
                    {
                        "key": "only_pt",
                        "label": "Portuguese hint",
                        "value": false,
                        "widget": {"kind": "bool"},
                        "showIf": {"field": 0, "equals": "pt"}
                    }
                ]
            });
            let (window, form) = open_form(model, cx);
            cx.update_window(window.into(), |_, window, cx| {
                window.render_frame(cx);
                assert!(!form.read(cx).shown(1, cx));
                let lang_id = match &form.read(cx).fields[0] {
                    Field::Choice { select, .. } => select.entity_id(),
                    _ => unreachable!(),
                };
                window.within(("select", lang_id)).click("input", cx);
                window.render_frame(cx);
                window.input("Port", cx);
                window.render_frame(cx);
                let search_bounds = window
                    .find_all_by_role(gpui_kit::Role::TextInput)
                    .into_iter()
                    .find(|el| el.value() == Some("Port"))
                    .expect("dropdown search input should hold 'Port'")
                    .bounds();
                let first_row = gpui_kit::point(search_bounds.center().x, search_bounds.bottom() + px(16.));
                use gpui_kit::test::TestEventExt;
                window.pointer_down(first_row, gpui_kit::MouseButton::Left, cx);
                window.pointer_up(first_row, gpui_kit::MouseButton::Left, cx);
            })
            .unwrap();
            cx.run_until_parked();
            cx.update_window(window.into(), |_, window, cx| {
                window.render_frame(cx);
                assert_eq!(form.read(cx).value(0, cx), Some(json!("pt")));
                assert!(
                    form.read(cx).shown(1, cx),
                    "showIf dependent field should become visible when strict choice confirms"
                );
            })
            .unwrap();
        }

        #[gpui_kit::test]
        fn choice_other_and_non_strict_input_sync_properly(cx: &mut TestAppContext) {
            let model = json!({
                "title": "Test",
                "fields": [
                    {
                        "key": "category",
                        "label": "Category",
                        "value": "Art",
                        "other": true,
                        "choices": [
                            {"id": "Art", "label": "Art"},
                            {"id": "Music", "label": "Music"}
                        ]
                    },
                    {
                        "key": "scene",
                        "label": "Scene",
                        "value": "Intro",
                        "choices": [
                            {"id": "Intro", "label": "Intro"},
                            {"id": "Main", "label": "Main"}
                        ]
                    }
                ]
            });
            let (window, form) = open_form(model, cx);
            cx.update_window(window.into(), |_, window, cx| {
                window.render_frame(cx);
                // Only "Scene" input is visible initially because Category is "Art" (not "Other").
                assert_eq!(window.find_all_by_role(gpui_kit::Role::TextInput).len(), 1);
                let cat_id = match &form.read(cx).fields[0] {
                    Field::Choice { select, .. } => select.entity_id(),
                    _ => unreachable!(),
                };
                window.within(("select", cat_id)).click("input", cx);
                window.render_frame(cx);
                window.input("other", cx);
                window.render_frame(cx);
                let search_bounds = window
                    .find_all_by_role(gpui_kit::Role::TextInput)
                    .into_iter()
                    .find(|el| el.value() == Some("other"))
                    .expect("dropdown search input should hold 'other'")
                    .bounds();
                let first_row = gpui_kit::point(search_bounds.center().x, search_bounds.bottom() + px(16.));
                use gpui_kit::test::TestEventExt;
                window.pointer_down(first_row, gpui_kit::MouseButton::Left, cx);
                window.pointer_up(first_row, gpui_kit::MouseButton::Left, cx);
            })
            .unwrap();
            cx.run_until_parked();
            cx.update_window(window.into(), |_, window, cx| {
                window.render_frame(cx);
                // Choosing "Other" reveals Category's text input alongside Scene's.
                let inputs = window.find_all_by_role(gpui_kit::Role::TextInput);
                assert_eq!(
                    inputs.len(),
                    2,
                    "Category custom input should appear when Other is picked"
                );
                let cat_input = match &form.read(cx).fields[0] {
                    Field::Choice { input: Some(input), .. } => input.clone(),
                    _ => unreachable!(),
                };
                cat_input.update(cx, |input, cx| input.focus(window, cx));
                window.key_down("ctrl-a", false, cx);
                window.key_up("ctrl-a", cx);
                window.input("Indie Game", cx);
                window.render_frame(cx);
                assert_eq!(form.read(cx).value(0, cx), Some(json!("Indie Game")));
                // Custom text keeps "Other" selected so the box remains visible.
                assert_eq!(window.find_all_by_role(gpui_kit::Role::TextInput).len(), 2);

                // Typing an existing choice in non-strict "Scene" syncs its Select state.
                let scene_input = match &form.read(cx).fields[1] {
                    Field::Choice { input: Some(input), .. } => input.clone(),
                    _ => unreachable!(),
                };
                scene_input.update(cx, |input, cx| input.focus(window, cx));
                window.key_down("ctrl-a", false, cx);
                window.key_up("ctrl-a", cx);
                window.input("Main", cx);
            })
            .unwrap();
            cx.run_until_parked();
            cx.update_window(window.into(), |_, window, cx| {
                window.render_frame(cx);
                let scene_val = match &form.read(cx).fields[1] {
                    Field::Choice { select, .. } => select.read(cx).selected_value().copied(),
                    _ => unreachable!(),
                };
                assert_eq!(scene_val, Some(1));
            })
            .unwrap();
        }

        #[gpui_kit::test]
        fn font_search_filters_dropdown_and_selects_filtered_font(cx: &mut TestAppContext) {
            let model = json!({
                "title": "Font form",
                "fields": [
                    {
                        "key": "font",
                        "label": "Font",
                        "value": "",
                        "widget": {"kind": "font"}
                    }
                ]
            });
            let (window, form) = open_form(model, cx);
            let target_font = cx
                .update_window(window.into(), |_, window, cx| {
                    window.render_frame(cx);
                    let (font_id, sample) = match &form.read(cx).fields[0] {
                        Field::Font { select, system } => {
                            let names = cx.text_system().all_font_names();
                            let sample = names
                                .into_iter()
                                .find(|n| !n.starts_with('.') && n != system && n.len() >= 4);
                            (select.entity_id(), sample)
                        }
                        _ => unreachable!(),
                    };
                    let sample = sample?;
                    let query = sample.chars().take(4).collect::<String>();
                    window.within(("select", font_id)).click("input", cx);
                    window.render_frame(cx);
                    window.input(&query, cx);
                    window.render_frame(cx);
                    let search_bounds = window
                        .find_all_by_role(gpui_kit::Role::TextInput)
                        .into_iter()
                        .find(|el| el.value() == Some(query.as_str()))
                        .expect("font dropdown search input should hold query")
                        .bounds();
                    let first_row = gpui_kit::point(search_bounds.center().x, search_bounds.bottom() + px(16.));
                    use gpui_kit::test::TestEventExt;
                    window.pointer_down(first_row, gpui_kit::MouseButton::Left, cx);
                    window.pointer_up(first_row, gpui_kit::MouseButton::Left, cx);
                    Some(query)
                })
                .unwrap();
            if let Some(query) = target_font {
                cx.run_until_parked();
                cx.update_window(window.into(), |_, window, cx| {
                    window.render_frame(cx);
                    let val = form.read(cx).value(0, cx).and_then(|v| v.as_str().map(str::to_owned));
                    assert!(
                        val.as_deref()
                            .is_some_and(|s| s.to_lowercase().contains(&query.to_lowercase())),
                        "expected selected font matching '{query}', got {val:?}"
                    );
                })
                .unwrap();
            }
        }
    }
}
