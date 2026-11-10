//! A person's card in a dialog: who they are, their time here, their groups and what they said.
//! The core supplies the whole model (`user_card`); the groups' switches are core actions.
use crate::send;
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
    /// The box shows what actions get for them, not what they said.
    details: bool,
}
impl Card {
    pub fn new(model: Value, route: String) -> Self {
        let card = Self {
            model,
            route,
            lookup: Value::Null,
            images: HashMap::new(),
            asking: false,
            details: false,
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
        // The platform has not finished: ask what it has said since, in a moment.
        if self.lookup["pending"] == true {
            let (route, id) = (
                self.route.clone(),
                self.model["id"].as_str().unwrap_or_default().to_owned(),
            );
            cx.spawn(async move |_, cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(350))
                    .await;
                if let Some((platform, user)) = id.split_once('|') {
                    send(
                        json!({"request": "StreamerzChat", "op": "card", "platform": platform, "userId": user,
                        "lookup": true, "id": format!("{route}:card-lookup")}),
                    );
                }
            })
            .detach();
        }
        cx.notify();
    }
    fn ask_images(&mut self) {
        let wanted = super::chat::ChatView::card_urls(&self.lookup);
        let urls: Vec<&str> = wanted
            .iter()
            .map(String::as_str)
            .filter(|url| !self.images.contains_key(*url))
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
        // What the platform said is the whole card (it has the person's groups and lines too); until it
        // comes, the core's own model stands in.
        let card = if self.lookup.is_object() {
            let mut card = self.lookup.clone();
            card["profile"] = self.model["profile"].clone();
            card
        } else {
            let model = &self.model;
            json!({"id": model["id"], "user": model["title"], "platform": model["platform"], "color": model["color"],
                "role": model["role"], "facts": model["facts"], "groups": model["groups"],
                "labels": model["labels"], "groupsLabel": model["labels"]["groups"], "groupsTip": model["labels"]["groupsTip"],
                "noGroups": model["labels"]["noGroups"], "recent": model["labels"]["messages"],
                "noMessages": model["labels"]["noMessages"], "detailsLabel": "", "recentShort": "",
                "messages": model["messages"].as_array().into_iter().flatten()
                    .map(|line| json!({"parts": [{"t": line["text"]}], "time": line["time"]})).collect::<Vec<_>>()})
        };
        let this = cx.entity();
        let toggle = move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
            this.update(cx, |card, cx| {
                card.details = !card.details;
                cx.notify();
            });
        };
        super::chat::card_body(&self.route, &card, &self.images, self.details, toggle, cx)
    }
}
