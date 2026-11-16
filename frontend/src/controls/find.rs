//! Finding a channel: a search box, the channels that match with their picture, followers
//! and whether they are live, as the platforms' own sites show them, and "Use my own channel". A link or an
//! @name typed in the box works too (for what can't be searched). The core supplies the model (`finder`),
//! searches (`StreamerzSearchChannels`) and reads the pick (`<platform>.set_channel`, `.own_channel`).
use crate::send;
use crate::theme::{CONTROL_HEIGHT, FONT_CAPTION, LINE_CAPTION, SPACE_LG, SPACE_MD, SPACE_SM, SPACE_XS};
use gpui_kit::component::{
    button::{Button, ButtonVariants},
    input::{Input, InputEvent, InputState},
    ActiveTheme, Disableable, Icon, Sizable, WindowExt,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;

/// How long typing rests before the search is asked.
const REST: std::time::Duration = std::time::Duration::from_millis(350);

pub struct Finder {
    model: Value,
    /// Where the answers to its requests come back, as an id prefix.
    route: String,
    input: Entity<InputState>,
    rows: Vec<Value>,
    selected: Option<usize>,
    /// What the list says when it has no rows (type to search, searching, nothing found, an error).
    status: String,
    /// The number of the latest search: an older answer is dropped.
    asked: u64,
    images: HashMap<String, PathBuf>,
    /// How many times the pictures were asked for since the last search.
    tries: u32,
    buttons_view: Entity<FinderButtons>,
    _subscriptions: Vec<Subscription>,
}

impl Finder {
    pub fn new(model: Value, route: String, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let placeholder = model["placeholder"].as_str().unwrap_or_default().to_owned();
        let typed = model["typed"].as_str().unwrap_or_default().to_owned();
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(placeholder)
                .default_value(typed)
        });
        let subscriptions = vec![cx.subscribe_in(
            &input,
            window,
            |finder, _, event: &InputEvent, window, cx| match event {
                InputEvent::Change => finder.typed(cx),
                InputEvent::PressEnter { .. } => finder.confirm(window, cx),
                _ => {}
            },
        )];
        let status = model["labels"]["empty"].as_str().unwrap_or_default().to_owned();
        let buttons_view = {
            let weak = cx.entity().downgrade();
            cx.new(|_| FinderButtons(weak))
        };
        let mut finder = Self {
            model,
            route,
            input,
            rows: Vec::new(),
            selected: None,
            status,
            asked: 0,
            images: HashMap::new(),
            tries: 0,
            buttons_view,
            _subscriptions: subscriptions,
        };
        // A channel already set is searched at once, so its row is there to see.
        finder.typed(cx);
        finder
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        self.input.update(cx, |input, cx| input.focus(window, cx));
    }

    fn label(&self, key: &str) -> String {
        self.model["labels"][key].as_str().unwrap_or_default().to_owned()
    }

    fn platform(&self) -> String {
        self.model["platform"].as_str().unwrap_or_default().to_owned()
    }

    /// The box changed: search what it holds once typing rests.
    fn typed(&mut self, cx: &mut Context<Self>) {
        self.asked += 1;
        let number = self.asked;
        let query = self.input.read(cx).value().trim().to_owned();
        if self.model["searchable"] != true {
            return;
        }
        if query.is_empty() {
            self.rows.clear();
            self.selected = None;
            self.status = self.label("empty");
            cx.notify();
            return;
        }
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(REST).await;
            let _ = this.update(cx, |finder, cx| {
                if finder.asked != number {
                    return;
                }
                finder.status = finder.label("searching");
                send(
                    json!({"request": "StreamerzSearchChannels", "platform": finder.platform(), "query": query,
                    "id": format!("{}:find:{number}", finder.route)}),
                );
                cx.notify();
            });
        })
        .detach();
    }

    /// The channels a search found (`number` is the search's; an older one's answer is dropped).
    pub fn found(&mut self, number: u64, value: Value, cx: &mut Context<Self>) {
        if number != self.asked {
            return;
        }
        if let Some(err) = crate::error::StreamerzError::from_response(&value) {
            self.rows.clear();
            self.selected = None;
            let text = err.summary_line();
            let mut chars = text.chars();
            self.status = chars
                .next()
                .map(|first| first.to_uppercase().chain(chars).collect())
                .unwrap_or_default();
        } else {
            self.rows = value["channels"].as_array().cloned().unwrap_or_default();
            self.selected = (!self.rows.is_empty()).then_some(0);
            self.status = if self.rows.is_empty() {
                self.label("none")
            } else {
                String::new()
            };
            self.tries = 0;
            self.ask_images();
        }
        cx.notify();
    }

    /// Pictures that are on disk now (the others come with the next answer).
    pub fn images(&mut self, value: &Value, cx: &mut Context<Self>) {
        for (url, file) in value["files"].as_object().into_iter().flatten() {
            if let Some(file) = file.as_str() {
                self.images.insert(url.clone(), PathBuf::from(file));
            }
        }
        // The core is still downloading the rest: ask again in a moment (a few times at most).
        if !self.missing().is_empty() && self.tries < 8 {
            self.tries += 1;
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(600))
                    .await;
                let _ = this.update(cx, |finder, _| finder.ask_images());
            })
            .detach();
        }
        cx.notify();
    }

    /// The rows' pictures not on disk yet.
    fn missing(&self) -> Vec<String> {
        self.rows
            .iter()
            .filter_map(|row| row["picture"].as_str())
            .filter(|url| !url.is_empty() && !self.images.contains_key(*url))
            .map(str::to_owned)
            .collect()
    }

    fn ask_images(&self) {
        let urls = self.missing();
        if !urls.is_empty() {
            send(json!({"request": "StreamerzChat", "op": "images", "urls": urls,
                "id": format!("{}:find-images", self.route)}));
        }
    }

    /// OK: the channel picked, else what was typed (a link or an @name); nothing to read, nothing done.
    fn confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let picked = self
            .selected
            .and_then(|at| self.rows.get(at))
            .and_then(|row| row["handle"].as_str())
            .map(str::to_owned)
            .or_else(|| Some(self.input.read(cx).value().trim().to_owned()).filter(|typed| !typed.is_empty()));
        let Some(picked) = picked else { return };
        send(json!({"request": "StreamerzCoreAction", "domain": "accounts",
            "action": format!("{}.set_channel", self.platform()), "itemId": picked, "id": format!("{}:act", self.route)}));
        window.close_dialog(cx);
    }

    fn own(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        send(json!({"request": "StreamerzCoreAction", "domain": "accounts",
            "action": format!("{}.own_channel", self.platform()), "id": format!("{}:act", self.route)}));
        window.close_dialog(cx);
    }

    /// The finder's buttons, for its dialog's footer: "Use my own channel" apart, OK and Cancel at the right.
    fn buttons(&self, cx: &mut Context<Self>) -> Div {
        let own = Button::new("find-own")
            .h(px(CONTROL_HEIGHT))
            .icon(Icon::empty().path("icons/user.svg"))
            .label(self.label("own"))
            .on_click(cx.listener(|finder, _, window, cx| finder.own(window, cx)))
            .into_any_element();
        let typed = !self.input.read(cx).value().trim().is_empty();
        let right = div()
            .flex()
            .gap(px(SPACE_SM))
            .child(
                Button::new("find-ok")
                    .h(px(CONTROL_HEIGHT))
                    .primary()
                    .label(self.label("ok"))
                    .disabled(self.selected.is_none() && !typed)
                    .on_click(cx.listener(|finder, _, window, cx| finder.confirm(window, cx))),
            )
            .child(
                Button::new("find-cancel")
                    .h(px(CONTROL_HEIGHT))
                    .label(self.label("cancel"))
                    .on_click(|_, window, cx| crate::controls::dialog::dismiss(window, cx)),
            );
        super::dialog::footer(Some(own), right, cx)
    }

    pub fn buttons_view(&self) -> Entity<FinderButtons> {
        self.buttons_view.clone()
    }
}

