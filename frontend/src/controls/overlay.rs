//! An overlay's own page, opened from the Overlays list: Back to the list, its toolbar, the preview and
//! its tabs (Settings, Test, Media, Live, Alerts for an alert box, Code). The core gives the model (the
//! `overlay` of the Overlays read); every change is a core request, as everywhere else.
use super::form::Form;
use crate::i18n::tr;
use crate::send;
use crate::theme::{self, CONTROL_HEIGHT, FONT_BODY, FONT_CAPTION, SPACE_LG, SPACE_MD, SPACE_SM, SPACE_XS};
use gpui_kit::component::{
    button::{Button, ButtonVariants},
    scroll::ScrollableElement,
    switch::Switch,
    ActiveTheme, Disableable, Icon, Sizable, WindowExt,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use serde_json::{json, Value};
use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

/// What the page asks of the window around it.
pub enum OverlayEvent {
    /// Back to the list.
    Back,
    /// Another tab: the next read brings its body.
    Tab(String),
}

/// From this width of content on, the preview sits beside the tabs; under it, above them.
const WIDE: f32 = 1100.;

pub struct OverlayPage {
    model: Value,
    route: String,
    /// The tab's form (Settings, Code), kept while the tab and its kind stay the same.
    form: Option<Entity<Form>>,
    form_kind: String,
    /// Each change of the settings form goes to the preview as a draft.
    _form_edits: Option<Subscription>,
    /// A form asked by a toolbar button (Add to OBS, Export), shown as a dialog.
    dialog: Option<Entity<Form>>,
    focus: FocusHandle,
    /// The live preview: the overlay it was started for, what is behind it, and the newest picture.
    preview_key: String,
    background: &'static str,
    frame: Option<Arc<RenderImage>>,
    frame_size: (f32, f32),
    frame_seq: u64,
    frame_pending: bool,
    frame_again: bool,
    preview_note: String,
    /// Where the picture is drawn, for turning a click into the page's own coordinates.
    preview_bounds: Rc<Cell<Bounds<Pixels>>>,
    /// Drawn at its own size (scrolling) instead of fitted.
    actual_size: bool,
    /// Your scene: a picture of OBS's program scene behind a see-through page.
    scene: Option<Arc<RenderImage>>,
    scene_seq: u64,
}

/// What can be behind the preview: see-through (a checkerboard), dark, light, or the OBS scene.
const BACKGROUNDS: [(&str, &str); 4] = [
    ("transparent", "overlays.preview.see_through"),
    ("#1f1f23", "overlays.preview.dark"),
    ("#f4f4f5", "overlays.preview.light"),
    ("scene", "overlays.preview.scene"),
];

/// A model value as text: a string as it is, anything else as JSON (nothing for null).
fn text_of(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// Bytes from base64 (the picture comes in JSON).
fn base64(text: &str) -> Vec<u8> {
    let value = |c: u8| match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    };
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let (mut buffer, mut bits) = (0u32, 0u32);
    for v in text.bytes().filter_map(value) {
        buffer = (buffer << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }
    out
}

impl Drop for OverlayPage {
    fn drop(&mut self) {
        // Leaving the page closes the preview's browser.
        if !self.preview_key.is_empty() {
            send(json!({"request": "StreamerzPreview", "op": "stop", "id": "preview-stop"}));
        }
    }
}

impl EventEmitter<OverlayEvent> for OverlayPage {}

impl OverlayPage {
    pub fn new(model: Value, route: String, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut page = Self {
            model: Value::Null,
            route,
            form: None,
            form_kind: String::new(),
            _form_edits: None,
            dialog: None,
            focus: cx.focus_handle(),
            preview_key: String::new(),
            background: BACKGROUNDS[1].0,
            frame: None,
            frame_size: (0., 0.),
            frame_seq: 0,
            frame_pending: false,
            frame_again: false,
            preview_note: tr("overlays.preview.starting").to_owned(),
            preview_bounds: Rc::new(Cell::new(Bounds::default())),
            actual_size: false,
            scene: None,
            scene_seq: 0,
        };
        page.update_model(model, window, cx);
        page
    }

    /// The overlay's id (`<group>:<file>`).
    pub fn id(&self) -> &str {
        self.model["id"].as_str().unwrap_or_default()
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        self.focus.focus(window, cx);
    }

    pub fn update_model(&mut self, model: Value, window: &mut Window, cx: &mut Context<Self>) {
        let form = model["sections"]
            .as_array()
            .and_then(|sections| sections.iter().find(|s| s["form"].is_object()))
            .map_or(&Value::Null, |s| &s["form"]);
        let kind = form["kind"].as_str().unwrap_or_default().to_owned();
        // A form being typed in stays as it is; another tab or overlay brings its own.
        if kind.is_empty() {
            self.form = None;
            self.form_kind.clear();
        } else if kind != self.form_kind || self.form.is_none() {
            let route = format!("{}:save", self.route);
            let value = form.clone();
            let new = cx.new(|cx| Form::new(value, route, window, cx));
            // The settings are drafts as they are changed: the preview shows them before Save.
            self._form_edits = (model["tab"] == "settings").then(|| {
                let route = self.route.clone();
                let id = model["id"].clone();
                cx.subscribe(&new, move |_, form, _: &super::form::Edited, cx| {
                    let changes = form.read(cx).changes(cx);
                    send(
                        json!({"request": "StreamerzPreview", "op": "draft", "itemId": id, "changes": changes,
                        "id": format!("{route}:draft")}),
                    );
                })
            });
            self.form = Some(new);
            self.form_kind = kind;
        }
        self.model = model;
        self.start_preview(false);
        cx.notify();
    }

    /// Starts the preview for this overlay (its size and background), unless it already shows it.
    fn start_preview(&mut self, again: bool) {
        let key = format!(
            "{}|{}x{}|{}",
            self.id(),
            self.model["width"],
            self.model["height"],
            self.background
        );
        if key == self.preview_key && !again {
            return;
        }
        self.preview_key = key;
        let behind = if self.background == "scene" {
            "transparent"
        } else {
            self.background
        };
        send(
            json!({"request": "StreamerzPreview", "op": "start", "itemId": self.id(),
            "background": behind, "id": format!("{}:preview", self.route)}),
        );
    }

    /// The core has a new picture: ask for it, one request at a time.
    pub fn want_frame(&mut self) {
        if self.frame_pending {
            self.frame_again = true;
            return;
        }
        self.frame_pending = true;
        send(
            json!({"request": "StreamerzPreview", "op": "frame", "have": self.frame_seq,
            "haveScene": self.scene_seq, "id": format!("{}:frame", self.route)}),
        );
    }

    fn take_frame(&mut self, value: &Value, window: &mut Window, cx: &mut Context<Self>) {
        self.frame_pending = false;
        if let Some(error) = value["error"].as_str().filter(|e| !e.is_empty()) {
            self.preview_note = error.to_owned();
        }
        if let Some(data) = value["data"].as_str() {
            let format = if value["format"] == "png" {
                ImageFormat::Png
            } else {
                ImageFormat::Jpeg
            };
            let image = Image::from_bytes(format, base64(data));
            if let Ok(render) = image.to_image_data(cx.svg_renderer()) {
                if let Some(old) = self.frame.replace(render) {
                    let _ = window.drop_image(old);
                }
                self.frame_size = (
                    value["width"].as_f64().unwrap_or(1920.) as f32,
                    value["height"].as_f64().unwrap_or(1080.) as f32,
                );
            }
            self.frame_seq = value["seq"].as_u64().unwrap_or(self.frame_seq);
        }
        if let Some(data) = value["scene"].as_str() {
            let image = Image::from_bytes(ImageFormat::Jpeg, base64(data));
            if let Ok(render) = image.to_image_data(cx.svg_renderer()) {
                if let Some(old) = self.scene.replace(render) {
                    let _ = window.drop_image(old);
                }
            }
            self.scene_seq = value["sceneSeq"].as_u64().unwrap_or(self.scene_seq);
        }
        if std::mem::take(&mut self.frame_again) {
            self.want_frame();
        }
        cx.notify();
    }

    /// A click, the wheel or a key at a point of the preview, sent to the page at its own coordinates.
    fn send_input(&self, mut event: Value, at: Option<Point<Pixels>>) {
        if let Some(at) = at {
            let bounds = self.preview_bounds.get();
            let (bw, bh) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
            let (pw, ph) = self.frame_size;
            if bw <= 0. || bh <= 0. || pw <= 0. || ph <= 0. {
                return;
            }
            let scale = if self.actual_size { 1. } else { (bw / pw).min(bh / ph) };
            let (ox, oy) = if self.actual_size {
                (0., 0.)
            } else {
                ((bw - pw * scale) / 2., (bh - ph * scale) / 2.)
            };
            let x = (f32::from(at.x) - f32::from(bounds.origin.x) - ox) / scale;
            let y = (f32::from(at.y) - f32::from(bounds.origin.y) - oy) / scale;
            if x < 0. || y < 0. || x > pw || y > ph {
                return;
            }
            event["x"] = json!(x);
            event["y"] = json!(y);
        }
        send(json!({"request": "StreamerzPreview", "op": "input", "event": event, "id": "preview-input"}));
    }

    /// The core's answer to one of this page's requests; true when the page should be read again.
    pub fn apply(&mut self, operation: &str, value: Value, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if let Some(err) = crate::error::StreamerzError::from_response(&value) {
            if operation == "draft" {
                if let Some(form) = &self.form {
                    form.update(cx, |form, cx| {
                        form.error = Some(err);
                        cx.notify();
                    });
                }
                return false;
            }
            if matches!(operation, "preview" | "frame") {
                self.frame_pending = false;
                self.preview_note = err.description.clone();
                cx.notify();
                return false;
            }
            let form = match operation {
                "save" | "json" => self.form.clone(),
                "dialog-save" => self.dialog.clone(),
                _ => None,
            };
            match form {
                Some(form) => form.update(cx, |form, cx| {
                    form.pending = false;
                    form.error = Some(err);
                    cx.notify();
                }),
                None => window.push_notification(err.summary_line(), cx),
            }
            return false;
        }
        match operation {
            "frame" => {
                self.take_frame(&value, window, cx);
                false
            }
            "preview" => false,
            "draft" => {
                self.model["draft"] = value["draft"].clone();
                cx.notify();
                false
            }
            "revert" => {
                // The form goes back to the saved values.
                self.form = None;
                self.form_kind.clear();
                true
            }
            "live" => true,
            "save" if self.form_kind.starts_with("overlay:test:") => {
                // Sent: the test form keeps what was typed, for the next one.
                if let Some(form) = &self.form {
                    form.update(cx, |form, cx| {
                        form.pending = false;
                        cx.notify();
                    });
                }
                let message = value["message"].as_str().unwrap_or_else(|| tr("overlays.test.done"));
                window.push_notification(message.to_owned(), cx);
                true
            }
            "save" => {
                // Read again with what was saved: the form is made anew from it.
                self.form = None;
                self.form_kind.clear();
                window.push_notification(tr("overlays.saved").to_owned(), cx);
                true
            }
            "json" => {
                if let Some(form) = &self.form {
                    form.update(cx, |form, cx| {
                        form.show_json(value["json"].as_str().unwrap_or_default(), window, cx)
                    });
                }
                false
            }
            "act" => {
                if value["values"].is_object() {
                    super::list::show_values(&value["values"], window, cx);
                }
                if let Some(text) = value["copy"].as_str() {
                    cx.write_to_clipboard(ClipboardItem::new_string(text.to_owned()));
                }
                if let Some(message) = value["message"].as_str() {
                    window.push_notification(message.to_owned(), cx);
                }
                true
            }
            "form" => {
                let title = value["title"].as_str().unwrap_or_default().to_owned();
                let route = format!("{}:dialog-save", self.route);
                let form = cx.new(|cx| Form::new(value, route, window, cx));
                self.dialog = Some(form.clone());
                window.open_dialog(cx, move |dialog, window, cx| {
                    super::dialog::shell(dialog, title.clone(), 560., window, cx)
                        .child(super::dialog::body(form.clone()))
                        .footer(form.read(cx).buttons_view())
                });
                false
            }
            "dialog-save" => {
                self.dialog = None;
                window.close_dialog(cx);
                if let Some(message) = value["message"].as_str() {
                    window.push_notification(message.to_owned(), cx);
                }
                true
            }
            _ => false,
        }
    }

    /// A toolbar or tab button: its form as a dialog, or the operation run on this overlay.
    fn press(&self, button: &Value, window: &mut Window, cx: &mut App) {
        // A button that asks first (Delete): the question, then the operation.
        if let Some(question) = button["confirm"].as_str() {
            let (question, label) = (
                question.to_owned(),
                button["label"].as_str().unwrap_or_default().to_owned(),
            );
            let danger = button["danger"] == true;
            let request = json!({"request": "StreamerzCoreAction", "domain": "overlays", "table": "overlay",
                "action": button["id"], "itemId": self.id(), "id": format!("{}:act", self.route)});
            window.open_dialog(cx, move |dialog, window, cx| {
                let request = request.clone();
                let confirm = Button::new("overlay-confirm")
                    .label(label.clone())
                    .on_click(move |_, window, cx| {
                        send(request.clone());
                        window.close_dialog(cx);
                    });
                let buttons = div()
                    .flex()
                    .justify_end()
                    .gap(px(SPACE_SM))
                    .child(
                        Button::new("overlay-confirm-cancel")
                            .label(tr("dialogs.cancel"))
                            .on_click(|_, window, cx| super::dialog::dismiss(window, cx)),
                    )
                    .child(if danger { confirm.danger() } else { confirm });
                let footer = super::dialog::footer(None, buttons, cx);
                super::dialog::shell(dialog, label.clone(), 448., window, cx)
                    .child(super::dialog::body(question.clone()))
                    .footer(footer)
            });
            return;
        }
        if let Some(kind) = button["form"].as_str() {
            send(
                json!({"request": "StreamerzEntityForm", "kind": kind, "itemId": "", "id": format!("{}:form", self.route)}),
            );
            return;
        }
        send(
            json!({"request": "StreamerzCoreAction", "domain": "overlays", "table": "overlay",
            "action": button["id"], "itemId": self.id(), "id": format!("{}:act", self.route)}),
        );
    }

    fn button(&self, index: usize, spec: &Value, cx: &mut Context<Self>) -> Button {
        let spec_for_click = spec.clone();
        let label = spec["label"].as_str().unwrap_or_default().to_owned();
        let icon = spec["icon"].as_str().unwrap_or("circle").to_owned();
        Button::new(("overlay-button", index))
            .h(px(CONTROL_HEIGHT))
            .icon(Icon::empty().path(format!("icons/{icon}.svg")).size(px(13.)))
            .label(label)
            .on_click(cx.listener(move |page, _, window, cx| page.press(&spec_for_click, window, cx)))
    }

    fn toolbar(&self, cx: &mut Context<Self>) -> Div {
        let mut bar = div().flex().flex_wrap().items_center().gap(px(SPACE_SM));
        for (index, spec) in self.model["toolbar"].as_array().into_iter().flatten().enumerate() {
            bar = bar.child(self.button(index, spec, cx));
        }
        bar
    }

    /// The overlay as OBS shows it, live: what the preview browser draws, scaled to fit (or at its own
    /// size), on what is chosen behind it, with clicks, the wheel and keys passed on to the page.
    fn preview(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let bounds = self.preview_bounds.clone();
        let checker = self.background == "transparent" || (self.background == "scene" && self.scene.is_none());
        let back = match self.background {
            "#f4f4f5" => gpui_kit::rgb(0xf4f4f5).into(),
            "transparent" => gpui_kit::rgb(0x2a2a2e).into(),
            _ => cx.theme().table,
        };
        let mut stage = div()
            .id("overlay-preview-stage")
            .relative()
            .flex_1()
            .min_h(px(120.))
            .rounded(px(6.))
            .border_1()
            .border_color(cx.theme().border)
            .bg(back)
            .overflow_hidden()
            .child(
                canvas(move |b, _, _| bounds.set(b), |_, _, _, _| {})
                    .absolute()
                    .size_full(),
            );
        if checker {
            // Squares behind a see-through page, as an editor shows transparency.
            let mut squares = div().absolute().size_full().flex().flex_wrap();
            for i in 0..600 {
                let dark = (i / 40 + i % 40) % 2 == 0;
                squares = squares.child(div().size(px(16.)).bg(if dark {
                    gpui_kit::rgb(0x2a2a2e)
                } else {
                    gpui_kit::rgb(0x3a3a3f)
                }));
            }
            stage = stage.child(squares);
        }
        if let (Some(scene), "scene") = (&self.scene, self.background) {
            stage = stage.child(
                img(ImageSource::Render(scene.clone()))
                    .absolute()
                    .size_full()
                    .object_fit(ObjectFit::Contain),
            );
        }
        stage = match &self.frame {
            Some(frame) if self.actual_size => stage.child(
                div()
                    .id("overlay-preview-scroll")
                    .absolute()
                    .size_full()
                    .overflow_scroll()
                    .child(
                        img(ImageSource::Render(frame.clone()))
                            .w(px(self.frame_size.0))
                            .h(px(self.frame_size.1)),
                    ),
            ),
            Some(frame) => stage.child(
                img(ImageSource::Render(frame.clone()))
                    .absolute()
                    .size_full()
                    .object_fit(ObjectFit::Contain),
            ),
            None => stage.child(
                div()
                    .absolute()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .p(px(SPACE_MD))
                    .child(super::page::guide(&self.preview_note, cx)),
            ),
        };
        let stage = stage
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|page, event: &MouseDownEvent, window, cx| {
                    page.focus.focus(window, cx);
                    page.send_input(
                        json!({"type": "mouse", "event": "mousePressed", "button": "left", "clickCount": event.click_count}),
                        Some(event.position),
                    );
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|page, event: &MouseUpEvent, _, _| {
                    page.send_input(
                        json!({"type": "mouse", "event": "mouseReleased", "button": "left", "clickCount": event.click_count}),
                        Some(event.position),
                    );
                }),
            )
            .on_scroll(cx.listener(|page, event: &ScrollEvent, _, _| {
                if page.actual_size {
                    return;
                }
                let delta = event.delta.pixel_delta(px(16.));
                page.send_input(
                    json!({"type": "wheel", "deltaX": -f32::from(delta.x), "deltaY": -f32::from(delta.y)}),
                    Some(event.position),
                );
            }));
        let mut tools = div().flex().flex_wrap().items_center().gap(px(SPACE_XS));
        for (index, (color, label)) in BACKGROUNDS.iter().enumerate() {
            let color: &'static str = color;
            tools = tools.child(
                Button::new(("preview-background", index))
                    .small()
                    .when(self.background != color, |b| b.ghost())
                    .label(tr(label))
                    .on_click(cx.listener(move |page, _, _, cx| {
                        page.background = color;
                        page.start_preview(false);
                        // Your scene: one picture, taken now.
                        if color == "scene" {
                            send(
                                json!({"request": "StreamerzPreview", "op": "scene", "width": page.model["width"],
                                "id": format!("{}:preview", page.route)}),
                            );
                        }
                        cx.notify();
                    })),
            );
        }
        tools = tools
            .child(
                Button::new("preview-fit")
                    .small()
                    .ghost()
                    .label(if self.actual_size {
                        tr("overlays.preview.fit")
                    } else {
                        tr("overlays.preview.actual")
                    })
                    .on_click(cx.listener(|page, _, _, cx| {
                        page.actual_size = !page.actual_size;
                        cx.notify();
                    })),
            )
            .child(
                Button::new("preview-reload")
                    .small()
                    .ghost()
                    .icon(Icon::empty().path("icons/refresh-cw.svg").size(px(12.)))
                    .tooltip(tr("overlays.preview.reload"))
                    .on_click(cx.listener(|page, _, _, cx| {
                        page.start_preview(true);
                        cx.notify();
                    })),
            );
        div()
            .id("overlay-preview")
            .size_full()
            .flex()
            .flex_col()
            .gap(px(SPACE_XS))
            .child(stage)
            .child(tools)
    }

    /// The tab's form (the first section with one), with Edit JSON and its own Save (or Send).
    fn form_block(&self, section: &Value, form: &Entity<Form>, cx: &mut Context<Self>) -> AnyElement {
        if section["form"]["fields"].as_array().is_some_and(|f| f.is_empty()) {
            return super::page::guide(section["form"]["empty"].as_str().unwrap_or_default(), cx).into_any_element();
        }
        let pending = form.read(cx).pending;
        let label = form.read(cx).model["okLabel"]
            .as_str()
            .map_or_else(|| tr("btn.save").to_owned(), str::to_owned);
        let (form_for_save, form_for_json) = (form.clone(), form.clone());
        div()
            .flex()
            .flex_col()
            .gap(px(SPACE_SM))
            .child(form.clone())
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(SPACE_SM))
                    .when(form.read(cx).model["json"] != false, |row| {
                        row.child(
                            Button::new("overlay-form-json")
                                .h(px(CONTROL_HEIGHT))
                                .ghost()
                                .icon(Icon::empty().path("icons/braces.svg"))
                                .label(tr("btn.edit_json"))
                                .disabled(pending)
                                .on_click(move |_, _, cx| form_for_json.update(cx, |form, cx| form.submit(true, cx))),
                        )
                    })
                    .child(
                        Button::new("overlay-form-save")
                            .h(px(CONTROL_HEIGHT))
                            .primary()
                            .label(label)
                            .disabled(pending)
                            .on_click(move |_, _, cx| form_for_save.update(cx, |form, cx| form.submit(false, cx))),
                    ),
            )
            .into_any_element()
    }

    /// A line of a section's list: what it is, a quiet detail, and its button.
    fn row(&self, at: usize, row: &Value, cx: &mut Context<Self>) -> AnyElement {
        let mut buttons = div().flex().flex_wrap().justify_end().gap(px(SPACE_XS)).flex_shrink_0();
        if row["button"].is_object() {
            buttons = buttons.child(self.button(1000 + at * 10, &row["button"], cx).small());
        }
        for (n, spec) in row["buttons"].as_array().into_iter().flatten().enumerate() {
            buttons = buttons.child(self.button(1000 + at * 10 + n + 1, spec, cx).small());
        }
        // A picture of the file, else its kind's icon.
        let lead = match row["picture"].as_str() {
            Some(path) => Some(
                img(std::path::PathBuf::from(path))
                    .size(px(40.))
                    .rounded(px(4.))
                    .object_fit(ObjectFit::Contain)
                    .flex_shrink_0()
                    .into_any_element(),
            ),
            None => row["icon"].as_str().map(|icon| {
                div()
                    .size(px(40.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        Icon::empty()
                            .path(format!("icons/{icon}.svg"))
                            .size_5()
                            .text_color(theme::icon_tone(cx)),
                    )
                    .into_any_element()
            }),
        };
        div()
            .id(("overlay-row", at))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(SPACE_SM))
            .px(px(SPACE_SM))
            .min_h(px(CONTROL_HEIGHT))
            .rounded(px(4.))
            .bg(theme::row_surface(cx))
            .py(px(SPACE_XS))
            .children(lead)
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .truncate()
                            .text_size(px(FONT_BODY))
                            .text_color(theme::strong(cx))
                            .child(text_of(&row["text"])),
                    )
                    .when(!text_of(&row["detail"]).is_empty(), |col| {
                        col.child(
                            div()
                                .truncate()
                                .text_size(px(FONT_CAPTION))
                                .text_color(theme::quiet(cx))
                                .child(text_of(&row["detail"])),
                        )
                    }),
            )
            .child(buttons)
            .into_any_element()
    }

    /// The tab's sections: a heading, a guide, the form, buttons and a list, each when it has one.
    fn tab_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut column = div().flex().flex_col().gap(px(SPACE_LG)).min_w_0();
        let mut form_drawn = false;
        let mut rows_seen = 0;
        let sections = self.model["sections"].as_array().cloned().unwrap_or_default();
        for (index, section) in sections.iter().enumerate() {
            let mut block = div().flex().flex_col().gap(px(SPACE_SM)).min_w_0();
            if let Some(title) = section["title"].as_str() {
                block = block.child(
                    div()
                        .text_size(px(theme::FONT_SECTION_TITLE))
                        .line_height(px(theme::LINE_SECTION_TITLE))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::strong(cx))
                        .child(title.to_owned()),
                );
            }
            if let Some(text) = section["text"].as_str() {
                block = block.child(super::page::guide(text, cx));
            }
            if section["form"].is_object() && !form_drawn {
                if let Some(form) = self.form.clone() {
                    block = block.child(self.form_block(section, &form, cx));
                    form_drawn = true;
                }
            }
            if let Some(buttons) = section["buttons"].as_array() {
                let mut row = div().flex().flex_wrap().gap(px(SPACE_SM));
                for (at, spec) in buttons.iter().enumerate() {
                    row = row.child(self.button(100 * (index + 1) + at, spec, cx));
                }
                block = block.child(row);
            }
            if let Some(rows) = section["rows"].as_array() {
                if rows.is_empty() {
                    block = block.child(super::page::guide(section["empty"].as_str().unwrap_or_default(), cx));
                }
                let mut list = div().flex().flex_col().gap(px(SPACE_XS));
                for row in rows {
                    list = list.child(self.row(rows_seen, row, cx));
                    rows_seen += 1;
                }
                block = block.child(list);
            }
            column = column.child(block);
        }
        column.into_any_element()
    }
}

