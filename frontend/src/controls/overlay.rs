//! An overlay's own page, opened from the Overlays list: Back to the list, its toolbar, the preview and
//! its tabs (Settings, Test, Media, Live, Alerts for an alert box, Code). The core gives the model (the
//! `overlay` of the Overlays read); every change is a core request, as everywhere else.
use super::form::Form;
use crate::i18n::tr;
use crate::send;
use crate::theme::{
    self, CONTROL_HEIGHT, FONT_BODY, FONT_CAPTION, LINE_BODY, LINE_CAPTION, SPACE_MD, SPACE_SM, SPACE_XS,
};
use gpui_kit::component::{
    button::{Button, ButtonVariants},
    scroll::ScrollableElement,
    ActiveTheme, Disableable, Icon, Sizable, WindowExt,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use serde_json::{json, Value};

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
    /// A form asked by a toolbar button (Add to OBS, Export), shown as a dialog.
    dialog: Option<Entity<Form>>,
    focus: FocusHandle,
}

impl EventEmitter<OverlayEvent> for OverlayPage {}

impl OverlayPage {
    pub fn new(model: Value, route: String, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut page = Self {
            model: Value::Null,
            route,
            form: None,
            form_kind: String::new(),
            dialog: None,
            focus: cx.focus_handle(),
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
        let form = &model["body"]["form"];
        let kind = form["kind"].as_str().unwrap_or_default().to_owned();
        // A form being typed in stays as it is; another tab or overlay brings its own.
        if kind.is_empty() {
            self.form = None;
            self.form_kind.clear();
        } else if kind != self.form_kind || self.form.is_none() {
            let route = format!("{}:save", self.route);
            let value = form.clone();
            self.form = Some(cx.new(|cx| Form::new(value, route, window, cx)));
            self.form_kind = kind;
        }
        self.model = model;
        cx.notify();
    }

    /// The core's answer to one of this page's requests; true when the page should be read again.
    pub fn apply(&mut self, operation: &str, value: Value, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if let Some(err) = crate::error::StreamerzError::from_response(&value) {
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
    fn press(&self, button: &Value) {
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
            .on_click(cx.listener(move |page, _, _, _| page.press(&spec_for_click)))
    }

    fn toolbar(&self, cx: &mut Context<Self>) -> Div {
        let mut bar = div().flex().flex_wrap().items_center().gap(px(SPACE_SM));
        for (index, spec) in self.model["toolbar"].as_array().into_iter().flatten().enumerate() {
            bar = bar.child(self.button(index, spec, cx));
        }
        bar
    }

    /// The overlay as OBS shows it. Until the live preview is on screen, the page's size and address.
    fn preview(&self, cx: &App) -> Stateful<Div> {
        let (width, height) = (
            self.model["width"].as_u64().unwrap_or(1920),
            self.model["height"].as_u64().unwrap_or(1080),
        );
        div()
            .id("overlay-preview")
            .size_full()
            .min_h(px(160.))
            .rounded(px(6.))
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().table)
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(SPACE_XS))
            .p(px(SPACE_MD))
            .child(
                Icon::empty()
                    .path("icons/layers.svg")
                    .size_6()
                    .text_color(theme::icon_tone(cx)),
            )
            .child(
                div()
                    .text_size(px(FONT_BODY))
                    .line_height(px(LINE_BODY))
                    .text_color(theme::strong(cx))
                    .child(format!("{width} x {height}")),
            )
            .child(
                div()
                    .max_w_full()
                    .truncate()
                    .text_size(px(FONT_CAPTION))
                    .line_height(px(LINE_CAPTION))
                    .text_color(theme::quiet(cx))
                    .child(self.model["url"].as_str().unwrap_or_default().to_owned()),
            )
    }

    fn tab_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let body = &self.model["body"];
        let mut column = div().flex().flex_col().gap(px(SPACE_MD)).min_w_0();
        if let Some(text) = body["text"].as_str() {
            column = column.child(super::page::guide(text, cx));
        }
        if let Some(form) = &self.form {
            if self.model["body"]["form"]["fields"]
                .as_array()
                .is_some_and(|f| f.is_empty())
            {
                column = column.child(super::page::guide(
                    self.model["body"]["form"]["empty"].as_str().unwrap_or_default(),
                    cx,
                ));
            } else {
                let pending = form.read(cx).pending;
                let form_for_save = form.clone();
                let form_for_json = form.clone();
                column = column.child(form.clone()).child(
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
                                    .on_click(move |_, _, cx| {
                                        form_for_json.update(cx, |form, cx| form.submit(true, cx))
                                    }),
                            )
                        })
                        .child(
                            Button::new("overlay-form-save")
                                .h(px(CONTROL_HEIGHT))
                                .primary()
                                .label(tr("btn.save"))
                                .disabled(pending)
                                .on_click(move |_, _, cx| form_for_save.update(cx, |form, cx| form.submit(false, cx))),
                        ),
                );
            }
        }
        if let Some(files) = body["files"].as_array() {
            if files.is_empty() {
                column = column.child(super::page::guide(body["empty"].as_str().unwrap_or_default(), cx));
            }
            for (index, file) in files.iter().enumerate() {
                column = column.child(
                    div()
                        .id(("overlay-file", index))
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(SPACE_SM))
                        .px(px(SPACE_SM))
                        .h(px(CONTROL_HEIGHT))
                        .rounded(px(4.))
                        .bg(theme::row_surface(cx))
                        .child(
                            div()
                                .min_w_0()
                                .truncate()
                                .text_size(px(FONT_BODY))
                                .text_color(theme::strong(cx))
                                .child(file["name"].as_str().unwrap_or_default().to_owned()),
                        )
                        .child(
                            div()
                                .flex_shrink_0()
                                .text_size(px(FONT_CAPTION))
                                .text_color(theme::quiet(cx))
                                .child(file["size"].as_str().unwrap_or_default().to_owned()),
                        ),
                );
            }
        }
        if let Some(buttons) = body["buttons"].as_array() {
            let mut row = div().flex().flex_wrap().gap(px(SPACE_SM));
            for (index, spec) in buttons.iter().enumerate() {
                row = row.child(self.button(100 + index, spec, cx));
            }
            column = column.child(row);
        }
        column.into_any_element()
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
            .on_click(cx.listener(|_, _, _, cx| cx.emit(OverlayEvent::Back)));
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
            .on_key_down(cx.listener(|_, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" {
                    cx.emit(OverlayEvent::Back);
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