impl Render for Finder {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let muted = theme.muted_foreground;
        let header = div()
            .flex()
            .items_center()
            .gap(px(SPACE_SM))
            .px(px(SPACE_MD))
            .text_size(px(FONT_CAPTION))
            .line_height(px(LINE_CAPTION))
            .font_weight(FontWeight::BOLD)
            .text_color(muted)
            .child(div().w(px(24.)).flex_shrink_0())
            .child(div().flex_1().min_w_0().child(self.label("channel")))
            .child(div().w(px(120.)).flex_shrink_0().child(self.label("followers")))
            .child(div().w(px(56.)).flex_shrink_0());
        let mut list = div()
            .id("find-rows")
            .h(px(super::dialog::fit_height(280., 140., 300., window)))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .rounded(theme.radius)
            .border_1()
            .border_color(theme.border)
            .bg(theme.table);
        if self.rows.is_empty() {
            list = list.child(
                div()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .px(px(SPACE_LG))
                    .text_color(muted)
                    // A long message (an error) wraps inside the list, centred.
                    .child(div().w_full().text_center().child(self.status.clone())),
            );
        }
        for (at, row) in self.rows.iter().enumerate() {
            let selected = self.selected == Some(at);
            let picture = row["picture"].as_str().and_then(|url| self.images.get(url)).cloned();
            list = list.child(
                div()
                    .id(("find-row", at))
                    .flex()
                    .items_center()
                    .gap(px(SPACE_SM))
                    .h(px(CONTROL_HEIGHT))
                    .px(px(SPACE_MD))
                    .flex_shrink_0()
                    .cursor_pointer()
                    .when(selected, |line| line.bg(theme.muted))
                    .hover(|line| line.bg(theme.muted.opacity(0.6)))
                    .child(match picture {
                        Some(file) => img(file)
                            .size(px(24.))
                            .rounded_full()
                            .flex_shrink_0()
                            .object_fit(ObjectFit::Cover)
                            .into_any_element(),
                        None => Icon::empty()
                            .path("icons/circle-user-round.svg")
                            .size(px(24.))
                            .text_color(muted)
                            .into_any_element(),
                    })
                    .child({
                        let name = row["name"].as_str().unwrap_or_default().to_owned();
                        let name_tip = name.clone();
                        div()
                            .id(SharedString::from(format!("find-name-{at}")))
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
                    .child({
                        let detail = row["detail"].as_str().unwrap_or_default().to_owned();
                        let detail_tip = detail.clone();
                        div()
                            .id(SharedString::from(format!("find-detail-{at}")))
                            .w(px(120.))
                            .flex_shrink_0()
                            .truncate()
                            .text_color(muted)
                            .child(detail)
                            .when(!detail_tip.is_empty(), |el| {
                                el.tooltip(move |window, cx| {
                                    gpui_kit::component::tooltip::Tooltip::new(detail_tip.clone()).build(window, cx)
                                })
                            })
                    })
                    .child(div().w(px(56.)).flex_shrink_0().when(row["live"] == true, |live| {
                        live.flex()
                            .items_center()
                            .gap(px(SPACE_XS))
                            .text_color(theme.danger)
                            .child(div().size(px(7.)).rounded_full().bg(theme.danger))
                            .child(self.label("live"))
                    }))
                    // A click picks the row; a double click reads it.
                    .on_click(cx.listener(move |finder, event: &ClickEvent, window, cx| {
                        finder.selected = Some(at);
                        if event.click_count() > 1 {
                            finder.confirm(window, cx);
                        }
                        cx.notify();
                    })),
            );
        }
        let note = self.model["note"]
            .as_str()
            .filter(|note| !note.is_empty())
            .map(str::to_owned);
        div()
            .id("channel-finder")
            .flex()
            .flex_col()
            .gap(px(SPACE_SM))
            .child(Input::new(&self.input).small().min_h(px(CONTROL_HEIGHT)))
            .when(self.model["searchable"] == true, |column| {
                column.child(header).child(list)
            })
            .when_some(note, |column, note| {
                column.child(
                    div()
                        .text_size(px(FONT_CAPTION))
                        .line_height(px(LINE_CAPTION))
                        .text_color(muted)
                        .child(note),
                )
            })
    }
}

/// The finder's buttons, drawn in its dialog's footer.
pub struct FinderButtons(WeakEntity<Finder>);
impl Render for FinderButtons {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        match self.0.upgrade() {
            Some(finder) => finder.update(cx, |finder, cx| finder.buttons(cx)).into_any_element(),
            None => div().into_any_element(),
        }
    }
}