impl OverlayPage {
    fn draft(&self) -> u64 {
        self.model["draft"].as_u64().unwrap_or(0)
    }

    /// Back to the list; with a draft, asks first: Save, Discard or Cancel.
    pub fn leave(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.draft() == 0 {
            cx.emit(OverlayEvent::Back);
            return;
        }
        let owner = cx.entity().downgrade();
        let (id, route) = (self.model["id"].clone(), self.route.clone());
        window.open_dialog(cx, move |dialog, window, cx| {
            let (save_owner, discard_owner) = (owner.clone(), owner.clone());
            let (save_id, discard_id, save_route, discard_route) =
                (id.clone(), id.clone(), route.clone(), route.clone());
            let buttons = div()
                .flex()
                .justify_end()
                .gap(px(SPACE_SM))
                .child(
                    Button::new("leave-cancel")
                        .label(tr("dialogs.cancel"))
                        .on_click(|_, window, cx| super::dialog::dismiss(window, cx)),
                )
                .child(
                    Button::new("leave-discard")
                        .label(tr("overlays.draft.discard"))
                        .on_click(move |_, window, cx| {
                            send(
                                json!({"request": "StreamerzPreview", "op": "revert", "itemId": discard_id,
                            "id": format!("{discard_route}:revert")}),
                            );
                            window.close_dialog(cx);
                            let _ = discard_owner.update(cx, |_, cx| cx.emit(OverlayEvent::Back));
                        }),
                )
                .child(
                    Button::new("leave-save")
                        .primary()
                        .label(tr("btn.save"))
                        .on_click(move |_, window, cx| {
                            send(json!({"request": "StreamerzPreview", "op": "save", "itemId": save_id,
                            "id": format!("{save_route}:revert")}));
                            window.close_dialog(cx);
                            let _ = save_owner.update(cx, |_, cx| cx.emit(OverlayEvent::Back));
                        }),
                );
            let footer = super::dialog::footer(None, buttons, cx);
            super::dialog::shell(dialog, tr("overlays.draft.leave_title"), 448., window, cx)
                .child(super::dialog::body(tr("overlays.draft.leave")))
                .footer(footer)
        });
    }

