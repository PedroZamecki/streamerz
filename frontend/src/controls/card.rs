//! A person's card in a dialog: who they are, their time here, their groups and what they said.
//! The core supplies the whole model (`user_card`); the groups' switches are core actions.
use crate::send;
use gpui_kit::component::{
    button::{Button, ButtonVariants},
    ActiveTheme, Sizable, WindowExt,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;

pub struct Card {
    model: Value,
    route: String,
    /// What the platform says about the person (picture, banner, followers, what we may do to them):
    /// it arrives after the card opened and survives the card being drawn again.
    lookup: Value,
    images: HashMap<String, PathBuf>,
    asking: bool,
}
impl Card {
    pub fn new(model: Value, route: String) -> Self {
        let card = Self {
            model,
            route,
            lookup: Value::Null,
            images: HashMap::new(),
            asking: false,
        };
        if let Some((platform, user)) = card.model["id"].as_str().and_then(|id| id.split_once('|')) {
            send(
                json!({"request": "StreamerzChat", "op": "card", "platform": platform, "userId": user,
                "lookup": true, "id": format!("{}:card-lookup", card.route)}),
            );
        }
        card
    }
    pub fn update_model(&mut self, model: Value, cx: &mut Context<Self>) {
        self.model = model;
        cx.notify();
    }
    /// The platform's answer.
    pub fn lookup(&mut self, value: Value, cx: &mut Context<Self>) {
        self.lookup = value["card"].clone();
        self.ask_images();
        cx.notify();
    }
    fn ask_images(&mut self) {
        let urls: Vec<&str> = ["picture", "banner"]
            .into_iter()
            .filter_map(|key| self.lookup[key].as_str())
            .filter(|url| !url.is_empty() && !self.images.contains_key(*url))
            .collect();
        if self.asking || urls.is_empty() {
            return;
        }
        self.asking = true;
        send(
            json!({"request": "StreamerzChat", "op": "images", "urls": urls, "id": format!("{}:card-images", self.route)}),
        );
    }
    pub fn images(&mut self, value: &Value, cx: &mut Context<Self>) {
        self.asking = false;
        for (url, file) in value["files"].as_object().into_iter().flatten() {
            if let Some(file) = file.as_str() {
                self.images.insert(url.clone(), PathBuf::from(file));
            }
        }
        cx.notify();
    }
}
impl Render for Card {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let card = &self.model;
        let id = card["id"].as_str().unwrap_or_default().to_owned();
        let title = card["title"].as_str().unwrap_or_default().to_owned();
        let tint = card["color"]
            .as_str()
            .and_then(|hex| u32::from_str_radix(hex.trim_start_matches('#'), 16).ok())
            .filter(|_| {
                card["color"]
                    .as_str()
                    .is_some_and(|hex| hex.trim_start_matches('#').len() == 6)
            })
            .map(|value| rgb(value).into());
        let tint: Hsla = tint.unwrap_or(cx.theme().muted_foreground);
        let label = |name: &str| card["labels"][name].as_str().unwrap_or_default().to_owned();
        let mut body = div().id("user-card").flex().flex_col().gap_3().w_full();
        if let Some(file) = self.lookup["banner"].as_str().and_then(|url| self.images.get(url)) {
            body = body.child(
                img(file.clone())
                    .id("card-banner")
                    .w_full()
                    .h(px(96.))
                    .rounded(cx.theme().radius)
                    .object_fit(ObjectFit::Cover),
            );
        }
        let mut body = body.child(
            div()
                .flex()
                .items_center()
                .gap_3()
                .child(
                    match self.lookup["picture"].as_str().and_then(|url| self.images.get(url)) {
                        Some(file) => img(file.clone())
                            .id("card-picture")
                            .size(px(44.))
                            .rounded_full()
                            .flex_shrink_0()
                            .object_fit(ObjectFit::Cover)
                            .into_any_element(),
                        None => div()
                            .size(px(44.))
                            .flex_shrink_0()
                            .rounded_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .bg(tint.opacity(0.25))
                            .text_color(tint)
                            .text_lg()
                            .font_weight(FontWeight::BOLD)
                            .child(
                                title
                                    .chars()
                                    .next()
                                    .map(|c| c.to_uppercase().to_string())
                                    .unwrap_or_default(),
                            )
                            .into_any_element(),
                    },
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .min_w_0()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    img(format!("brands/{}.svg", card["platform"].as_str().unwrap_or_default()))
                                        .id("card-platform")
                                        .size_4()
                                        .flex_shrink_0(),
                                )
                                .child(
                                    div()
                                        .font_weight(FontWeight::BOLD)
                                        .child(card["platformLabel"].as_str().unwrap_or_default().to_owned()),
                                ),
                        )
                        .child(
                            div()
                                .text_color(cx.theme().muted_foreground)
                                .truncate()
                                .child(card["role"].as_str().unwrap_or_default().to_owned()),
                        ),
                ),
        );
        for fact in card["facts"].as_array().into_iter().flatten() {
            body = body.child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        div()
                            .w(px(150.))
                            .flex_shrink_0()
                            .text_color(cx.theme().muted_foreground)
                            .child(fact["label"].as_str().unwrap_or_default().to_owned()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(fact["value"].as_str().unwrap_or_default().to_owned()),
                    ),
            );
        }
        let said = self.lookup["facts"].as_array().cloned().unwrap_or_default();
        let platform_facts: Vec<&Value> = said
            .iter()
            .filter(|fact| fact["label"] != self.lookup["sessionLabel"])
            .collect();
        if !platform_facts.is_empty() {
            body = body.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(self.lookup["says"].as_str().unwrap_or_default().to_owned()),
            );
        }
        for fact in platform_facts {
            body = body.child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        div()
                            .w(px(150.))
                            .flex_shrink_0()
                            .text_color(cx.theme().muted_foreground)
                            .child(fact["label"].as_str().unwrap_or_default().to_owned()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(fact["value"].as_str().unwrap_or_default().to_owned()),
                    ),
            );
        }
        // The platform's answer is on its way.
        if self.lookup.is_null() && card["asks"] == true {
            body = body.child(
                div()
                    .flex()
                    .gap_2()
                    .text_color(cx.theme().muted_foreground)
                    .child(div().w(px(150.)).flex_shrink_0().child(label("says")))
                    .child(label("looking")),
            );
        }
        let groups = card["groups"].as_array().cloned().unwrap_or_default();
        body = body.child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(div().font_weight(FontWeight::BOLD).child(label("groups")))
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .when(groups.is_empty(), |row| {
                            row.child(div().text_color(cx.theme().muted_foreground).child(label("noGroups")))
                        })
                        .children(groups.iter().enumerate().map(|(index, group)| {
                            let (route, item) = (
                                self.route.clone(),
                                format!("{id}|{}", group["id"].as_str().unwrap_or_default()),
                            );
                            Button::new(("card-group", index))
                                .small()
                                .label(group["name"].as_str().unwrap_or_default().to_owned())
                                .when(group["member"] == true, |button| {
                                    button
                                        .primary()
                                        .icon(gpui_kit::component::Icon::empty().path("icons/check.svg"))
                                })
                                .tooltip(label("groupsTip"))
                                .on_click(move |_, _, _| {
                                    send(json!({"request": "StreamerzCoreAction", "domain": "community",
                                        "action": "card_group", "itemId": item, "id": format!("{route}:card-act")}))
                                })
                        })),
                ),
        );
        let messages = card["messages"].as_array().cloned().unwrap_or_default();
        body = body.child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(div().font_weight(FontWeight::BOLD).child(label("messages")))
                .child(
                    div()
                        .id("card-messages")
                        .max_h(px(200.))
                        .overflow_y_scroll()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .when(messages.is_empty(), |list| {
                            list.child(div().text_color(cx.theme().muted_foreground).child(label("noMessages")))
                        })
                        .children(messages.iter().map(|message| {
                            div()
                                .flex()
                                .gap_2()
                                .child(
                                    div()
                                        .w(px(64.))
                                        .flex_shrink_0()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(message["time"].as_str().unwrap_or_default().to_owned()),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .child(message["text"].as_str().unwrap_or_default().to_owned()),
                                )
                        })),
                ),
        );
        let route = self.route.clone();
        let who = id
            .split_once('|')
            .map(|(platform, user)| (platform.to_owned(), user.to_owned()));
        let mut moderation = div().flex().flex_wrap().gap_2();
        for act in self.lookup["acts"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            let (icon, label, secs) = match act {
                "timeout" => (
                    "clock",
                    self.lookup["labels"]["timeout"].as_str().unwrap_or_default().to_owned(),
                    600,
                ),
                "ban" => (
                    "ban",
                    self.lookup["labels"]["ban"].as_str().unwrap_or_default().to_owned(),
                    0,
                ),
                _ => continue,
            };
            let (act, who) = (act.to_owned(), who.clone());
            moderation = moderation.child(
                Button::new(SharedString::from(format!("card-{act}")))
                    .small()
                    .icon(gpui_kit::component::Icon::empty().path(format!("icons/{icon}.svg")))
                    .label(label)
                    .on_click(move |_, window, cx| {
                        if let Some((platform, user)) = &who {
                            send(
                                json!({"request": "StreamerzChat", "op": "act", "platform": platform, "userId": user,
                                "act": act, "secs": secs, "id": "chat:act"}),
                            );
                        }
                        window.close_dialog(cx);
                    }),
            );
        }
        body.child(moderation).child(
            div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("card-profile")
                        .small()
                        .icon(gpui_kit::component::Icon::empty().path("icons/external-link.svg"))
                        .label(label("profile"))
                        .on_click(move |_, _, _| {
                            send(json!({"request": "StreamerzCoreAction", "domain": "community",
                                "action": "open_profile", "itemId": id, "id": format!("{route}:card-open")}))
                        }),
                )
                .child(
                    Button::new("card-close")
                        .small()
                        .label(label("close"))
                        .on_click(|_, window, cx| window.close_dialog(cx)),
                ),
        )
    }
}
