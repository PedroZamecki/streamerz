//! A person's card in a dialog: who they are, their time here, their groups and what they said.
//! The core supplies the whole model (`user_card`); the groups' switches are core actions.
use super::images::{Pictures, MAX_PENDING};
use crate::send;
use gpui_kit::*;
use serde_json::{json, Value};
use std::collections::HashSet;

pub struct Card {
    model: Value,
    route: String,
    /// What the platform says about the person (picture, banner, followers, what we may do to them):
    /// it arrives after the card opened and survives the card being drawn again.
    lookup: Value,
    images: Pictures,
    /// The box shows what actions get for them, not what they said.
    details: bool,
    /// How many times the platform was asked again while it had not finished.
    retries: u32,
}

/// The platform is asked again this often while it has not finished, and this many times at most
/// (about seven seconds): a lookup that never ends must not keep the window busy.
const RETRY_MS: u64 = 350;
const RETRIES: u32 = 20;
const BATCH: usize = 64;
const REMEMBERED: usize = 256;
impl Card {
    pub fn new(model: Value, route: String) -> Self {
        let images = Pictures::new(format!("{route}:card-images"), BATCH, REMEMBERED, MAX_PENDING);
        let card = Self {
            model,
            route,
            lookup: Value::Null,
            images,
            details: false,
            retries: 0,
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
        if self.retry() {
            let (route, id) = (
                self.route.clone(),
                self.model["id"].as_str().unwrap_or_default().to_owned(),
            );
            cx.spawn(async move |card, cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(RETRY_MS))
                    .await;
                // A card closed since asks nothing more.
                if card.upgrade().is_none() {
                    return;
                }
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
    /// Whether to ask the platform again: it has not finished, and the tries are not used up.
    fn retry(&mut self) -> bool {
        if self.lookup["pending"] != true || self.retries >= RETRIES {
            return false;
        }
        self.retries += 1;
        true
    }
    fn ask_images(&mut self) {
        let wanted = super::chat::ChatView::card_urls(&self.lookup);
        self.images.ask_for(wanted.iter().map(String::as_str));
    }
    pub fn images(&mut self, value: &Value, cx: &mut Context<Self>) {
        self.images.apply(value, HashSet::new);
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
        super::chat::card_body(&self.route, &card, self.images.files(), self.details, toggle, None, cx)
    }
}

#[cfg(test)]
mod tests {
    use super::{Card, Pictures, BATCH, MAX_PENDING, REMEMBERED, RETRIES};
    use serde_json::{json, Value};

    #[test]
    fn a_lookup_that_never_finishes_is_asked_again_a_few_times_only() {
        let mut card = Card {
            model: json!({"id": "twitch|1"}),
            route: "chat:1".into(),
            lookup: Value::Null,
            images: Pictures::new("chat:1:card-images", BATCH, REMEMBERED, MAX_PENDING),
            details: false,
            retries: 0,
        };
        assert!(!card.retry(), "no answer yet: nothing to ask again");
        card.lookup = json!({"pending": true});
        let asked = (0..RETRIES * 2).filter(|_| card.retry()).count();
        assert_eq!(asked as u32, RETRIES);
        card.lookup = json!({"user": "someone"});
        assert!(!card.retry());
    }
}