    /// While there is a draft: how many changes, Revert and Save; and Show on OBS while I edit.
    fn draft_bar(&self, cx: &mut Context<Self>) -> Div {
        let draft = self.draft();
        let live = self.model["live"] == true;
        let id = self.model["id"].clone();
        let route = self.route.clone();
        let mut bar = div().flex().flex_wrap().items_center().gap(px(SPACE_SM)).child(
            Switch::new("overlay-live-edit")
                .checked(live)
                .label(tr("overlays.draft.live"))
                .on_click({
                    let (id, route) = (id.clone(), route.clone());
                    move |checked: &bool, _, _| {
                        send(
                            json!({"request": "StreamerzPreview", "op": "live", "itemId": id, "on": checked,
                            "id": format!("{route}:live")}),
                        );
                    }
                }),
        );
        if draft > 0 && !live {
            let form = self.form.clone();
            bar = bar
                .child(
                    div()
                        .text_size(px(FONT_CAPTION))
                        .text_color(theme::quiet(cx))
                        .child(crate::i18n::trf(
                            "overlays.draft.count",
                            &[("count", &draft.to_string())],
                        )),
                )
                .child(
                    Button::new("overlay-revert")
                        .h(px(CONTROL_HEIGHT))
                        .icon(Icon::empty().path("icons/rotate-ccw.svg").size(px(13.)))
                        .label(tr("overlays.draft.revert"))
                        .on_click(move |_, _, _| {
                            send(json!({"request": "StreamerzPreview", "op": "revert", "itemId": id,
                                "id": format!("{route}:revert")}));
                        }),
                )
                .child(
                    Button::new("overlay-save")
                        .h(px(CONTROL_HEIGHT))
                        .primary()
                        .label(tr("btn.save"))
                        .on_click(move |_, _, cx| {
                            if let Some(form) = &form {
                                form.update(cx, |form, cx| form.submit(false, cx));
                            }
                        }),
                );
        }
        bar
    }
}

