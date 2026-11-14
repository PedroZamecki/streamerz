//! Pick one item by its picture: a field that shows the chosen item (picture, name and a quiet detail, such as
//! a gift's price) and opens a searchable grid of them, with "any" first. The core sends the items
//! (`choices` with `picture`, `detail` and `tip`) and what the field says (`gallery`); the grid is
//! virtualised and asks the core for the pictures of the rows in view only. Ids stay out of sight.
use crate::send;
use gpui_kit::component::{
    button::{Button, ButtonVariants},
    input::{Input, InputEvent, InputState},
    popover::Popover,
    tooltip::Tooltip,
    ActiveTheme, Disableable, Icon, Sizable,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::path::PathBuf;

/// Cells per row, a cell's size and its picture's.
const COLUMNS: usize = 6;
const CELL_W: f32 = 78.;
const CELL_H: f32 = 92.;
const PICTURE: f32 = 40.;
/// Pictures asked for in one request.
const BATCH: usize = 48;

struct Item {
    id: String,
    label: String,
    picture: String,
    detail: String,
    tip: String,
}

fn items_of(choices: &Value) -> Vec<Item> {
    let text = |choice: &Value, key: &str| choice[key].as_str().unwrap_or_default().to_owned();
    choices
        .as_array()
        .into_iter()
        .flatten()
        .map(|choice| Item {
            id: text(choice, "id"),
            label: text(choice, "label"),
            picture: text(choice, "picture"),
            detail: text(choice, "detail"),
            tip: text(choice, "tip"),
        })
        .collect()
}

pub struct Gallery {
    items: Vec<Item>,
    /// What the field says (`any`, `anyTip`, `search`, `empty`, `detailIcon`, `status`, `current`).
    spec: Value,
    /// The chosen id ("" for any).
    pub selected: String,
    open: bool,
    search: Entity<InputState>,
    /// The items the search keeps, by index.
    shown: Vec<usize>,
    images: HashMap<String, PathBuf>,
    missing: HashSet<String>,
    asking: bool,
    /// The request id pictures come back with.
    images_id: String,
    /// Reload, as the core described it (`request`, `label`), and whether it is on its way.
    refresh: Value,
    refresh_id: String,
    pub loading: bool,
    scroll: UniformListScrollHandle,
    id: SharedString,
    _subscription: Subscription,
}

impl Gallery {
    /// `route` names this field's replies (`<route>:images` and `<route>:choices`).
    pub fn new(field: &Value, route: String, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let spec = field["gallery"].clone();
        let hint = spec["search"].as_str().unwrap_or_default().to_owned();
        let search = cx.new(|cx| InputState::new(window, cx).placeholder(hint));
        let subscription = cx.subscribe(&search, |gallery, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                gallery.filter(cx);
                gallery.scroll.scroll_to_item(0, ScrollStrategy::Top);
                cx.notify();
            }
        });
        let mut gallery = Self {
            items: items_of(&field["choices"]),
            spec,
            selected: field["value"].as_str().unwrap_or_default().trim().to_owned(),
            open: false,
            search,
            shown: Vec::new(),
            images: HashMap::new(),
            missing: HashSet::new(),
            asking: false,
            images_id: format!("{route}:images"),
            refresh: field["refresh"].clone(),
            refresh_id: format!("{route}:choices"),
            loading: false,
            scroll: UniformListScrollHandle::new(),
            id: SharedString::from(route),
            _subscription: subscription,
        };
        gallery.filter(cx);
        // No list yet: load it as the form opens.
        if gallery.refresh["auto"] == true {
            gallery.reload(cx);
        }
        gallery
    }

    fn filter(&mut self, cx: &App) {
        let query = self.search.read(cx).value().trim().to_lowercase();
        self.shown = (0..self.items.len())
            .filter(|at| query.is_empty() || self.items[*at].label.to_lowercase().contains(&query))
            .collect();
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        let mut request = self.refresh["request"].clone();
        if !request.is_object() || self.loading {
            return;
        }
        request["id"] = self.refresh_id.clone().into();
        send(request);
        self.loading = true;
        cx.notify();
    }

    /// The core's reloaded list (`choices`, `status`), or why it failed.
    pub fn set_choices(&mut self, reply: &Value, cx: &mut Context<Self>) {
        self.loading = false;
        if reply["status"] == "error" {
            self.spec["status"] = reply["error"].clone();
        } else {
            self.items = items_of(&reply["choices"]);
            self.spec["status"] = reply["status"].clone();
            // A trigger saved with a gift the old list didn't have may be named now.
            if self.items.iter().any(|item| item.id == self.selected) {
                self.spec["current"] = Value::Null;
            }
        }
        self.filter(cx);
        cx.notify();
    }

    /// Pictures the grid asked for.
    pub fn images(&mut self, reply: &Value, cx: &mut Context<Self>) {
        self.asking = false;
        for (url, file) in reply["files"].as_object().into_iter().flatten() {
            if let Some(file) = file.as_str() {
                self.images.insert(url.clone(), PathBuf::from(file));
                self.missing.remove(url);
            }
        }
        cx.notify();
    }

    /// The core downloaded pictures: ask again for the ones still missing.
    pub fn images_arrived(&mut self) {
        self.asking = false;
        self.ask_images();
    }

    fn ask_images(&mut self) {
        if self.asking || self.missing.is_empty() {
            return;
        }
        let urls: Vec<&String> = self.missing.iter().take(BATCH).collect();
        self.asking = true;
        send(json!({"request": "StreamerzChat", "op": "images", "urls": urls, "id": self.images_id}));
    }

    fn want(&mut self, url: &str) {
        if url.starts_with("http") && !self.images.contains_key(url) {
            self.missing.insert(url.to_owned());
        }
    }

    /// The icon drawn for "any" and for a picture not here yet (`gallery.icon`, else an image).
    fn icon(&self) -> String {
        format!("icons/{}.svg", self.spec["icon"].as_str().unwrap_or("image"))
    }

    fn pick(&mut self, id: String, cx: &mut Context<Self>) {
        self.selected = id;
        self.open = false;
        cx.notify();
    }

    fn picture(&self, url: &str, size: f32, cx: &App) -> AnyElement {
        match self.images.get(url) {
            Some(file) => img(file.clone())
                .size(px(size))
                .flex_shrink_0()
                .object_fit(ObjectFit::Contain)
                .into_any_element(),
            None => Icon::empty()
                .path(self.icon())
                .size(px(size * 0.6))
                .text_color(cx.theme().muted_foreground)
                .into_any_element(),
        }
    }

    /// The closed field: the chosen item's picture, name and detail, opening the grid.
    pub fn field(this: &Entity<Self>, label: String, disabled: bool, cx: &mut App) -> AnyElement {
        let gallery = this.read(cx);
        let theme = cx.theme();
        let chosen = gallery.items.iter().find(|item| item.id == gallery.selected);
        let mut line = div().flex().items_center().gap_2().w_full().min_w_0();
        line = match chosen {
            Some(item) => {
                let url = item.picture.clone();
                line.child(gallery.picture(&url, 18., cx))
                    .child(div().min_w_0().truncate().child(item.label.clone()))
                    .when(!item.detail.is_empty(), |line| {
                        line.child(gallery.detail(&item.detail, cx))
                    })
            }
            None if gallery.selected.is_empty() => line
                .child(
                    Icon::empty()
                        .path(gallery.icon())
                        .size_4()
                        .text_color(theme.muted_foreground),
                )
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_color(theme.muted_foreground)
                        .child(gallery.spec["any"].as_str().unwrap_or_default().to_owned()),
                ),
            // Saved, but not in the list (yet): its saved name.
            None => line
                .child(
                    Icon::empty()
                        .path(gallery.icon())
                        .size_4()
                        .text_color(theme.muted_foreground),
                )
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .child(gallery.spec["current"].as_str().unwrap_or(&gallery.selected).to_owned()),
                ),
        };
        let line = line.child(div().flex_1()).child(
            Icon::empty()
                .path("icons/chevron-down.svg")
                .size_4()
                .text_color(theme.muted_foreground),
        );
        let (open, id) = (gallery.open, gallery.id.clone());
        let (owner, panel) = (this.downgrade(), this.clone());
        Popover::new(SharedString::from(format!("{id}-popover")))
            .open(open && !disabled)
            .trigger_style(StyleRefinement::default().w_full())
            .on_open_change(move |open, window, cx| {
                let _ = owner.update(cx, |gallery, cx| {
                    gallery.open = *open;
                    if *open {
                        gallery.search.update(cx, |search, cx| search.focus(window, cx));
                    }
                    cx.notify();
                });
            })
            .trigger(
                Button::new(SharedString::from(format!("{id}-field")))
                    .small()
                    .outline()
                    .w_full()
                    .h(px(26.))
                    .accessibility_label(label)
                    .disabled(disabled)
                    .child(line),
            )
            .content(move |_, _, _| panel.clone())
            .into_any_element()
    }

    fn detail(&self, detail: &str, cx: &App) -> Div {
        let theme = cx.theme();
        div()
            .flex()
            .items_center()
            .gap(px(3.))
            .flex_shrink_0()
            .text_size(px(11.5))
            .text_color(theme.muted_foreground)
            .when_some(self.spec["detailIcon"].as_str(), |row, icon| {
                row.child(Icon::empty().path(format!("icons/{icon}.svg")).size_3())
            })
            .child(detail.to_owned())
    }

    /// One cell of the grid: the picture, the name and the detail; the tip on hover.
    #[allow(clippy::too_many_arguments)]
    fn cell(
        &self,
        key: usize,
        id: String,
        picture: AnyElement,
        label: String,
        detail: Option<Div>,
        tip: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme();
        let chosen = id == self.selected;
        let tip = SharedString::from(tip);
        div()
            .id(("gallery-cell", key))
            .w(px(CELL_W))
            .h(px(CELL_H))
            .p_1()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(3.))
            .rounded(theme.radius)
            .border_1()
            .border_color(if chosen { theme.ring } else { gpui::transparent_black() })
            .when(chosen, |cell| cell.bg(theme.accent))
            .cursor_pointer()
            .hover(|style| style.bg(theme.accent))
            .when(!tip.is_empty(), |cell| {
                cell.tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
            })
            .on_click(cx.listener(move |gallery, _, _, cx| gallery.pick(id.clone(), cx)))
            .child(
                div()
                    .size(px(PICTURE))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(picture),
            )
            .child(
                div()
                    .w_full()
                    .h(px(16.))
                    .flex_shrink_0()
                    .line_height(px(16.))
                    .text_center()
                    .truncate()
                    .text_size(px(11.5))
                    .text_color(theme.foreground)
                    .child(label),
            )
            .children(detail.map(|detail| detail.h(px(14.)).flex_shrink_0()))
            .into_any_element()
    }
}