#[cfg(all(test, feature = "test-support"))]
mod tests {
    use super::Finder;
    use gpui_kit::test::{TestEventExt, TestWindowExt};
    use gpui_kit::{
        div, px, size, AppContext, Bounds, Entity, IntoElement, ParentElement, Render, Styled, TestAppContext, Window,
        WindowBounds, WindowOptions,
    };
    use serde_json::json;

    struct FinderHost(Entity<Finder>);
    impl Render for FinderHost {
        fn render(&mut self, _: &mut Window, _: &mut gpui_kit::Context<Self>) -> impl IntoElement {
            div().size_full().p_4().child(self.0.clone())
        }
    }

    #[gpui_kit::test]
    fn channel_finder_drops_stale_results_and_clears_on_empty_query(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::theme::apply(true, cx);
        });
        let model = json!({
            "platform": "twitch",
            "searchable": true,
            "placeholder": "Search channels",
            "typed": "",
            "labels": {
                "empty": "Type to search.",
                "searching": "Searching...",
                "none": "Nothing found.",
                "channel": "Channel",
                "followers": "Followers",
                "live": "LIVE",
                "own": "Use my own channel",
                "ok": "OK",
                "cancel": "Cancel"
            }
        });
        let slot: std::rc::Rc<std::cell::RefCell<Option<Entity<Finder>>>> =
            std::rc::Rc::new(std::cell::RefCell::new(None));
        let s = slot.clone();
        let window = cx.update(|cx| {
            let bounds = Bounds {
                origin: gpui_kit::Point::default(),
                size: size(px(640.), px(520.)),
            };
            let (window, _) = gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                cx,
                move |window, cx| {
                    let finder = cx.new(|cx| Finder::new(model, "accounts".into(), window, cx));
                    *s.borrow_mut() = Some(finder.clone());
                    cx.new(|_| FinderHost(finder))
                },
            )
            .expect("open test window");
            window.downcast::<gpui_kit::base::Root>().expect("root window")
        });
        let finder = slot.borrow().clone().expect("finder created");

        cx.update_window(window.into(), |_, window, cx| {
            window.render_frame(cx);
            finder.update(cx, |finder, cx| finder.focus(window, cx));
            window.input("streamer", cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(window.into(), |_, window, cx| {
            let asked = finder.read(cx).asked;
            // Stale response with older search number is ignored.
            finder.update(cx, |finder, cx| {
                finder.found(
                    asked.saturating_sub(1),
                    json!({"channels": [{"handle": "old", "name": "Old"}]}),
                    cx,
                );
            });
            assert!(finder.read(cx).rows.is_empty());

            // Matching response populates rows and selects the first result.
            finder.update(cx, |finder, cx| {
                finder.found(
                    asked,
                    json!({"channels": [{"handle": "streamerz", "name": "Streamerz", "detail": "100"}]}),
                    cx,
                );
            });
            assert_eq!(finder.read(cx).rows.len(), 1);
            assert_eq!(finder.read(cx).selected, Some(0));

            // Clearing the query clears rows and restores the empty prompt.
            window.key_down("ctrl-a", false, cx);
            window.key_up("ctrl-a", cx);
            window.key_down("backspace", false, cx);
            window.key_up("backspace", cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(window.into(), |_, _, cx| {
            assert!(finder.read(cx).rows.is_empty());
            assert_eq!(finder.read(cx).selected, None);
            assert_eq!(finder.read(cx).status, "Type to search.");
        })
        .unwrap();
    }
}