impl Render for OverlayPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let viewport = f32::from(window.viewport_size().width);
        let wide = viewport - theme::SIDEBAR_WIDTH - 2. * theme::PAGE_PAD_X >= WIDE;
        let title = self.model["title"].as_str().unwrap_or_default().to_owned();
        let subtitle = self.model["subtitle"].as_str().unwrap_or_default().to_owned();
        let back = Button::new("overlay-back")
            .small()
            .ghost()
            .icon(Icon::empty().path("icons/arrow-left.svg").size(px(13.)))
            .label(self.model["back"].as_str().unwrap_or_default().to_owned())
            .on_click(cx.listener(|page, _, window, cx| page.leave(window, cx)));
        let owner = cx.entity().downgrade();
        let tabs = super::tabs::bar(&self.model["tabs"], window, cx, move |kind, _, cx| {
            let _ = owner.update(cx, |_, cx| cx.emit(OverlayEvent::Tab(kind)));
        });
        let header = super::page::header(Some("layers"), title, Some(self.toolbar(cx).into_any_element()), cx);
        let panel = div()
            .id("overlay-tab-body")
            .flex_1()
            .min_h_0()
            .overflow_y_scrollbar()
            .child(self.tab_body(cx));
        let tabs_column = div()
            .flex()
            .flex_col()
            .gap(px(SPACE_MD))
            .min_w_0()
            .min_h_0()
            .flex_1()
            .child(self.draft_bar(cx))
            .child(tabs)
            .child(panel);
        let content = if wide {
            div()
                .flex()
                .gap(px(SPACE_MD))
                .flex_1()
                .min_h_0()
                .child(div().w(relative(0.6)).flex_shrink_0().child(self.preview(cx)))
                .child(tabs_column)
        } else {
            div()
                .flex()
                .flex_col()
                .gap(px(SPACE_MD))
                .flex_1()
                .min_h_0()
                .child(div().h(relative(0.4)).flex_shrink_0().child(self.preview(cx)))
                .child(tabs_column)
        };
        div()
            .id("overlay-page")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|page, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" {
                    page.leave(window, cx);
                }
            }))
            .size_full()
            .flex()
            .flex_col()
            .gap(px(SPACE_MD))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(SPACE_SM))
                    .child(back)
                    .when(!subtitle.is_empty(), |row| {
                        row.child(
                            div()
                                .text_size(px(FONT_CAPTION))
                                .text_color(theme::quiet(cx))
                                .child(subtitle),
                        )
                    }),
            )
            .child(header)
            .child(content)
    }
}