impl Render for Gallery {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let tip = self.refresh["tip"]
            .as_str()
            .or(self.refresh["label"].as_str())
            .unwrap_or_default()
            .to_owned();
        let header = div()
            .flex()
            .items_center()
            .gap(px(4.))
            .child(
                div().flex_1().min_w_0().child(
                    Input::new(&self.search)
                        .small()
                        .prefix(
                            Icon::empty()
                                .path("icons/search.svg")
                                .small()
                                .text_color(theme.muted_foreground),
                        )
                        .aria_label(self.spec["search"].as_str().unwrap_or_default().to_owned()),
                ),
            )
            .when(self.refresh.is_object(), |header| {
                header.child(
                    Button::new("gallery-refresh")
                        .small()
                        .ghost()
                        .icon(Icon::empty().path(if self.loading {
                            "icons/loader-circle.svg"
                        } else {
                            "icons/refresh-cw.svg"
                        }))
                        .tooltip(tip.clone())
                        .accessibility_label(self.refresh["label"].as_str().unwrap_or_default().to_owned())
                        .disabled(self.loading)
                        .on_click(cx.listener(|gallery, _, _, cx| gallery.reload(cx))),
                )
            });
        let mut panel = div()
            .id("gallery")
            .w(px(CELL_W * COLUMNS as f32 + 8.))
            .flex()
            .flex_col()
            .gap_2()
            .child(header);
        // Loading, failed or empty, said in place of the grid (what is known still shows).
        let status = self.spec["status"].as_str().filter(|_| self.items.is_empty());
        if let Some(status) = status {
            panel = panel.child(
                div()
                    .h(px(CELL_H * 2.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .p_3()
                    .text_center()
                    .text_size(px(12.5))
                    .text_color(theme.muted_foreground)
                    .child(status.to_owned()),
            );
            return panel.into_any_element();
        }
        let searching = !self.search.read(cx).value().trim().is_empty();
        // "Any" leads the grid, unless a search is narrowing it.
        let any = usize::from(!searching);
        let count = any + self.shown.len();
        if count == 0 {
            return panel
                .child(
                    div()
                        .h(px(CELL_H * 2.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(12.5))
                        .text_color(theme.muted_foreground)
                        .child(self.spec["empty"].as_str().unwrap_or_default().to_owned()),
                )
                .into_any_element();
        }
        let rows = count.div_ceil(COLUMNS);
        let list = uniform_list(
            "gallery-rows",
            rows,
            cx.processor(move |gallery, range: Range<usize>, _, cx| {
                for at in range.clone().flat_map(|row| row * COLUMNS..(row + 1) * COLUMNS) {
                    if let Some(index) = at.checked_sub(any).and_then(|at| gallery.shown.get(at)) {
                        let url = gallery.items[*index].picture.clone();
                        gallery.want(&url);
                    }
                }
                gallery.ask_images();
                range
                    .map(|row| {
                        let mut line = div().h(px(CELL_H)).flex();
                        for at in row * COLUMNS..((row + 1) * COLUMNS).min(count) {
                            let cell = if at < any {
                                let icon = Icon::empty()
                                    .path(gallery.icon())
                                    .size(px(PICTURE * 0.6))
                                    .text_color(cx.theme().muted_foreground)
                                    .into_any_element();
                                gallery.cell(
                                    at,
                                    String::new(),
                                    icon,
                                    gallery.spec["any"].as_str().unwrap_or_default().to_owned(),
                                    None,
                                    gallery.spec["anyTip"].as_str().unwrap_or_default().to_owned(),
                                    cx,
                                )
                            } else {
                                let item = &gallery.items[gallery.shown[at - any]];
                                let (id, label, tip) = (item.id.clone(), item.label.clone(), item.tip.clone());
                                let picture = gallery.picture(&item.picture, PICTURE, cx);
                                let detail = (!item.detail.is_empty()).then(|| gallery.detail(&item.detail, cx));
                                gallery.cell(at, id, picture, label, detail, tip, cx)
                            };
                            line = line.child(cell);
                        }
                        line.into_any_element()
                    })
                    .collect()
            }),
        )
        .track_scroll(&self.scroll)
        .h(px(CELL_H * (rows.min(4) as f32)));
        panel.child(list).into_any_element()
    }
}
