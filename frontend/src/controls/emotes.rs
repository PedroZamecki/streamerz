//! The emote and emoji picker: a tab per source, a search across them, and a grid of pictures.
//!
//! The core decides the tabs and what each holds (`StreamerzChat` op `emotes`); this draws them. The
//! grid is virtualised, and asks the core for the pictures of the rows in view only. A click picks
//! (Shift keeps the picker open to pick several), a right-click keeps or drops a favorite.
use crate::send;
use gpui_kit::component::{
    input::{Input, InputEvent, InputState},
    tab::{Tab, TabBar},
    ActiveTheme, Icon, Sizable,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::path::PathBuf;

/// Emotes per row, and the size of a cell and of its picture.
const COLUMNS: usize = 10;
const CELL: f32 = 40.;
const PICTURE: f32 = 28.;
/// Pictures asked for in one request.
const BATCH: usize = 48;

/// An emote was picked: its name, picture URL, and whether the picker stays open for more.
pub struct Picked(pub String, pub String, pub bool);
impl EventEmitter<Picked> for EmotePicker {}

enum Row {
    Head {
        title: String,
        open: bool,
        count: usize,
        note: Option<String>,
    },
    Cells(Vec<(String, String)>),
}

pub struct EmotePicker {
    platform: String,
    provider: String,
    data: Value,
    rows: Vec<Row>,
    folded: HashSet<String>,
    favorites: HashSet<String>,
    input: Entity<InputState>,
    images: HashMap<String, PathBuf>,
    missing: HashSet<String>,
    asking: bool,
    error: String,
    scroll: UniformListScrollHandle,
    _subscriptions: Vec<Subscription>,
}

impl EmotePicker {
    pub fn new(platform: &str, placeholder: String, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
        let subscriptions = vec![cx.subscribe_in(&input, window, |view, _, event: &InputEvent, _, cx| {
            if matches!(event, InputEvent::Change) {
                view.request(cx);
            }
        })];
        let view = Self {
            platform: platform.to_owned(),
            provider: String::new(),
            data: Value::Null,
            rows: Vec::new(),
            folded: HashSet::new(),
            favorites: HashSet::new(),
            input,
            images: HashMap::new(),
            missing: HashSet::new(),
            asking: false,
            error: String::new(),
            scroll: UniformListScrollHandle::new(),
            _subscriptions: subscriptions,
        };
        view.request(cx);
        view
    }

    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |input, cx| input.focus(window, cx));
    }

    /// The downloaded file for an emote's picture URL, if cached.
    pub fn image_file(&self, url: &str) -> Option<&PathBuf> {
        self.images.get(url)
    }

    /// Ask the core for the open tab, or for a search across them.
    fn request(&self, cx: &App) {
        send(
            json!({"request": "StreamerzChat", "op": "emotes", "platform": self.platform, "provider": self.provider,
            "query": self.input.read(cx).value().to_string(), "id": "chat:emotes"}),
        );
    }

    /// The core's answer: tabs and sections, laid out in rows.
    pub fn apply(&mut self, value: Value, cx: &mut Context<Self>) {
        if value["status"] == "error" {
            self.error = value["error"].as_str().unwrap_or_default().to_owned();
            self.data = Value::Null;
            self.rows.clear();
            cx.notify();
            return;
        }
        self.error.clear();
        let data = value["emotes"].clone();
        // A search that is no longer what is typed is an old answer.
        if data["query"].as_str().unwrap_or_default() != self.input.read(cx).value().trim().to_lowercase() {
            return;
        }
        self.provider = data["active"].as_str().unwrap_or_default().to_owned();
        self.favorites = data["favorites"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|name| name.as_str().map(str::to_owned))
            .collect();
        self.data = data;
        self.layout();
        cx.notify();
    }

    fn layout(&mut self) {
        self.rows.clear();
        for section in self.data["sections"].as_array().into_iter().flatten() {
            let title = section["title"].as_str().unwrap_or_default().to_owned();
            let open = !self.folded.contains(&title);
            let emotes: Vec<(String, String)> = section["emotes"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|emote| Some((emote[0].as_str()?.to_owned(), emote[1].as_str()?.to_owned())))
                .collect();
            self.rows.push(Row::Head {
                title,
                open,
                count: section["count"].as_u64().unwrap_or(emotes.len() as u64) as usize,
                note: section["note"].as_str().map(str::to_owned),
            });
            if open {
                self.rows
                    .extend(emotes.chunks(COLUMNS).map(|chunk| Row::Cells(chunk.to_vec())));
            }
        }
    }

    /// Pictures the page asked for arrived (or the core has more of them now).
    pub fn images_arrived(&mut self, cx: &mut Context<Self>) {
        // A catalog still loading fills the tab when it lands.
        if self.rows.iter().all(|row| matches!(row, Row::Head { .. })) {
            self.request(cx);
        }
        self.ask_images();
    }

    pub fn images(&mut self, value: &Value, cx: &mut Context<Self>) {
        self.asking = false;
        for (url, file) in value["files"].as_object().into_iter().flatten() {
            if let Some(file) = file.as_str() {
                self.images.insert(url.clone(), PathBuf::from(file));
                self.missing.remove(url);
            }
        }
        cx.notify();
    }

    fn ask_images(&mut self) {
        if self.asking || self.missing.is_empty() {
            return;
        }
        let urls: Vec<&String> = self.missing.iter().take(BATCH).collect();
        self.asking = true;
        send(json!({"request": "StreamerzChat", "op": "images", "urls": urls, "id": "chat:emoteimages"}));
    }

    /// Note the pictures of the rows in view that are not here yet.
    fn want(&mut self, range: Range<usize>) {
        for row in self.rows.get(range).into_iter().flatten() {
            if let Row::Cells(cells) = row {
                for (_, url) in cells {
                    if url.starts_with("http") && !self.images.contains_key(url) {
                        self.missing.insert(url.clone());
                    }
                }
            }
        }
        self.ask_images();
    }
}

impl Render for EmotePicker {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let tabs: Vec<(String, String)> = self.data["tabs"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|tab| {
                (
                    tab["id"].as_str().unwrap_or_default().to_owned(),
                    tab["label"].as_str().unwrap_or_default().to_owned(),
                )
            })
            .collect();
        let searching = !self.input.read(cx).value().trim().is_empty();
        let active = tabs.iter().position(|(id, _)| *id == self.provider).unwrap_or(0);
        let ids: Vec<String> = tabs.iter().map(|(id, _)| id.clone()).collect();
        let mut body = div().flex().flex_col().gap_2().w(px(CELL * COLUMNS as f32 + 20.));
        if !tabs.is_empty() {
            body = body.child(
                TabBar::new("emote-tabs")
                    .segmented()
                    .small()
                    .selected_index(if searching { usize::MAX } else { active })
                    .children(
                        tabs.iter()
                            .map(|(_, label)| Tab::new().aria_label(label.clone()).label(label.clone())),
                    )
                    .on_click(cx.listener(move |view, index: &usize, window, cx| {
                        if let Some(id) = ids.get(*index) {
                            view.provider = id.clone();
                            // Clearing a search asks by itself; otherwise ask for the tab.
                            if view.input.read(cx).value().is_empty() {
                                view.request(cx);
                            } else {
                                view.input.update(cx, |input, cx| input.set_value("", window, cx));
                            }
                        }
                    })),
            );
        }
        body = body.child(
            Input::new(&self.input)
                .small()
                .aria_label(self.data["search"].as_str().unwrap_or_default().to_owned()),
        );
        body = body.child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(self.data["help"].as_str().unwrap_or_default().to_owned()),
        );
        if !self.error.is_empty() {
            body = body.child(super::notice::error(&self.error, cx));
        }
        if self.rows.is_empty() && self.data != Value::Null {
            return body
                .child(
                    div()
                        .h(px(CELL * 6.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(theme.muted_foreground)
                        .child(self.data["empty"].as_str().unwrap_or_default().to_owned()),
                )
                .into_any_element();
        }
        let list = uniform_list(
            "emote-rows",
            self.rows.len(),
            cx.processor(move |view, range: Range<usize>, _, cx| {
                view.want(range.clone());
                let theme = cx.theme().clone();
                range
                    .filter_map(|at| view.rows.get(at).map(|row| (at, row)))
                    .map(|(at, row)| match row {
                        Row::Head { title, open, count, note } => {
                            let folded = title.clone();
                            div()
                                .id(("emote-head", at))
                                .h(px(CELL))
                                .flex()
                                .items_center()
                                .gap_2()
                                .cursor_pointer()
                                .on_click(cx.listener(move |view, _, _, cx| {
                                    if !view.folded.remove(&folded) {
                                        view.folded.insert(folded.clone());
                                    }
                                    view.layout();
                                    cx.notify();
                                }))
                                .child(
                                    Icon::empty()
                                        .path(if *open { "icons/chevron-down.svg" } else { "icons/chevron-right.svg" })
                                        .size_4()
                                        .text_color(theme.muted_foreground),
                                )
                                .child(div().font_weight(FontWeight::BOLD).child(title.clone()))
                                .child(div().text_xs().text_color(theme.muted_foreground).child(count.to_string()))
                                .when_some(note.clone(), |head, note| {
                                    head.child(div().text_xs().min_w_0().truncate().text_color(theme.muted_foreground).child(note))
                                })
                                .into_any_element()
                        }
                        Row::Cells(cells) => div()
                            .h(px(CELL))
                            .flex()
                            .children(cells.iter().enumerate().map(|(column, (name, url))| {
                                let favorite = view.favorites.contains(name);
                                let (pick, picked_url, keep, star) = (name.clone(), url.clone(), (name.clone(), url.clone()), favorite);
                                let picture = view.images.get(url).map(|file| {
                                    img(file.clone()).w(px(PICTURE)).h(px(PICTURE)).object_fit(ObjectFit::Contain)
                                });
                                div()
                                    .id(("emote", at * COLUMNS + column))
                                    .w(px(CELL))
                                    .h(px(CELL))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(theme.radius)
                                    .cursor_pointer()
                                    .hover(|style| style.bg(theme.accent))
                                    .tooltip({
                                        let tip = SharedString::from(name.clone());
                                        move |window, cx| gpui_kit::component::tooltip::Tooltip::new(tip.clone()).build(window, cx)
                                    })
                                    .when(star, |cell| cell.border_1().border_color(theme.primary))
                                    .on_click(cx.listener(move |_, event: &ClickEvent, _, cx| {
                                        cx.emit(Picked(pick.clone(), picked_url.clone(), event.modifiers().shift));
                                    }))
                                    .on_mouse_down(
                                        MouseButton::Right,
                                        cx.listener(move |view, _, _, cx| {
                                            send(json!({"request": "StreamerzChat", "op": "favorite", "name": keep.0, "url": keep.1, "id": "chat:favorite"}));
                                            if view.favorites.contains(&keep.0) {
                                                view.favorites.remove(&keep.0);
                                            } else {
                                                view.favorites.insert(keep.0.clone());
                                            }
                                            cx.notify();
                                        }),
                                    )
                                    .child(match picture {
                                        Some(picture) => picture.into_any_element(),
                                        None => div()
                                            .text_xs()
                                            .text_color(theme.muted_foreground)
                                            .child(name.chars().take(3).collect::<String>())
                                            .into_any_element(),
                                    })
                            }))
                            .into_any_element(),
                    })
                    .collect()
            }),
        )
        .track_scroll(&self.scroll)
        .h(px(CELL * 8.));
        body.child(list).into_any_element()
    }
}
