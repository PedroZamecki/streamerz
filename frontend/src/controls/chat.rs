//! The Chat page: every platform's chat as one virtualised list, a send box and the user card.
//!
//! The core decides what each line is made of; this only draws it. The list holds at most `MOST`
//! lines (the core keeps no more), follows the newest one until the reader scrolls up, and asks the
//! core for just the lines newer than the last one it has. Pictures are files the core downloaded:
//! the page asks for the ones it shows and draws them when they land.
use super::emotes::{EmotePicker, Picked};
use super::sections::Sections;
use crate::send;
use gpui_kit::component::{
    button::{Button, ButtonVariants},
    checkbox::Checkbox,
    input::{InlineToken, InlineTokenContext, Input, InputContent, InputEvent, InputState},
    menu::{ContextMenuExt, DropdownMenu, PopupMenuItem},
    message_scroller::{MessageScroller, MessageScrollerState},
    popover::Popover,
    select::{Select, SelectEvent, SelectState},
    tooltip::Tooltip,
    ActiveTheme, Disableable, Icon, IndexPath, Sizable, WindowExt,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

/// The names of our accounts, from the page's model.
fn value_me(model: &Value) -> Vec<String> {
    model["chat"]["me"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|name| name.as_str().map(str::to_owned))
        .collect()
}

/// What is listed above a send box while a command or a name is being typed.
struct Suggest {
    pane: usize,
    /// Typing a name (an `@`), not a command.
    mention: bool,
    header: String,
    /// Where the word being typed starts, in characters.
    start: usize,
    rows: Vec<Value>,
    at: usize,
}

/// The line whose name the pointer rests on (0: none).
static HOVERED: AtomicU64 = AtomicU64::new(0);

/// The most lines the page holds.
const MOST: usize = 1000;
/// Pictures asked for in one request.
const BATCH: usize = 64;
/// Pictures remembered before the map starts over.
const REMEMBERED: usize = 3000;

/// The page asks to go to another one (Accounts, when no chat is connected).
pub struct Go(pub String);
impl EventEmitter<Go> for ChatView {}

/// The platforms a pane can show, in the order of the Show box.
const PLATFORMS: [&str; 4] = ["twitch", "youtube", "kick", "tiktok"];

/// One chat column: the lines of one platform (or all of them), its own scroll and its own send box.
struct Pane {
    /// The platform it shows; `None`: every chat together.
    platform: Option<&'static str>,
    scroller: Entity<MessageScrollerState>,
    /// The numbers of the lines it shows, and where they are in the page's list.
    seqs: Vec<u64>,
    at: Vec<usize>,
    input: Entity<InputState>,
    /// The text last asked about: an answer for other text is out of date.
    typed: String,
}

pub struct ChatView {
    lines: Vec<Value>,
    /// The "all together" column first, then one for each platform.
    panes: Vec<Pane>,
    /// The pane the emote picker or a reply is for.
    asking: Option<usize>,
    generation: u64,
    seq: u64,
    model: Value,
    /// The box of "Message all".
    all_input: Entity<InputState>,
    reply: Option<(u64, String, String)>,
    images: HashMap<String, PathBuf>,
    missing: HashSet<String>,
    asking_images: bool,
    reading: bool,
    read_again: bool,
    error: String,
    /// A header or account picture is on its way: the page is read again when pictures land.
    wants_pictures: bool,
    /// The names of our own accounts (lower case): a tag of one is a mention of us.
    me: Vec<String>,
    /// The list open above a send box: what the core suggested for what is typed.
    suggest: Option<Suggest>,
    /// When each line that came while the page was open appeared, for its fade in (lines are dropped
    /// from here once they are fully in).
    born: HashMap<u64, std::time::Instant>,
    fading: bool,
    /// The Show box's choices (ids) and the box.
    view_ids: Vec<String>,
    view_select: Option<Entity<SelectState<Vec<String>>>>,
    options: Option<Entity<Sections>>,
    card: Option<Entity<CardView>>,
    picker: Option<Entity<EmotePicker>>,
    all_targets: HashSet<String>,
    all_bot: bool,
    all_fallback: bool,
    _subscriptions: Vec<Subscription>,
}

impl ChatView {
    pub fn new(model: Value, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let hint = model["chat"]["texts"]["sendPlaceholder"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        let mut subscriptions = Vec::new();
        let mut panes = Vec::new();
        for platform in std::iter::once(None).chain(PLATFORMS.iter().copied().map(Some)) {
            let input = cx.new(|cx| InputState::new(window, cx).placeholder(hint.clone()));
            let at = panes.len();
            subscriptions.push(
                cx.subscribe_in(
                    &input,
                    window,
                    move |view, _, event: &InputEvent, window, cx| match event {
                        InputEvent::PressEnter { .. } => view.submit(at, window, cx),
                        InputEvent::Change => view.ask_suggest(at, cx),
                        _ => {}
                    },
                ),
            );
            panes.push(Pane {
                platform,
                scroller: cx.new(|cx| MessageScrollerState::new(0, cx)),
                seqs: Vec::new(),
                at: Vec::new(),
                input,
                typed: String::new(),
            });
        }
        let all_input = cx.new(|cx| InputState::new(window, cx).placeholder(hint));
        subscriptions.push(
            cx.subscribe_in(&all_input, window, |view, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    view.submit_all(window, cx);
                }
            }),
        );
        let mut view = Self {
            lines: Vec::new(),
            panes,
            asking: None,
            generation: 0,
            seq: 0,
            model: Value::Null,
            all_input,
            reply: None,
            images: HashMap::new(),
            missing: HashSet::new(),
            asking_images: false,
            reading: false,
            read_again: false,
            error: String::new(),
            wants_pictures: false,
            me: Vec::new(),
            suggest: None,
            born: HashMap::new(),
            fading: false,
            view_ids: Vec::new(),
            view_select: None,
            options: None,
            card: None,
            picker: None,
            all_targets: PLATFORMS.iter().map(|p| (*p).to_string()).collect(),
            all_bot: false,
            all_fallback: true,
            _subscriptions: subscriptions,
        };
        view.update_model(model, window, cx);
        view
    }

    /// Ask the core for what is new. One request at a time; a change meanwhile asks again after.
    pub fn refresh(&mut self) {
        if self.reading {
            self.read_again = true;
            return;
        }
        self.reading = true;
        send(
            json!({"request": "StreamerzCoreView", "domain": "chat", "gen": self.generation, "seq": self.seq,
            "have": self.lines.len(), "look": self.model["chat"]["look"], "id": "chat:read"}),
        );
    }

    /// Pictures arrived: the ones the page waits for are asked for again.
    pub fn images_arrived(&mut self, cx: &mut Context<Self>) {
        self.ask_images();
        if self.wants_pictures {
            self.refresh();
        }
        if let Some(picker) = self.picker.clone() {
            picker.update(cx, |picker, cx| picker.images_arrived(cx));
        }
    }

    /// Open the emote picker for the chosen platform; a pick goes into the send box.
    fn open_picker(&mut self, pane: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.picker.is_some() {
            return;
        }
        self.asking = Some(pane);
        let hint = self.texts()["emoteSearch"].as_str().unwrap_or_default().to_owned();
        let platform = self.panes[pane].platform.unwrap_or("all").to_owned();
        let picker = cx.new(|cx| EmotePicker::new(&platform, hint, window, cx));
        self._subscriptions
            .push(cx.subscribe_in(&picker, window, |view, _, event: &Picked, window, cx| {
                let Picked(name, url, keep) = event;
                let pane = view.asking.unwrap_or(0);
                let input = view.panes[pane].input.clone();
                input.update(cx, |input, cx| put_emote(input, name, url, window, cx));
                if let Some(picker_ref) = view.picker.as_ref() {
                    if let Some(file) = picker_ref.read(cx).image_file(url) {
                        view.images.insert(url.clone(), file.clone());
                    }
                }
                if !url.is_empty() && !view.images.contains_key(url) {
                    view.missing.insert(url.clone());
                    view.ask_images();
                }
                view.ask_suggest(pane, cx);
                if !keep {
                    view.close_picker(window, cx);
                    input.update(cx, |input, cx| input.focus(window, cx));
                }
            }));
        self.picker = Some(picker.clone());
        picker.update(cx, |picker, cx| picker.focus(window, cx));
        cx.notify();
    }

    fn close_picker(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.picker = None;
        self.asking = None;
        cx.notify();
    }

    fn ask_images(&mut self) {
        if self.asking_images || self.missing.is_empty() {
            return;
        }
        let urls: Vec<&String> = self.missing.iter().take(BATCH).collect();
        self.asking_images = true;
        send(json!({"request": "StreamerzChat", "op": "images", "urls": urls, "id": "chat:images"}));
    }

    /// Note the pictures of a line the page has not got yet.
    fn want(&mut self, line: &Value) {
        let mut urls: Vec<&str> = Vec::new();
        for part in line["parts"].as_array().into_iter().flatten() {
            if let Some(url) = part["u"].as_str() {
                urls.push(url);
            }
        }
        for badge in line["badges"].as_array().into_iter().flatten() {
            if let Some(url) = badge["url"].as_str().filter(|url| !url.is_empty()) {
                urls.push(url);
            }
        }
        for key in ["picture", "gif", "costIcon"] {
            if let Some(url) = line[key].as_str().filter(|url| !url.is_empty()) {
                urls.push(url);
            }
        }
        for url in urls {
            if !self.images.contains_key(url) {
                self.missing.insert(url.to_owned());
            }
        }
    }

    pub fn update_model(&mut self, value: Value, window: &mut Window, cx: &mut Context<Self>) {
        let chat = &value["chat"];
        let fresh: Vec<Value> = chat["lines"].as_array().cloned().unwrap_or_default();
        let replaced = chat["reset"] == true;
        if replaced {
            self.lines.clear();
        }
        let fade = value["chat"]["fadeIn"].as_u64().unwrap_or(0);
        let now = std::time::Instant::now();
        for line in &fresh {
            self.want(line);
            // A line that comes while the page is open fades in; the ones of a fresh read are just there.
            if fade > 0 && !replaced && !cx.reduce_motion() {
                if let Some(seq) = line["seq"].as_u64() {
                    self.born.insert(seq, now);
                }
            }
        }
        self.lines.extend(fresh);
        if !self.born.is_empty() && !self.fading {
            self.fade_on(fade, cx);
        }
        let over = self.lines.len().saturating_sub(MOST);
        if over > 0 {
            self.lines.drain(..over);
        }
        self.sync_panes(replaced, cx);
        self.generation = chat["gen"].as_u64().unwrap_or(0);
        self.seq = chat["seq"].as_u64().unwrap_or(0);
        self.model = value;
        self.me = value_me(&self.model);
        self.wants_pictures = self.platforms_wait();
        self.sync_view(window, cx);
        if let Some(reply) = &self.reply {
            if !self.lines.iter().any(|line| line["seq"] == reply.0) {
                self.reply = None;
            }
        }
        self.ask_images();
        self.reading = false;
        if std::mem::take(&mut self.read_again) {
            self.refresh();
        }
        cx.notify();
    }

    /// Redraw about 30 times a second while lines are fading in, and not at all when none is.
    fn fade_on(&mut self, fade: u64, cx: &mut Context<Self>) {
        self.fading = true;
        cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(33))
                .await;
            let more = this
                .update(cx, |view, cx| {
                    let span = std::time::Duration::from_millis(fade);
                    view.born.retain(|_, born| born.elapsed() < span);
                    cx.notify();
                    let more = !view.born.is_empty();
                    view.fading = more;
                    more
                })
                .unwrap_or(false);
            if !more {
                break;
            }
        })
        .detach();
    }

    /// How much of a line shows, 0 to 1: coming in over `fade` ms from when it came.
    fn shown(&self, seq: u64) -> f32 {
        let fade = self.model["chat"]["fadeIn"].as_f64().unwrap_or(0.).max(1.);
        self.born
            .get(&seq)
            .map_or(1., |born| (born.elapsed().as_secs_f64() * 1000. / fade).min(1.) as f32)
    }

    /// Whether a platform's header or account picture is still on its way (the core has the name only).
    fn platforms_wait(&self) -> bool {
        self.model["chat"]["platforms"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|platform| platform["present"] == true)
            .any(|platform| {
                let empty = |value: &Value| value.as_str().is_some_and(str::is_empty);
                (platform["room"]["name"].as_str().is_some_and(|name| !name.is_empty())
                    && empty(&platform["room"]["picture"]))
                    || (platform["sender"]["name"].is_string() && empty(&platform["sender"]["picture"]))
            })
    }

    /// Fill each pane with the lines it shows, keeping its scroll: lines that came are appended, the
    /// ones that left the front are dropped, and anything else starts the pane over.
    fn sync_panes(&mut self, replaced: bool, cx: &mut Context<Self>) {
        let lines = &self.lines;
        for pane in &mut self.panes {
            let (at, seqs): (Vec<usize>, Vec<u64>) = lines
                .iter()
                .enumerate()
                .filter(|(_, line)| pane.platform.is_none_or(|platform| line["platform"] == platform))
                .map(|(at, line)| (at, line["seq"].as_u64().unwrap_or(0)))
                .unzip();
            let old = std::mem::replace(&mut pane.seqs, seqs.clone());
            pane.at = at;
            if replaced {
                pane.scroller.update(cx, |scroller, cx| scroller.reset(seqs.len(), cx));
                continue;
            }
            if old == seqs {
                continue;
            }
            let dropped = old
                .iter()
                .take_while(|seq| seqs.first().is_none_or(|first| *seq < first))
                .count();
            let rest = &old[dropped..];
            if seqs.len() >= rest.len() && seqs[..rest.len()] == *rest {
                if dropped > 0 {
                    pane.scroller
                        .update(cx, |scroller, cx| scroller.splice(0..dropped, 0, cx));
                }
                let added = seqs.len() - rest.len();
                if added > 0 {
                    pane.scroller.update(cx, |scroller, cx| scroller.append(added, cx));
                }
            } else {
                pane.scroller.update(cx, |scroller, cx| scroller.reset(seqs.len(), cx));
            }
        }
    }

    fn submit(&mut self, pane: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(shown) = self.panes.get(pane) else { return };
        let text = shown.input.read(cx).value().trim().to_owned();
        if text.is_empty() {
            return;
        }
        let platform = shown.platform.unwrap_or("all");
        let input = shown.input.clone();
        let mut request =
            json!({"request": "StreamerzChat", "op": "send", "platform": platform, "text": text, "id": "chat:send"});
        if let Some((seq, _, _)) = &self.reply {
            request["replySeq"] = (*seq).into();
        }
        send(request);
        input.update(cx, |input, cx| input.set_value("", window, cx));
        self.reply = None;
        self.suggest = None;
        cx.notify();
    }

    /// Ask the core what fits what is typed: Twitch's and Kick's commands after a `/`, names after an `@`.
    fn ask_suggest(&mut self, pane: usize, cx: &mut Context<Self>) {
        let platform = self.panes.get(pane).and_then(|p| p.platform).unwrap_or("all");
        let input = self.panes[pane].input.read(cx);
        let text = input.value().to_string();
        let cursor = input.cursor_position().character as usize;
        self.panes[pane].typed.clone_from(&text);
        if text.is_empty() {
            if self.suggest.take().is_some() {
                cx.notify();
            }
            return;
        }
        send(
            json!({"request": "StreamerzChat", "op": "suggest", "platform": platform, "text": text,
            "cursor": cursor, "id": format!("chat:suggest:{pane}")}),
        );
    }

    /// Draw the emotes the core found in what is typed as pictures inside the box: each one is an inline
    /// token over its word, so the text (and what is sent) stays the same.
    fn inline_emotes(&mut self, pane: usize, parts: &[Value], window: &mut Window, cx: &mut Context<Self>) {
        let input = self.panes[pane].input.clone();
        let state = input.read(cx);
        let text = state.value().to_string();
        if text != self.panes[pane].typed {
            return;
        }
        let selection = state.selected_range();
        let old: Vec<(Range<usize>, String)> = state
            .tokens()
            .iter()
            .map(|span| (span.range(), span.token().id().to_string()))
            .collect();
        let caret = selection.is_empty().then_some(selection.start);
        let Some(spans) = emote_spans(&text, parts, &old, caret) else {
            return;
        };
        if spans == old {
            return;
        }
        let mut content = InputContent::new(text.clone());
        for (range, id) in spans {
            let token = InlineToken::new(id, text[range.clone()].to_owned());
            match content.with_token(range, token) {
                Ok(next) => content = next,
                Err(_) => return,
            }
        }
        input.update(cx, |input, cx| {
            if input.marked_text_range(window, cx).is_some() {
                return;
            }
            input.set_value(content, window, cx);
            input.set_selected_range(selection, cx);
        });
    }

    /// Put the picked row in the box: a name replaces the word being typed, a command is the whole text.
    fn complete(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = self.suggest.take() else { return };
        let Some(row) = open.rows.get(open.at) else { return };
        let fill = row["fill"].as_str().unwrap_or_default().to_owned();
        let more = row["more"] == true;
        let input = self.panes[open.pane].input.clone();
        let text = input.read(cx).value().to_string();
        let cursor = input.read(cx).cursor_position().character as usize;
        let chars: Vec<char> = text.chars().collect();
        let cursor = cursor.min(chars.len());
        let (new, at) = if open.mention {
            let before: String = chars[..open.start.min(cursor)].iter().collect();
            let after: String = chars[cursor..].iter().collect();
            let inserted = before.chars().count() + fill.chars().count() + 1;
            (format!("{before}{fill} {after}"), inserted)
        } else {
            let done = if more { format!("{fill} ") } else { fill };
            let length = done.chars().count();
            (done, length)
        };
        input.update(cx, |input, cx| {
            input.set_value(new, window, cx);
            input.set_cursor_position(
                gpui_kit::component::input::Position {
                    line: 0,
                    character: at as u32,
                },
                window,
                cx,
            );
        });
        self.ask_suggest(open.pane, cx);
        cx.notify();
    }

    /// Keys while the list is up: the arrows pick, Tab finishes, Escape closes, and Enter finishes a
    /// command that is not typed in full (a finished one is sent).
    fn suggest_key(&mut self, pane: usize, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = self.suggest.as_mut().filter(|open| open.pane == pane) else {
            return;
        };
        let count = open.rows.len().max(1);
        match event.keystroke.key.as_str() {
            "down" => open.at = (open.at + 1) % count,
            "up" => open.at = (open.at + count - 1) % count,
            "tab" => self.complete(window, cx),
            "escape" => self.suggest = None,
            "enter" => {
                let typed = self.panes[pane].input.read(cx).value().to_string();
                let sendable = open
                    .rows
                    .get(open.at)
                    .is_some_and(|row| row["fill"] == typed.as_str() && row["more"] != true);
                if sendable {
                    self.suggest = None;
                    return;
                }
                self.complete(window, cx);
            }
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }

    /// "Message all" (or a reply when all the chats show together): one box that sends to every chat.
    fn submit_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.all_input.read(cx).value().trim().to_owned();
        if text.is_empty() {
            return;
        }
        let targets: Vec<String> = self.all_targets.iter().cloned().collect();
        let mut request = json!({
            "request": "StreamerzChat",
            "op": "send",
            "platform": "all",
            "text": text,
            "targets": targets,
            "use_bot": self.all_bot,
            "fallback": self.all_fallback,
            "id": "chat:send"
        });
        if let Some((seq, _, _)) = &self.reply {
            request["replySeq"] = (*seq).into();
        }
        send(request);
        self.all_input.update(cx, |input, cx| input.set_value("", window, cx));
        self.reply = None;
        window.close_dialog(cx);
        cx.notify();
    }

    /// The Message all box in a dialog, for a reply or for every chat at once.
    fn message_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let title = self.texts()["messageAll"].as_str().unwrap_or_default().to_owned();
        let send_label = self.texts()["send"].as_str().unwrap_or_default().to_owned();
        let reply = self.reply.clone();
        let input = self.all_input.clone();
        let owner = cx.entity().downgrade();
        let platforms = self.model["chat"]["platforms"].as_array().cloned().unwrap_or_default();
        input.update(cx, |input, cx| input.focus(window, cx));
        window.open_dialog(cx, move |dialog, _, cx| {
            let owner = owner.clone();
            let theme = cx.theme().clone();
            let texts = owner.upgrade().map(|o| o.read(cx).texts().clone()).unwrap_or_default();
            let (all_targets, all_bot, all_fallback) = owner
                .upgrade()
                .map(|o| {
                    let v = o.read(cx);
                    (v.all_targets.clone(), v.all_bot, v.all_fallback)
                })
                .unwrap_or_default();
            let plat_checks = platforms.iter().map(|p| {
                let id = p["id"].as_str().unwrap_or_default().to_owned();
                let name = p["name"].as_str().unwrap_or_default().to_owned();
                let sends = p["send"] == true;
                let tip = p["tip"].as_str().unwrap_or_default().to_owned();
                let checked = all_targets.contains(&id);
                let owner = owner.clone();
                let target_id = id.clone();
                let mut wrap = div()
                    .id(SharedString::from(format!("msg-all-wrap-{id}")))
                    .flex()
                    .items_center()
                    .gap_1();
                if !tip.is_empty() {
                    let tip = tip.clone();
                    wrap = wrap.tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx));
                }
                wrap.child(
                    Checkbox::new(SharedString::from(format!("msg-all-plat-{id}")))
                        .small()
                        .label(name)
                        .checked(checked)
                        .disabled(!sends)
                        .on_click(move |checked, _, cx| {
                            let target_id = target_id.clone();
                            let is_checked = *checked;
                            let _ = owner.update(cx, |view, _| {
                                if is_checked {
                                    view.all_targets.insert(target_id);
                                } else {
                                    view.all_targets.remove(&target_id);
                                }
                            });
                        }),
                )
            });
            let owner_role = owner.clone();
            let owner_fb = owner.clone();
            let send_owner = owner.clone();
            let buttons = div()
                .flex()
                .gap_2()
                .child(
                    Button::new("message-all-cancel")
                        .label(crate::i18n::tr("dialogs.cancel").to_owned())
                        .on_click(|_, window, cx| window.close_dialog(cx)),
                )
                .child(
                    Button::new("message-all-send")
                        .primary()
                        .icon(Icon::empty().path("icons/send.svg"))
                        .label(send_label.clone())
                        .on_click(move |_, window, cx| {
                            let _ = send_owner.update(cx, |view, cx| view.submit_all(window, cx));
                        }),
                );
            let footer = crate::controls::dialog::footer(None, buttons, cx);
            crate::controls::dialog::shell(dialog, title.clone(), 520., cx)
                .footer(footer)
                .child(crate::controls::dialog::body(
                    div()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .when_some(reply.clone(), |column, (_, who, said)| {
                            column.child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(Icon::empty().path("icons/corner-up-left.svg").size_3().flex_shrink_0())
                                    .child(div().min_w_0().truncate().child(format!("@{who}: {said}"))),
                            )
                        })
                        .child(div().flex().flex_wrap().items_center().gap_3().children(plat_checks))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .gap_2()
                                .child(
                                    Checkbox::new("msg-all-bot")
                                        .small()
                                        .label(texts["sendAs"].as_str().unwrap_or("Bot").to_owned())
                                        .checked(all_bot)
                                        .on_click(move |checked, _, cx| {
                                            let is_checked = *checked;
                                            let _ = owner_role.update(cx, |view, _| view.all_bot = is_checked);
                                        }),
                                )
                                .child(
                                    Checkbox::new("msg-all-fallback")
                                        .small()
                                        .label(texts["fallback"].as_str().unwrap_or("Fallback").to_owned())
                                        .checked(all_fallback)
                                        .on_click(move |checked, _, cx| {
                                            let is_checked = *checked;
                                            let _ = owner_fb.update(cx, |view, _| view.all_fallback = is_checked);
                                        }),
                                ),
                        )
                        .child(Input::new(&input).small()),
                ))
        });
    }

    /// An answer to one of this page's requests (ids `chat:...`).
    pub fn apply(&mut self, operation: &str, value: Value, window: &mut Window, cx: &mut Context<Self>) {
        let failed = value["status"] == "error";
        match operation {
            "read" => {
                if failed {
                    self.reading = false;
                    self.error = value["error"].as_str().unwrap_or_default().to_owned();
                    cx.notify();
                } else {
                    self.error.clear();
                    self.update_model(value, window, cx);
                }
            }
            "images" => {
                self.asking_images = false;
                if let Some(files) = value["files"].as_object() {
                    if self.images.len() > REMEMBERED {
                        self.images.clear();
                    }
                    for (url, file) in files {
                        if let Some(file) = file.as_str() {
                            self.images.insert(url.clone(), PathBuf::from(file));
                            self.missing.remove(url);
                        }
                    }
                }
                // What the core is still fetching comes with the images topic.
                if let Some(open) = self.card.clone() {
                    let images = self.card_images(&open.read(cx).card);
                    open.update(cx, |open, cx| {
                        open.images = images;
                        cx.notify();
                    });
                }
                cx.notify();
            }
            "card" => {
                if failed {
                    self.error = value["error"].as_str().unwrap_or_default().to_owned();
                } else {
                    self.error.clear();
                    self.show_card(value["card"].clone(), window, cx);
                }
                cx.notify();
            }
            "options" => {
                if !failed {
                    let sections = cx.new(|cx| Sections::new(value, "chat:opt".into(), window, cx));
                    self.options = Some(sections.clone());
                    let owner = cx.entity().downgrade();
                    let title = self.texts()["options"].as_str().unwrap_or_default().to_owned();
                    window.open_dialog(cx, move |dialog, _, cx| {
                        let owner = owner.clone();
                        let close = crate::controls::dialog::close_footer(cx);
                        crate::controls::dialog::shell(dialog, title.clone(), 560., cx)
                            .child(crate::controls::dialog::body(
                                div()
                                    .id("chat-options")
                                    .max_h(px(520.))
                                    .overflow_y_scroll()
                                    .pr_2()
                                    .child(sections.clone()),
                            ))
                            .footer(close)
                            .on_cancel(move |_, _, cx| {
                                let _ = owner.update(cx, |view, _| view.options = None);
                                true
                            })
                    });
                }
            }
            "optread" => {
                if let Some(options) = self.options.clone().filter(|_| !failed) {
                    options.update(cx, |options, cx| options.update_model(value, window, cx));
                }
            }
            _ if operation.starts_with("opt:") => {
                if let Some(options) = self.options.clone() {
                    if options.update(cx, |options, cx| {
                        options.apply(&operation["opt:".len()..], value, window, cx)
                    }) {
                        send(json!({"request": "StreamerzCoreView", "domain": "chat_options", "id": "chat:optread"}));
                    }
                }
                self.refresh();
            }
            "emotes" => {
                if let Some(picker) = self.picker.clone() {
                    picker.update(cx, |picker, cx| picker.apply(value, cx));
                }
            }
            "emoteimages" => {
                if let Some(picker) = self.picker.clone() {
                    picker.update(cx, |picker, cx| picker.images(&value, cx));
                }
            }
            _ if operation.starts_with("suggest:") => {
                let pane = operation["suggest:".len()..].parse::<usize>().unwrap_or(0);
                // The words typed, with their emotes drawn as pictures in the box (their pictures asked for).
                let parts = value["preview"].as_array().cloned().unwrap_or_default();
                for part in &parts {
                    if let Some(url) = part["u"].as_str().filter(|url| !self.images.contains_key(*url)) {
                        self.missing.insert(url.to_owned());
                    }
                }
                self.ask_images();
                if !failed && pane < self.panes.len() {
                    self.inline_emotes(pane, &parts, window, cx);
                }
                let found = &value["suggest"];
                let rows = found["rows"].as_array().cloned().unwrap_or_default();
                self.suggest = (!failed && !rows.is_empty() && pane < self.panes.len()).then(|| Suggest {
                    pane,
                    mention: found["mention"] == true,
                    header: found["header"].as_str().unwrap_or_default().to_owned(),
                    start: found["start"].as_u64().unwrap_or(0) as usize,
                    at: self
                        .suggest
                        .as_ref()
                        .filter(|old| old.pane == pane)
                        .map_or(0, |old| old.at.min(rows.len() - 1)),
                    rows,
                });
                cx.notify();
            }
            "card-act" => {
                // A group changed: draw the card again with what the core has now.
                if let Some(open) = self.card.clone() {
                    let seq = open.read(cx).card["seq"].clone();
                    send(
                        json!({"request": "StreamerzChat", "op": "card", "seq": seq, "lookup": true, "id": "chat:card"}),
                    );
                }
            }
            "send" | "act" | "show" | "clear" | "sender" | "card-open" | "prefetch" | "reconnect" => {
                self.error = if failed {
                    value["error"].as_str().unwrap_or_default().to_owned()
                } else {
                    String::new()
                };
                cx.notify();
            }
            _ => {}
        }
    }

    pub(super) fn card_urls(card: &Value) -> Vec<String> {
        let mut urls: Vec<String> = [card["picture"].as_str(), card["banner"].as_str()]
            .into_iter()
            .flatten()
            .chain(
                card["badges"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|badge| badge["url"].as_str()),
            )
            .map(str::to_owned)
            .collect();
        for line in card["messages"].as_array().into_iter().flatten() {
            urls.extend(
                line["parts"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|part| part["u"].as_str())
                    .chain(
                        line["badges"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(|badge| badge["url"].as_str()),
                    )
                    .chain(line["gif"].as_str())
                    .map(str::to_owned),
            );
        }
        urls.retain(|url| !url.is_empty());
        urls
    }

    /// The pictures of a card that the page already has.
    fn card_images(&self, card: &Value) -> HashMap<String, PathBuf> {
        Self::card_urls(card)
            .into_iter()
            .filter_map(|url| self.images.get(&url).map(|file| (url, file.clone())))
            .collect()
    }

    /// Ask for the pictures a card needs and the page does not have.
    fn want_card(&mut self, card: &Value) {
        for url in Self::card_urls(card) {
            if !self.images.contains_key(&url) {
                self.missing.insert(url);
            }
        }
        self.ask_images();
    }

    /// Open the card, or fill the open one with what came after. A card still waiting for the platform asks
    /// again in a moment.
    fn show_card(&mut self, card: Value, window: &mut Window, cx: &mut Context<Self>) {
        self.want_card(&card);
        let images = self.card_images(&card);
        if let Some(open) = self.card.clone() {
            if open.read(cx).card["id"] == card["id"] {
                let again = card["pending"] == true;
                open.update(cx, |open, cx| {
                    open.card = card;
                    open.images = images;
                    cx.notify();
                });
                if again {
                    self.ask_card_later(cx);
                }
                return;
            } else {
                let again = card["pending"] == true;
                let seq = card["seq"].clone();
                open.update(cx, |open, cx| {
                    open.card = card;
                    open.images = images;
                    open.details = false;
                    cx.notify();
                });
                if again {
                    self.ask_card_later(cx);
                }
                send(json!({"request": "StreamerzChat", "op": "card", "seq": seq, "lookup": true, "id": "chat:card"}));
                return;
            }
        }
        if card["loading"] != true {
            return;
        }
        let seq = card["seq"].clone();
        let owner = cx.entity().downgrade();
        let body = cx.new(|_| CardView {
            owner: owner.clone(),
            card,
            images,
            details: false,
        });
        self.card = Some(body.clone());
        window.open_dialog(cx, move |dialog, _, _| {
            let owner_close = owner.clone();
            let owner_cancel = owner.clone();
            dialog
                .centered()
                .overflow_hidden()
                .close_button(false)
                .pt(px(0.))
                .pb(px(0.))
                .pl(px(0.))
                .pr(px(0.))
                .width(px(520.))
                .child(body.clone())
                .on_cancel(move |_, _, cx| {
                    let _ = owner_cancel.update(cx, |view, _| view.card = None);
                    true
                })
                .on_close(move |_, _, cx| {
                    let _ = owner_close.update(cx, |view, _| view.card = None);
                })
        });
        send(json!({"request": "StreamerzChat", "op": "card", "seq": seq, "lookup": true, "id": "chat:card"}));
    }

    /// The platform has not finished: ask what it has said since, in a moment.
    fn ask_card_later(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(std::time::Duration::from_millis(350)).await;
            let _ = this.update(cx, |view, cx| {
                if let Some(open) = view.card.clone() {
                    let seq = open.read(cx).card["seq"].clone();
                    send(json!({"request": "StreamerzChat", "op": "card", "seq": seq, "lookup": true, "id": "chat:card"}));
                }
            });
        })
        .detach();
    }

    fn texts(&self) -> &Value {
        &self.model["chat"]["texts"]
    }
}

/// The open user card: the page updates it when the platform's facts arrive.
struct CardView {
    owner: WeakEntity<ChatView>,
    card: Value,
    images: HashMap<String, PathBuf>,
    /// The box shows what actions get for them, not what they said.
    details: bool,
}
type CardClose = Option<Rc<dyn Fn(&mut Window, &mut App)>>;

impl Render for CardView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let this = cx.entity();
        let toggle = move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
            this.update(cx, |view, cx| {
                view.details = !view.details;
                cx.notify();
            });
        };
        let owner = self.owner.clone();
        let on_close = Rc::new(move |window: &mut Window, cx: &mut App| {
            window.close_dialog(cx);
            let _ = owner.update(cx, |view, _| view.card = None);
        });
        card_body(
            "chat",
            &self.card,
            &self.images,
            self.details,
            toggle,
            Some(on_close),
            cx,
        )
    }
}

/// A picture of the core's cache at `height`, as wide as its own proportions make it (a wide emote stays
/// wide, a tall one tall), or an empty box of the same height while it is on its way.
/// Where the emotes of what is typed go: `parts` is the core's split of `text` (its words and emotes, in
/// order), `old` the pictures the box shows now (range, picture URL) and `caret` where the caret is.
/// A word still being typed at the caret waits until it ends, a picture the box has keeps its own, and
/// one the core does not know (picked from the list) stays while it is a word of its own. `None` when
/// the parts are not of this text.
fn emote_spans(
    text: &str,
    parts: &[Value],
    old: &[(Range<usize>, String)],
    caret: Option<usize>,
) -> Option<Vec<(Range<usize>, String)>> {
    let mut spans: Vec<(Range<usize>, String)> = Vec::new();
    let mut at = 0;
    for part in parts {
        if let Some(plain) = part["t"].as_str() {
            at += plain.len();
            continue;
        }
        let name = part["e"].as_str()?;
        let range = at..at + name.len();
        at = range.end;
        if text.get(range.clone()) != Some(name) {
            return None;
        }
        let Some(url) = part["u"].as_str().filter(|url| !url.is_empty()) else {
            continue;
        };
        match old.iter().find(|(had, _)| *had == range) {
            Some((_, id)) => spans.push((range, id.clone())),
            None if caret == Some(range.end) => {}
            None => spans.push((range, url.to_owned())),
        }
    }
    if !parts.is_empty() && at != text.len() {
        return None;
    }
    for (range, id) in old {
        let alone = text[..range.start].chars().next_back().is_none_or(|c| c == ' ')
            && text[range.end..].chars().next().is_none_or(|c| c == ' ');
        let taken = spans
            .iter()
            .any(|(span, _)| span.start < range.end && range.start < span.end);
        if alone && !taken {
            spans.push((range.clone(), id.clone()));
        }
    }
    spans.sort_by_key(|(range, _)| range.start);
    Some(spans)
}

/// Put a picked emote at the caret as one picture in the text, with a space on each side.
fn put_emote(input: &mut InputState, name: &str, url: &str, window: &mut Window, cx: &mut Context<InputState>) {
    let start = input.selected_range().start;
    if input.value()[..start].chars().next_back().is_some_and(|c| c != ' ') {
        input.replace(" ", window, cx);
    }
    let token = InlineToken::new(url.to_owned(), name.to_owned());
    if url.is_empty() || input.replace_with_token(token, window, cx).is_err() {
        input.replace(name.to_owned(), window, cx);
    }
    let caret = input.cursor();
    if input.value()[caret..].starts_with(' ') {
        input.set_selected_range(caret + 1..caret + 1, cx);
    } else {
        input.insert(" ", window, cx);
    }
}

/// An emote in the send box: its picture as tall as the line (its name until the picture is here).
fn emote_token(token: &InlineTokenContext, owner: &WeakEntity<ChatView>, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let file = owner
        .upgrade()
        .and_then(|view| view.read(cx).images.get(token.token().id().as_ref()).cloned());
    let height = token.line_height();
    div()
        .flex()
        .items_center()
        .h(height)
        .max_w(token.available_width())
        .rounded(theme.radius)
        .when(token.is_selected(), |shown| shown.bg(theme.selection))
        .when(token.is_disabled(), |shown| shown.opacity(0.5))
        .child(match file {
            Some(file) => img(file)
                .h(height)
                .flex_shrink_0()
                .object_fit(ObjectFit::Contain)
                .into_any_element(),
            None => div().child(token.token().label().clone()).into_any_element(),
        })
        .into_any_element()
}

fn picture(file: Option<&PathBuf>, height: f32, id: impl Into<ElementId>) -> AnyElement {
    match file {
        Some(file) => img(file.clone())
            .id(id)
            .h(px(height))
            .flex_shrink_0()
            .object_fit(ObjectFit::Contain)
            .into_any_element(),
        None => div().w(px(height)).h(px(height)).flex_shrink_0().into_any_element(),
    }
}

fn color(hex: &str, fallback: Hsla) -> Hsla {
    u32::from_str_radix(hex.trim_start_matches('#'), 16)
        .ok()
        .filter(|_| hex.trim_start_matches('#').len() == 6)
        .map_or(fallback, |value| rgb(value).into())
}

fn brand_color(platform: &str) -> Hsla {
    match platform {
        "twitch" => rgb(0x9146ff).into(),
        "youtube" => rgb(0xff0033).into(),
        "kick" => rgb(0x53fc18).into(),
        "tiktok" => rgb(0x25f4ee).into(),
        _ => rgb(0x888888).into(),
    }
}

/// The user card as the platforms draw theirs: the banner with their picture, name and account age, their
/// badges and groups, what the platform says, what they said, and what we may do to them.
pub(super) fn card_body(
    route: &str,
    card: &Value,
    images: &HashMap<String, PathBuf>,
    details: bool,
    toggle: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    on_close: CardClose,
    cx: &mut App,
) -> impl IntoElement {
    let theme = cx.theme().clone();
    let muted = theme.muted_foreground;
    let seq = card["seq"].as_u64().unwrap_or(0);
    let id = card["id"].as_str().unwrap_or_default().to_owned();
    let label = |key: &str| card["labels"][key].as_str().unwrap_or_default().to_owned();
    let on_banner: Hsla = rgb(0xe4e4e7).into();
    // The banner: their picture behind a dim layer, with the face, the name and when the account began.
    let face = card["picture"].as_str().and_then(|url| images.get(url));
    let pictured = card["banner"].as_str().is_some_and(|url| images.contains_key(url));
    // With no banner of their own the band is only as tall as the face needs (no empty stripe).
    let mut banner = div()
        .relative()
        .h(px(if pictured { 130. } else { 96. }))
        .w_full()
        .flex_shrink_0()
        .overflow_hidden()
        .rounded_t(theme.radius_lg)
        .bg(theme.muted);
    if let Some(file) = card["banner"].as_str().and_then(|url| images.get(url)) {
        banner = banner.child(
            img(file.clone())
                .id("card-banner")
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .object_fit(ObjectFit::Cover),
        );
    }
    banner = banner
        .child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .bg(rgb(0x000000).opacity(0.45)),
        )
        .child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .flex()
                .items_center()
                .gap_3()
                .px_4()
                .child(match face {
                    Some(file) => img(file.clone())
                        .id("card-picture")
                        .size(px(66.))
                        .rounded_full()
                        .flex_shrink_0()
                        .object_fit(ObjectFit::Cover)
                        .into_any_element(),
                    None => div()
                        .size(px(66.))
                        .flex_shrink_0()
                        .rounded_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(rgb(0x000000).opacity(0.35))
                        .child(
                            Icon::empty()
                                .path("icons/circle-user-round.svg")
                                .size(px(40.))
                                .text_color(on_banner),
                        )
                        .into_any_element(),
                })
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .min_w_0()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    img(format!("brands/{}.svg", card["platform"].as_str().unwrap_or("twitch")))
                                        .id("card-platform")
                                        .size(px(22.))
                                        .flex_shrink_0(),
                                )
                                .child(
                                    div()
                                        .font_weight(FontWeight::BOLD)
                                        .text_size(px(22.))
                                        .text_color(rgb(0xffffff))
                                        .truncate()
                                        .child(card["user"].as_str().unwrap_or_default().to_owned()),
                                ),
                        )
                        .when_some(
                            card["handle"].as_str().filter(|text| !text.is_empty()),
                            |column, handle| {
                                column.child(div().text_color(on_banner).truncate().child(handle.to_owned()))
                            },
                        )
                        .when_some(
                            card["createdText"].as_str().filter(|text| !text.is_empty()),
                            |column, text| {
                                column.child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_1()
                                        .text_color(on_banner)
                                        .child(Icon::empty().path("icons/calendar.svg").size(px(14.)))
                                        .child(text.to_owned()),
                                )
                            },
                        ),
                ),
        )
        .child(
            div()
                .absolute()
                .top(px(8.))
                .right(px(8.))
                .flex()
                .gap_1()
                .child(
                    Button::new("card-profile")
                        .small()
                        .ghost()
                        .icon(Icon::empty().path("icons/external-link.svg"))
                        .tooltip(label("profile"))
                        .accessibility_label(label("profile"))
                        .on_click({
                            let (id, route) = (id.clone(), route.to_owned());
                            move |_, _, _| {
                                send(json!({"request": "StreamerzCoreAction", "domain": "community",
                                    "action": "open_profile", "itemId": id, "id": &format!("{route}:card-open")}))
                            }
                        }),
                )
                .child(
                    Button::new("card-close")
                        .small()
                        .ghost()
                        .icon(Icon::empty().path("icons/x.svg"))
                        .tooltip(label("close"))
                        .accessibility_label(label("close"))
                        .on_click({
                            let on_close = on_close.clone();
                            move |_, window, cx| {
                                if let Some(close) = &on_close {
                                    close(window, cx);
                                } else {
                                    window.close_dialog(cx);
                                }
                            }
                        }),
                ),
        );
    // Their badges, and the groups they are in.
    let mut badges = div().flex().items_center().gap_1().min_h(px(24.));
    for (at, badge) in card["badges"].as_array().into_iter().flatten().enumerate() {
        badges = badges.child(badge_icon(
            badge,
            images.get(badge["url"].as_str().unwrap_or_default()),
            at,
            22.,
        ));
    }
    let groups = card["groups"].as_array().cloned().unwrap_or_default();
    let named: Vec<String> = groups
        .iter()
        .filter(|group| group["member"] == true)
        .filter_map(|group| group["name"].as_str().map(str::to_owned))
        .collect();
    let in_groups = if named.is_empty() {
        card["noGroups"].as_str().unwrap_or_default().to_owned()
    } else {
        named.join(", ")
    };
    let (groups_label, groups_tip) = (
        card["groupsLabel"].as_str().unwrap_or_default().to_owned(),
        card["groupsTip"].as_str().unwrap_or_default().to_owned(),
    );
    let groups_menu = groups.clone();
    let group_row = div()
        .flex()
        .items_center()
        .justify_between()
        .gap_2()
        .child(div().min_w_0().truncate().text_color(muted).child(in_groups))
        .when(!groups.is_empty(), |row| {
            row.child(
                Button::new("card-groups")
                    .small()
                    .icon(Icon::empty().path("icons/users.svg"))
                    .label(groups_label.clone())
                    .tooltip(groups_tip.clone())
                    .dropdown_menu({
                        let (id, route) = (id.clone(), route.to_owned());
                        move |mut menu, _, _| {
                            for group in &groups_menu {
                                let route = route.clone();
                                let item = format!("{id}|{}", group["id"].as_str().unwrap_or_default());
                                menu = menu.item(
                                    PopupMenuItem::new(group["name"].as_str().unwrap_or_default().to_owned())
                                        .checked(group["member"] == true)
                                        .on_click(move |_, _, _| {
                                            send(json!({"request": "StreamerzCoreAction", "domain": "community",
                                                "action": "card_group", "itemId": item, "id": format!("{route}:card-act")}))
                                        }),
                                );
                            }
                            menu
                        }
                    }),
            )
        });
    // What the platform says, as a framed table.
    let mut facts = div()
        .flex()
        .flex_col()
        .rounded(theme.radius)
        .border_1()
        .border_color(theme.border)
        .bg(theme.table)
        .px_3()
        .py_1();
    let list = card["facts"].as_array().cloned().unwrap_or_default();
    for fact in &list {
        facts = facts.child(
            div()
                .flex()
                .gap_2()
                .h(px(28.))
                .items_center()
                .child(
                    div()
                        .w(px(200.))
                        .flex_shrink_0()
                        .child(fact["label"].as_str().unwrap_or_default().to_owned()),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .child(fact["value"].as_str().unwrap_or_default().to_owned()),
                ),
        );
    }
    // What they said, drawn as the chat draws lines.
    let messages = card["messages"].as_array().cloned().unwrap_or_default();
    let mut said = div()
        .id("card-messages")
        .h(px(190.))
        .overflow_y_scroll()
        .flex()
        .flex_col()
        .gap_1()
        .p_2()
        .rounded(theme.radius)
        .border_1()
        .border_color(theme.border)
        .bg(theme.table);
    if messages.is_empty() {
        said = said.child(
            div()
                .text_color(muted)
                .child(card["noMessages"].as_str().unwrap_or_default().to_owned()),
        );
    }
    for (at, message) in messages.iter().enumerate() {
        said = said.child(div().flex().child(line_content(message, images, 900_000 + at, &[], cx)));
    }
    // What actions get for them: a click copies the value.
    let copied = card["copied"].as_str().unwrap_or_default().to_owned();
    let mut table = div()
        .id("card-details-table")
        .h(px(190.))
        .overflow_y_scroll()
        .flex()
        .flex_col()
        .px_2()
        .rounded(theme.radius)
        .border_1()
        .border_color(theme.border)
        .bg(theme.table);
    for (at, row) in card["details"].as_array().into_iter().flatten().enumerate() {
        let value = row["value"].as_str().unwrap_or_default().to_owned();
        let copied = copied.clone();
        table = table.child(
            div()
                .id(("card-detail", at))
                .flex()
                .gap_2()
                .min_h(px(26.))
                .items_center()
                .cursor_pointer()
                .child(
                    div()
                        .w(px(170.))
                        .flex_shrink_0()
                        .text_color(muted)
                        .child(row["key"].as_str().unwrap_or_default().to_owned()),
                )
                .child(div().flex_1().min_w_0().truncate().child(value.clone()))
                .on_click(move |_, window, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(value.clone()));
                    window.push_notification(copied.clone(), cx);
                }),
        );
    }
    let mut actions = div().flex().flex_wrap().gap_2();
    for act in card["acts"].as_array().into_iter().flatten().filter_map(Value::as_str) {
        let (icon, secs) = match act {
            "delete" => ("trash-2", 0),
            "timeout" => ("clock", 600),
            "ban" => ("ban", 0),
            _ => continue,
        };
        let name = act.to_owned();
        actions = actions.child(
            Button::new(SharedString::from(format!("card-{act}")))
                .small()
                .icon(Icon::empty().path(format!("icons/{icon}.svg")))
                .label(label(act))
                .on_click({
                    let who = id.split_once('|').map(|(platform, user)| (platform.to_owned(), user.to_owned()));
                    let on_close = on_close.clone();
                    move |_, window, cx| {
                        // A chat line is the one to act on; a person without one is asked by who they are.
                        let mut request = json!({"request": "StreamerzChat", "op": "act", "act": name, "secs": secs, "id": "chat:act"});
                        match (&who, seq) {
                            (Some((platform, user)), 0) => {
                                request["platform"] = platform.clone().into();
                                request["userId"] = user.clone().into();
                            }
                            _ => request["seq"] = seq.into(),
                        }
                        send(request);
                        if let Some(close) = &on_close {
                            close(window, cx);
                        } else {
                            window.close_dialog(cx);
                        }
                    }
                }),
        );
    }
    let has_actions = card["acts"].as_array().is_some_and(|acts| !acts.is_empty());
    div()
        .flex()
        .flex_col()
        .w_full()
        .overflow_hidden()
        .rounded(theme.radius_lg)
        .child(banner)
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .p_4()
                .child(badges)
                .child(group_row)
                .child(facts)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div().font_weight(FontWeight::BOLD).child(
                                card[if details { "detailsLabel" } else { "recent" }]
                                    .as_str()
                                    .unwrap_or_default()
                                    .to_owned(),
                            ),
                        )
                        .child(
                            Button::new("card-details")
                                .small()
                                .label(
                                    card[if details { "recentShort" } else { "detailsLabel" }]
                                        .as_str()
                                        .unwrap_or_default()
                                        .to_owned(),
                                )
                                .on_click(toggle),
                        ),
                )
                .child(if details {
                    table.into_any_element()
                } else {
                    said.into_any_element()
                })
                .when(has_actions, |body| body.child(actions)),
        )
}

fn badge_icon(badge: &Value, file: Option<&PathBuf>, at: usize, size: f32) -> AnyElement {
    match file {
        Some(file) => img(file.clone())
            .id(("badge", at))
            .w(px(size))
            .h(px(size))
            .flex_shrink_0()
            .object_fit(ObjectFit::Contain)
            .into_any_element(),
        None if badge["url"].as_str().is_some_and(|url| !url.is_empty()) => {
            div().w(px(size)).h(px(size)).flex_shrink_0().into_any_element()
        }
        None if badge["art"].is_string() => img(format!("badges/{}.svg", badge["art"].as_str().unwrap_or_default()))
            .id(("badge", at))
            .w(px(size))
            .h(px(size))
            .flex_shrink_0()
            .into_any_element(),
        None => Icon::empty()
            .path(format!("icons/{}.svg", badge["icon"].as_str().unwrap_or("award")))
            .size(px(size))
            .flex_shrink_0()
            .into_any_element(),
    }
}

/// What the pointer shows over an emote or a badge: the picture large, and its name.
#[derive(Clone)]
struct PictureTip {
    file: Option<PathBuf>,
    /// A badge drawn by the app (a file of `assets/badges`), when it has no picture file.
    art: Option<String>,
    name: String,
    height: f32,
}
impl Render for PictureTip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap_1()
            .p_2()
            .rounded(theme.radius)
            .border_1()
            .border_color(theme.border)
            .bg(theme.popover)
            .text_color(theme.foreground)
            .when_some(self.file.clone(), |tip, file| {
                tip.child(img(file).h(px(self.height)).object_fit(ObjectFit::Contain))
            })
            .when_some(self.art.clone(), |tip, art| {
                tip.child(img(format!("badges/{art}.svg")).size(px(self.height)))
            })
            .child(div().text_xs().child(self.name.clone()))
    }
}

/// What one line says: its badges, the name (a click opens the person's card), the event and the words
/// with their pictures. A chat list and a person's card draw lines the same way; `at` keeps ids apart.
fn line_content(line: &Value, images: &HashMap<String, PathBuf>, at: usize, me: &[String], cx: &App) -> AnyElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let seq = line["seq"].as_u64().unwrap_or(0);
    let deleted = line["deleted"] == true;
    let accent = line["accent"].as_str().map(|hex| color(hex, theme.border));
    let name_color = color(line["color"].as_str().unwrap_or_default(), theme.foreground);
    let text_color = if deleted { muted } else { theme.foreground };
    let mut body = div()
        .id(("line-content", at))
        .flex()
        .flex_wrap()
        .items_center()
        .gap_x_1()
        .flex_1()
        .min_w_0();
    if let Some(orig) = line["original"].as_str().filter(|_| deleted) {
        let orig = format!("Deleted: {orig}");
        body = body.tooltip(move |window, cx| Tooltip::new(orig.clone()).build(window, cx));
    }
    for (index, badge) in line["badges"].as_array().into_iter().flatten().enumerate() {
        let file = badge["url"].as_str().and_then(|url| images.get(url));
        let uid = at * 8 + index;
        let tip = PictureTip {
            file: file.cloned(),
            art: badge["art"].as_str().filter(|_| file.is_none()).map(str::to_owned),
            name: badge["label"]
                .as_str()
                .filter(|label| !label.is_empty())
                .unwrap_or_else(|| badge["key"].as_str().unwrap_or_default())
                .to_owned(),
            height: 48.,
        };
        body = body.child(
            div()
                .id(("badge-tip", uid))
                .flex_shrink_0()
                .child(badge_icon(badge, file, uid, 16.))
                .tooltip(move |_, cx| cx.new(|_| tip.clone()).into()),
        );
    }
    let name = line["user"].as_str().unwrap_or_default().to_owned();
    let clickable = line["userId"].as_str().is_some_and(|id| !id.is_empty());
    let user = div()
        .id(("name", at))
        .font_weight(FontWeight::BOLD)
        .text_color(name_color)
        .when(clickable, |name| name.cursor_pointer())
        .child(format!("{name}:"))
        .when(clickable, |name| {
            name.on_click(move |_, _, _| {
                send(json!({"request": "StreamerzChat", "op": "card", "seq": seq, "id": "chat:card"}));
            })
            // Resting on a name asks the platform about them, so the card opens with the answer.
            .on_hover(move |hovered, _, cx| {
                if !*hovered {
                    let _ = HOVERED.compare_exchange(seq, 0, Ordering::Relaxed, Ordering::Relaxed);
                    return;
                }
                HOVERED.store(seq, Ordering::Relaxed);
                let timer = cx.background_executor().clone();
                cx.spawn(async move |_| {
                    timer.timer(std::time::Duration::from_millis(300)).await;
                    if HOVERED.load(Ordering::Relaxed) == seq {
                        send(json!({"request": "StreamerzChat", "op": "card", "seq": seq, "prefetch": true, "id": "chat:prefetch"}));
                    }
                })
                .detach();
            })
        });
    body = body.child(user);
    let tint = accent.unwrap_or(theme.primary);
    if let Some(icon) = line["icon"].as_str().filter(|icon| !icon.is_empty()) {
        body = body.child(
            Icon::empty()
                .path(format!("icons/{icon}.svg"))
                .size(px(14.))
                .text_color(tint),
        );
    }
    if let Some(event) = line["event"].as_str() {
        body = body.child(div().text_color(tint).child(event.to_owned()));
    }
    // What it cost (bits, points), with its picture or its icon.
    if let Some(cost) = line["cost"].as_u64() {
        let mut price = div()
            .flex()
            .items_center()
            .gap_1()
            .text_color(tint)
            .font_weight(FontWeight::BOLD);
        match line["costIcon"].as_str().filter(|icon| !icon.is_empty()) {
            Some(url) if url.starts_with("http") => {
                price = price.child(picture(images.get(url), 16., ("cost", at)));
            }
            Some(icon) => {
                price = price.child(Icon::empty().path(format!("icons/{icon}.svg")).size(px(14.)));
            }
            None => {}
        }
        body = body.child(price.child(cost.to_string()));
    }
    // The picture an event brings (a reward's emote, a giant one drawn big).
    if let Some(file) = line["picture"].as_str().and_then(|url| images.get(url)) {
        let height = if line["huge"] == true { 72. } else { 28. };
        body = body.child(picture(Some(file), height, ("event-picture", at)));
    }
    let action = line["action"] == true;
    let mut emotes = 0;
    let mut words = 0;
    for part in line["parts"].as_array().into_iter().flatten() {
        if let Some(text) = part["t"].as_str() {
            for word in text.split(' ').filter(|word| !word.is_empty()) {
                words += 1;
                let name = word.trim_end_matches(|c: char| !c.is_alphanumeric() && c != '_');
                let is_mention = !deleted && name.len() > 1 && name.starts_with('@');
                let ours = is_mention && me.iter().any(|mine| *mine == name[1..].to_lowercase());
                let (chip, rest) = if is_mention {
                    word.split_at(name.len())
                } else {
                    (word, "")
                };
                let target_user = if is_mention {
                    name[1..].to_owned()
                } else {
                    String::new()
                };
                let line_plat = line["platform"].as_str().unwrap_or("twitch").to_owned();
                let word_el = div().id(("word", at * 256 + words)).when(is_mention, |el| {
                    el.cursor_pointer().on_click(move |_, _, _| {
                        send(json!({
                            "request": "StreamerzChat",
                            "op": "card",
                            "platform": line_plat,
                            "user": target_user,
                            "id": "chat:card"
                        }));
                    })
                });
                body = body.child(
                    word_el
                        .text_color(if action { name_color } else { text_color })
                        .when(action, |word| word.italic())
                        .when(deleted, |word| word.line_through())
                        .when(ours, |word| {
                            word.px_1()
                                .rounded(px(4.))
                                .bg(theme.muted)
                                .border_1()
                                .border_color(theme.border)
                        })
                        .child(chip.to_owned()),
                );
                if !rest.is_empty() {
                    body = body.child(div().text_color(text_color).child(rest.to_owned()));
                }
            }
        } else if let Some(emote) = part["e"].as_str() {
            let size = if part["big"] == true { 72. } else { 24. };
            let file = part["u"].as_str().and_then(|url| images.get(url));
            emotes += 1;
            body = body.child(match file {
                Some(file) => {
                    let tip = PictureTip {
                        file: Some(file.clone()),
                        art: None,
                        name: emote.to_owned(),
                        height: 112.,
                    };
                    div()
                        .id(("emote-tip", at * 64 + emotes))
                        .flex_shrink_0()
                        .child(picture(Some(file), size, ("emote", at * 64 + emotes)))
                        .tooltip(move |_, cx| cx.new(|_| tip.clone()).into())
                        .into_any_element()
                }
                None => div().text_color(muted).child(emote.to_owned()).into_any_element(),
            });
        }
    }
    if let Some(file) = line["gif"].as_str().and_then(|url| images.get(url)) {
        body = body.child(picture(Some(file), 96., ("gif", at)));
    }
    body.into_any_element()
}

impl ChatView {
    /// One line of the list.
    fn row(&self, pane: usize, row: usize, this: &Entity<Self>, cx: &App) -> AnyElement {
        let Some(line) = self.panes[pane].at.get(row).and_then(|at| self.lines.get(*at)) else {
            return div().into_any_element();
        };
        // Element ids are unique across panes.
        let at = pane * 100_000 + row;
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        if let Some(rule) = line["rule"].as_str() {
            return div()
                .flex()
                .items_center()
                .gap_2()
                .px_2()
                .py_1()
                .text_xs()
                .text_color(muted)
                .child(div().flex_1().h(px(1.)).bg(theme.border))
                .child(rule.to_owned())
                .child(div().flex_1().h(px(1.)).bg(theme.border))
                .into_any_element();
        }
        let seq = line["seq"].as_u64().unwrap_or(0);
        if line["note"] == true {
            return div()
                .flex()
                .gap_2()
                .px_2()
                .py_0p5()
                .text_color(muted)
                .when_some(line["time"].as_str(), |row, time| {
                    row.child(div().flex_shrink_0().text_xs().child(time.to_owned()))
                })
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(line["text"].as_str().unwrap_or_default().to_owned()),
                )
                .into_any_element();
        }
        let accent = line["accent"].as_str().map(|hex| color(hex, theme.border));
        let line_id = if seq > 0 { seq as usize } else { at };
        let body = line_content(line, &self.images, line_id, &self.me, cx);
        let mut column = div().flex().flex_col().flex_1().min_w_0().gap_0p5();
        if let Some(tag) = line["tag"].as_str() {
            column = column.child(
                div()
                    .text_xs()
                    .text_color(accent.unwrap_or(muted))
                    .child(tag.to_owned()),
            );
        }
        if let Some(reply) = line["reply"].as_object() {
            column = column.child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .text_xs()
                    .text_color(muted)
                    .child(Icon::empty().path("icons/corner-up-left.svg").size_3().flex_shrink_0())
                    .child(div().truncate().child(format!(
                        "{}: {}",
                        reply["user"].as_str().unwrap_or_default(),
                        reply["text"].as_str().unwrap_or_default()
                    ))),
            );
        }
        column = column.child(body);
        // A column of one chat has its logo in the header; the chats together tell them apart by it.
        let logo = self.panes[pane].platform.is_none().then(|| {
            img(format!("brands/{}.svg", line["platform"].as_str().unwrap_or("twitch")))
                .id(("logo", at))
                .size_4()
                .flex_shrink_0()
                .mt(px(2.))
        });
        let acts: Vec<String> = line["acts"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|act| act.as_str().map(str::to_owned))
            .collect();
        let texts = self.texts().clone();
        let text = line["parts"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|part| part["t"].as_str().or_else(|| part["e"].as_str()).unwrap_or_default())
            .collect::<String>();
        let (user_id, who) = (
            line["userId"].as_str().unwrap_or_default().to_owned(),
            line["user"].as_str().unwrap_or_default().to_owned(),
        );
        let view = this.downgrade();
        let shown = self.shown(seq);
        // The actions of this message, on the line while the pointer is over it (the right click has them too).
        let group = SharedString::from(format!("line-{at}"));
        let mut bar = div()
            .absolute()
            .top(px(-6.))
            .right(px(8.))
            .flex()
            .items_center()
            .gap_0p5()
            .p_0p5()
            .rounded(theme.radius)
            .border_1()
            .border_color(theme.border)
            .bg(theme.popover)
            .invisible()
            .group_hover(group.clone(), |style| style.visible());
        for (index, act) in acts.iter().enumerate() {
            let tip = texts[act.as_str()].as_str().unwrap_or_default().to_owned();
            let icon = match act.as_str() {
                "reply" => "corner-up-left",
                "copy" => "copy",
                "pin" => "pin",
                "unpin" => "pin-off",
                "delete" => "trash-2",
                "timeout" => "clock",
                _ => "ban",
            };
            let button = Button::new(("line-act", at * 8 + index))
                .xsmall()
                .ghost()
                .icon(Icon::empty().path(format!("icons/{icon}.svg")))
                .tooltip(tip.clone())
                .accessibility_label(tip);
            let (view, who, text, act) = (view.clone(), who.clone(), text.clone(), act.clone());
            bar = bar.child(match act.as_str() {
                "reply" => button
                    .on_click(move |_, window, cx| {
                        let (who, text) = (who.clone(), text.clone());
                        let _ = view.update(cx, |this, cx| this.start_reply(seq, who, text, window, cx));
                    })
                    .into_any_element(),
                "copy" => button
                    .on_click(move |_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(text.clone())))
                    .into_any_element(),
                "pin" | "timeout" => {
                    let durations = texts[if act == "pin" { "pins" } else { "timeouts" }].clone();
                    button
                        .dropdown_menu(move |mut menu, _, _| {
                            for choice in durations.as_array().into_iter().flatten() {
                                let (secs, name) =
                                    (choice[0].as_u64().unwrap_or(0), choice[1].as_str().unwrap_or_default().to_owned());
                                let act = act.clone();
                                menu = menu.item(PopupMenuItem::new(name).on_click(move |_, _, _| {
                                    send(json!({"request": "StreamerzChat", "op": "act", "seq": seq, "act": act, "secs": secs, "id": "chat:act"}));
                                }));
                            }
                            menu
                        })
                        .into_any_element()
                }
                _ => button
                    .on_click(move |_, _, _| {
                        send(json!({"request": "StreamerzChat", "op": "act", "seq": seq, "act": act, "id": "chat:act"}));
                    })
                    .into_any_element(),
            });
        }
        let has_bar = !acts.is_empty();
        let hover = theme.muted.opacity(0.35);
        // Ours, waiting for the platform's copy: dimmed until it comes.
        let pending = line["pending"] == true;
        div()
            .id(("line", at))
            .opacity(if pending { shown * 0.5 } else { shown })
            .relative()
            .group(group)
            .hover(move |style| style.bg(hover))
            .flex()
            .gap_2()
            .px_2()
            .py_0p5()
            .when_some(accent, |row, accent| row.border_l_2().border_color(accent))
            .when_some(line["time"].as_str(), |row, time| {
                row.child(div().flex_shrink_0().text_xs().text_color(muted).mt(px(3.)).child(time.to_owned()))
            })
            .children(logo)
            .child(column)
            .when(has_bar, |row| row.child(bar))
            .context_menu(move |mut menu, window, cx| {
                let label = |key: &str| texts[key].as_str().unwrap_or_default().to_owned();
                let item = |text: String, icon: &str| PopupMenuItem::new(text).icon(Icon::empty().path(format!("icons/{icon}.svg")));
                if !user_id.is_empty() {
                    let view = view.clone();
                    menu = menu.item(item(label("userInfo"), "user").on_click(move |_, _, cx| {
                        let _ = view.update(cx, |_, _| {
                            send(json!({"request": "StreamerzChat", "op": "card", "seq": seq, "id": "chat:card"}));
                        });
                    }));
                }
                for act in &acts {
                    match act.as_str() {
                        "reply" => {
                            let (view, who, text) = (view.clone(), who.clone(), text.clone());
                            menu = menu.item(item(label("reply"), "corner-up-left").on_click(move |_, window, cx| {
                                let (who, text) = (who.clone(), text.clone());
                                let _ = view.update(cx, |this, cx| this.start_reply(seq, who, text, window, cx));
                            }));
                        }
                        "copy" => {
                            let text = text.clone();
                            menu = menu.item(item(label("copy"), "copy").on_click(move |_, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                            }));
                        }
                        "pin" | "timeout" => {
                            let (act, durations) = (act.clone(), texts[if act == "pin" { "pins" } else { "timeouts" }].clone());
                            menu = menu.submenu(label(&act), window, cx, move |mut submenu, _, _| {
                                for choice in durations.as_array().into_iter().flatten() {
                                    let (secs, name) = (choice[0].as_u64().unwrap_or(0), choice[1].as_str().unwrap_or_default().to_owned());
                                    let act = act.clone();
                                    submenu = submenu.item(PopupMenuItem::new(name).on_click(move |_, _, _| {
                                        send(json!({"request": "StreamerzChat", "op": "act", "seq": seq, "act": act, "secs": secs, "id": "chat:act"}));
                                    }));
                                }
                                submenu
                            });
                        }
                        other => {
                            let (act, icon) = (other.to_owned(), match other {
                                "unpin" => "pin-off",
                                "delete" => "trash-2",
                                _ => "ban",
                            });
                            menu = menu.item(item(label(other), icon).on_click(move |_, _, _| {
                                send(json!({"request": "StreamerzChat", "op": "act", "seq": seq, "act": act, "id": "chat:act"}));
                            }));
                        }
                    }
                }
                menu
            })
            .into_any_element()
    }
}

impl ChatView {
    /// Who the box sends as on the chosen platform: a round account button, and the broadcaster and the
    /// bot to pick from (the core decides who can send; TikTok has one browser account, nothing to pick).
    fn sender_button(&self, platform: &Value, texts: &Value, cx: &mut Context<Self>) -> impl IntoElement {
        let id = platform["id"].as_str().unwrap_or_default().to_owned();
        let tip = platform["sender"]["tip"].as_str().unwrap_or_default().to_owned();
        let choices = platform["senders"].as_array().cloned().unwrap_or_default();
        let text = |key: &str| texts[key].as_str().unwrap_or_default().to_owned();
        let (title, manage, nobody) = (text("senderTitle"), text("manageAccounts"), text("notLoggedIn"));
        let view = cx.entity();
        let signed_in = platform["sender"]["name"].is_string();
        let face = platform["sender"]["picture"]
            .as_str()
            .filter(|file| !file.is_empty())
            .map(PathBuf::from);
        Button::new(SharedString::from(format!("chat-sender-{id}")))
            .small()
            .ghost()
            .when_some(face.clone(), |button, file| {
                button.child(img(file).size(px(22.)).rounded_full().object_fit(ObjectFit::Cover))
            })
            .when(face.is_none(), |button| {
                button.icon(Icon::empty().path("icons/circle-user-round.svg"))
            })
            .when(!signed_in, |button| button.opacity(0.5))
            .tooltip(tip.clone())
            .accessibility_label(tip)
            .dropdown_menu(move |mut menu, _, _| {
                menu = menu.item(PopupMenuItem::label(title.clone()));
                for choice in &choices {
                    let role = choice["role"].as_str().unwrap_or_default().to_owned();
                    let name = choice["name"].as_str().map(str::to_owned);
                    let label = format!(
                        "{}: {}",
                        choice["label"].as_str().unwrap_or_default(),
                        name.clone().unwrap_or_else(|| nobody.clone())
                    );
                    let platform = id.clone();
                    menu = menu.item(
                        PopupMenuItem::new(label)
                            .checked(choice["current"] == true)
                            .disabled(name.is_none())
                            .on_click(move |_, _, _| {
                                send(json!({"request": "StreamerzChat", "op": "sender", "platform": platform,
                                    "broadcaster": role == "broadcaster", "id": "chat:sender"}));
                            }),
                    );
                }
                let view = view.clone();
                menu.item(PopupMenuItem::separator()).item(
                    PopupMenuItem::new(manage.clone())
                        .icon(Icon::empty().path("icons/user.svg"))
                        .on_click(move |_, _, cx| {
                            view.update(cx, |_, cx| cx.emit(Go("Accounts".into())));
                        }),
                )
            })
    }
}

impl ChatView {
    /// Reply to a line: the chats together answer in the Message all box, a column in its own box.
    fn start_reply(&mut self, seq: u64, who: String, said: String, window: &mut Window, cx: &mut Context<Self>) {
        let platform = self
            .lines
            .iter()
            .find(|line| line["seq"] == seq)
            .and_then(|line| line["platform"].as_str())
            .map(str::to_owned);
        self.reply = Some((seq, who, said));
        let view = self.model["chat"]["view"].as_str().unwrap_or("all").to_owned();
        let pane = platform.and_then(|platform| {
            self.panes
                .iter()
                .position(|pane| pane.platform == Some(platform.as_str()))
        });
        match (view.as_str(), pane) {
            ("all", _) | (_, None) => self.message_all(window, cx),
            (_, Some(pane)) => {
                let input = self.panes[pane].input.clone();
                input.update(cx, |input, cx| input.focus(window, cx));
            }
        }
        cx.notify();
    }

    /// The Show box: all together, side by side, or one of the chats that are there.
    fn sync_view(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let chat = &self.model["chat"];
        let mut ids = vec!["all".to_owned(), "side".to_owned()];
        let mut names = vec![
            chat["texts"]["all"].as_str().unwrap_or("All").to_owned(),
            chat["texts"]["side"].as_str().unwrap_or("Side by side").to_owned(),
        ];
        for platform in chat["platforms"].as_array().into_iter().flatten() {
            if platform["present"] == true {
                ids.push(platform["id"].as_str().unwrap_or_default().to_owned());
                names.push(platform["name"].as_str().unwrap_or_default().to_owned());
            }
        }
        let at = ids
            .iter()
            .position(|id| id == chat["view"].as_str().unwrap_or("all"))
            .unwrap_or(0);
        if let (true, Some(select)) = (ids == self.view_ids, &self.view_select) {
            if select.read(cx).selected_index(cx).map(|index| index.row) != Some(at) {
                select.update(cx, |select, cx| {
                    select.set_selected_index(Some(IndexPath::new(at)), window, cx)
                });
            }
            return;
        }
        let select = cx.new(|cx| SelectState::new(names, Some(IndexPath::new(at)), window, cx));
        self._subscriptions.push(
            cx.subscribe(&select, |view, select, event: &SelectEvent<Vec<String>>, cx| {
                let SelectEvent::Confirm(Some(_)) = event else { return };
                let chosen = select
                    .read(cx)
                    .selected_index(cx)
                    .and_then(|index| view.view_ids.get(index.row));
                if let Some(chosen) = chosen {
                    send(json!({"request": "StreamerzChat", "op": "view", "view": chosen, "id": "chat:view"}));
                }
            }),
        );
        self.view_select = Some(select);
        self.view_ids = ids;
    }

    /// A column's header: the chat's logo, the channel's picture and its name.
    fn head(&self, platform: &Value, cx: &App) -> Div {
        let id = platform["id"].as_str().unwrap_or("twitch");
        let name = platform["name"].as_str().unwrap_or_default();
        let room = platform["room"]["name"].as_str().unwrap_or_default();
        let shown = if room.is_empty() {
            name.to_owned()
        } else {
            format!("{name}: {room}")
        };
        let face = platform["room"]["picture"]
            .as_str()
            .filter(|file| !file.is_empty())
            .map(PathBuf::from);
        div()
            .flex()
            .items_center()
            .gap_2()
            .h(px(24.))
            .flex_shrink_0()
            .child(
                img(format!("brands/{id}.svg"))
                    .id(SharedString::from(format!("head-logo-{id}")))
                    .size(px(18.))
                    .flex_shrink_0(),
            )
            .child(match face {
                Some(file) => img(file)
                    .id(SharedString::from(format!("head-face-{id}")))
                    .size(px(22.))
                    .rounded_full()
                    .flex_shrink_0()
                    .object_fit(ObjectFit::Cover)
                    .into_any_element(),
                None => Icon::empty()
                    .path("icons/circle-user-round.svg")
                    .size(px(18.))
                    .text_color(cx.theme().muted_foreground)
                    .into_any_element(),
            })
            .child(div().font_weight(FontWeight::BOLD).min_w_0().truncate().child(shown))
            .when_some(
                platform["viewers"].as_str().filter(|_| platform["connected"] == true),
                |head, viewers| {
                    head.child(
                        div()
                            .ml_auto()
                            .flex_shrink_0()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(viewers.to_owned()),
                    )
                },
            )
    }

    /// One column: its header, its lines and its send box. `platform` is `None` for the chats together,
    /// which have neither a header nor a send box (Message all answers there).
    fn column(&mut self, pane: usize, platform: Option<Value>, cx: &mut Context<Self>) -> Div {
        let chat = self.model["chat"].clone();
        let texts = chat["texts"].clone();
        let text = |key: &str| texts[key].as_str().unwrap_or_default().to_owned();
        let theme = cx.theme().clone();
        let this = cx.entity();
        let framed = |content: Div| {
            content
                .rounded(theme.radius)
                .border_1()
                .border_color(theme.border)
                .bg(theme.table)
        };
        let mut column = div().relative().flex().flex_col().gap_1().flex_1().min_w_0().min_h_0();
        if let Some(platform) = &platform {
            column = column.child(self.head(platform, cx));
        }
        let empty = match &platform {
            None => chat["connected"] == false && self.lines.is_empty(),
            Some(platform) => platform["connected"] == false && self.panes[pane].seqs.is_empty(),
        };
        let list = if chat["history"] == false {
            framed(div())
                .flex_1()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_1()
                .text_color(theme.muted_foreground)
                .child(div().font_weight(FontWeight::BOLD).child(text("historyOff")))
                .child(div().child(text("historyOffAbout")))
                .into_any_element()
        } else if empty {
            framed(div())
                .pt(px(22.))
                .flex_1()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_2()
                .text_color(theme.muted_foreground)
                .child(div().child(text("empty")))
                .child(
                    Button::new(SharedString::from(format!("chat-accounts-{pane}")))
                        .small()
                        .mt_6()
                        .h(px(26.))
                        .w(px(126.))
                        .icon(Icon::empty().path("icons/user.svg"))
                        .label(text("accounts"))
                        .on_click(cx.listener(|_, _, _, cx| cx.emit(Go("Accounts".into())))),
                )
                .into_any_element()
        } else {
            let rows = this.clone();
            let scroller = self.panes[pane].scroller.clone();
            framed(div())
                .flex_1()
                .min_h_0()
                .child(
                    MessageScroller::new(
                        SharedString::from(format!("chat-lines-{pane}")),
                        scroller,
                        move |at, _, cx| rows.read(cx).row(pane, at, &rows, cx),
                    )
                    .with_row_style(StyleRefinement::default().pb_0().px_0())
                    .with_jump_button_label(text("live")),
                )
                .into_any_element()
        };
        column = column.child(list);
        let Some(platform) = platform else {
            return column;
        };
        let id = platform["id"].as_str().unwrap_or_default().to_owned();
        // A chat that is down has no send box: it says so and offers to reconnect or hide it.
        if platform["connected"] == false {
            let (again, away) = (id.clone(), id.clone());
            return column.child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap_1()
                    .h(px(36.))
                    .flex_shrink_0()
                    .child(div().text_xs().text_color(theme.muted_foreground).child(text("disconnected")))
                    .child(
                        Button::new(SharedString::from(format!("chat-reconnect-{pane}")))
                            .small()
                            .ghost()
                            .icon(Icon::empty().path("icons/refresh-cw.svg"))
                            .tooltip(text("reconnect"))
                            .accessibility_label(text("reconnect"))
                            .on_click(move |_, _, _| {
                                send(json!({"request": "StreamerzChat", "op": "reconnect", "platform": again, "id": "chat:reconnect"}))
                            }),
                    )
                    .child(
                        Button::new(SharedString::from(format!("chat-hide-{pane}")))
                            .small()
                            .ghost()
                            .icon(Icon::empty().path("icons/eye-off.svg"))
                            .tooltip(text("hideChat"))
                            .accessibility_label(text("hideChat"))
                            .on_click(move |_, _, _| {
                                send(json!({"request": "StreamerzChat", "op": "show", "platform": away, "shown": false, "id": "chat:show"}))
                            }),
                    ),
            );
        }
        // The send box: who sends, what, the emotes and Send.
        if let Some((seq, who, said)) = self.reply.clone().filter(|(seq, _, _)| {
            self.lines
                .iter()
                .any(|line| line["seq"] == *seq && line["platform"] == id.as_str())
        }) {
            let _ = seq;
            column = column.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(Icon::empty().path("icons/corner-up-left.svg").size_3().flex_shrink_0())
                    .child(div().flex_1().min_w_0().truncate().child(format!("@{who}: {said}")))
                    .child(
                        Button::new(SharedString::from(format!("reply-cancel-{pane}")))
                            .xsmall()
                            .ghost()
                            .icon(Icon::empty().path("icons/x.svg"))
                            .tooltip(text("dontReply"))
                            .accessibility_label(text("dontReply"))
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.reply = None;
                                cx.notify();
                            })),
                    ),
            );
        }
        let can_send = platform["send"] == true && chat["history"] != false;
        let input = self.panes[pane].input.clone();
        let composer = div()
            .flex()
            .items_center()
            .gap_2()
            .flex_shrink_0()
            .child(self.sender_button(&platform, &texts, cx))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    // Before the box sees them: the arrows, Tab, Escape and Enter belong to the list while it is up.
                    .capture_key_down(cx.listener(move |view, event: &KeyDownEvent, window, cx| {
                        view.suggest_key(pane, event, window, cx)
                    }))
                    .child(
                        Input::new(&input)
                            .small()
                            .disabled(!can_send)
                            .aria_label(text("sendPlaceholder"))
                            .token({
                                let owner = cx.entity().downgrade();
                                move |token, _, cx| emote_token(token, &owner, cx)
                            }),
                    ),
            )
            .child({
                let is_picker_open = self.picker.is_some() && self.asking == Some(pane);
                let owner = cx.entity().downgrade();
                let owner_change = owner.clone();
                Popover::new(SharedString::from(format!("chat-emotes-popover-{pane}")))
                    .anchor(Anchor::BottomRight)
                    .open(is_picker_open)
                    .on_open_change(move |open, window, cx| {
                        let _ = owner_change.update(cx, |view, cx| {
                            if *open {
                                if can_send {
                                    view.open_picker(pane, window, cx);
                                }
                            } else {
                                view.close_picker(window, cx);
                            }
                        });
                    })
                    .trigger(
                        Button::new(SharedString::from(format!("chat-emotes-{pane}")))
                            .small()
                            .ghost()
                            .icon(Icon::empty().path("icons/smile.svg"))
                            .disabled(!can_send)
                            .tooltip(text("emoteTip"))
                            .accessibility_label(text("emoteTip")),
                    )
                    .content(move |_, _, cx| {
                        if let Some(chat) = owner.upgrade() {
                            if let Some(picker) = chat.read(cx).picker.clone() {
                                return div().child(picker).into_any_element();
                            }
                        }
                        div().into_any_element()
                    })
            })
            .child(
                Button::new(SharedString::from(format!("chat-send-{pane}")))
                    .small()
                    .ghost()
                    .child(
                        Icon::empty()
                            .path("icons/send-horizontal.svg")
                            .size(px(18.))
                            .text_color(if can_send {
                                theme.primary
                            } else {
                                theme.muted_foreground
                            }),
                    )
                    .disabled(!can_send)
                    .tooltip(text("send"))
                    .accessibility_label(text("send"))
                    .on_click(cx.listener(move |view, _, window, cx| view.submit(pane, window, cx))),
            );
        // The commands or names that fit what is typed, over the list above the box.
        if let Some(open) = self.suggest.as_ref().filter(|open| open.pane == pane) {
            let mut popup = div()
                .absolute()
                .left_0()
                .bottom(px(40.))
                .w(px(420.))
                .max_w_full()
                .flex()
                .flex_col()
                .gap_0p5()
                .p_2()
                .rounded(theme.radius_lg)
                .border_1()
                .border_color(theme.border)
                .bg(theme.popover)
                .occlude()
                .child(
                    div()
                        .px_2()
                        .text_size(px(11.))
                        .font_weight(FontWeight::BOLD)
                        .text_color(theme.muted_foreground)
                        .child(open.header.clone()),
                );
            for (index, row) in open.rows.iter().enumerate() {
                popup = popup.child(
                    div()
                        .id(("suggest", pane * 16 + index))
                        .flex()
                        .flex_col()
                        .px_2()
                        .py_1()
                        .rounded(theme.radius)
                        .cursor_pointer()
                        .when(index == open.at, |row| row.bg(theme.muted))
                        .hover(|row| row.bg(theme.muted))
                        .child(
                            div()
                                .flex()
                                .gap_2()
                                .child(
                                    div()
                                        .font_weight(FontWeight::BOLD)
                                        .child(row["title"].as_str().unwrap_or_default().to_owned()),
                                )
                                .child(
                                    div()
                                        .text_color(theme.muted_foreground)
                                        .child(row["args"].as_str().unwrap_or_default().to_owned()),
                                ),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(row["about"].as_str().unwrap_or_default().to_owned()),
                        )
                        .on_click(cx.listener(move |view, _, window, cx| {
                            if let Some(open) = view.suggest.as_mut() {
                                open.at = index;
                            }
                            view.complete(window, cx);
                        })),
                );
            }
            column = column.child(popup);
        }
        column.child(composer)
    }
}

impl Render for ChatView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let narrow = f32::from(window.viewport_size().width) < 760.;
        let chat = self.model["chat"].clone();
        let texts = chat["texts"].clone();
        let text = |key: &str| texts[key].as_str().unwrap_or_default().to_owned();
        let theme = cx.theme().clone();
        let platforms = chat["platforms"].as_array().cloned().unwrap_or_default();
        let present: Vec<Value> = platforms
            .iter()
            .filter(|platform| platform["present"] == true)
            .cloned()
            .collect();
        let idle = chat["connected"] == false && self.lines.is_empty();
        let history_off = chat["historyDisabled"] == true;
        // The top bar: what the chat shows, then the page's actions.
        let mut shows = div()
            .flex()
            .items_center()
            .gap_2()
            .min_w_0()
            .when(narrow, |shows| shows.w_full())
            .child(div().child(text("show")));
        if let Some(select) = &self.view_select {
            shows = shows.child(
                div()
                    .when(narrow, |select| select.flex_1())
                    .when(!narrow, |select| select.w(px(136.)))
                    .child(Select::new(select).small().disabled(present.is_empty())),
            );
        }
        if let Some(total) = chat["totalViewers"].as_str() {
            let breakdown: String = chat["viewersBreakdown"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|item| {
                            let plat = item.get(0)?.as_str()?;
                            let count = item.get(1)?.as_str()?;
                            Some(format!("{plat}: {count}"))
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default();
            let viewers_tip = if breakdown.is_empty() {
                format!("{}: {total}", text("viewers"))
            } else {
                format!("{}: {total} ({breakdown})", text("viewers"))
            };
            shows = shows.child(
                div()
                    .id("chat-total-viewers")
                    .flex()
                    .items_center()
                    .gap_1()
                    .px_1p5()
                    .py_0p5()
                    .rounded(theme.radius)
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .tooltip(move |window, cx| Tooltip::new(viewers_tip.clone()).build(window, cx))
                    .child(Icon::empty().path("icons/users.svg").size_3().flex_shrink_0())
                    .child(div().child(total.to_owned())),
            );
        }
        let action = |id: &'static str, label: String, icon: &'static str| {
            Button::new(id)
                .small()
                .h(px(26.))
                .min_w(px(if id == "message-all" { 135. } else { 84. }))
                .icon(Icon::empty().path(format!("icons/{icon}.svg")))
                .label(label.clone())
                .accessibility_label(label.clone())
                .tooltip(label)
        };
        let bar = div()
            .flex()
            .items_center()
            .gap_2()
            .mt(px(2.))
            .mb(px(-4.))
            .when(narrow, |bar| bar.flex_wrap().gap_1().mb(px(-2.)))
            .flex_shrink_0()
            .child(shows)
            .when(!narrow, |bar| {
                bar.child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_right()
                        .text_color(theme.muted_foreground)
                        .text_xs()
                        .child(text("hint")),
                )
            })
            .child(
                action("message-all", text("messageAll"), "megaphone")
                    .disabled(idle || history_off)
                    .on_click(cx.listener(|view, _, window, cx| {
                        view.reply = None;
                        view.message_all(window, cx);
                    })),
            )
            .child(
                Button::new("chat-options")
                    .small()
                    .h(px(26.))
                    .min_w(px(84.))
                    .accessibility_label(text("options"))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(text("options"))
                            .child(Icon::empty().path("icons/chevron-down.svg").size_3()),
                    )
                    .on_click(|_, _, _| {
                        send(json!({"request": "StreamerzCoreView", "domain": "chat_options", "id": "chat:options"}));
                    }),
            )
            .child(action("chat-clear", text("clear"), "eraser").on_click(|_, _, _| {
                send(json!({"request": "StreamerzChat", "op": "clear", "id": "chat:clear"}));
            }));
        let mut page = div().size_full().flex().flex_col().gap_2().child(bar);
        if !self.error.is_empty() {
            page = page.child(super::notice::error(&self.error, cx));
        }
        if history_off {
            page = page.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_1p5()
                    .rounded(theme.radius)
                    .bg(theme.muted.opacity(0.4))
                    .border_1()
                    .border_color(theme.border)
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(Icon::empty().path("icons/eye-off.svg").size_3p5().flex_shrink_0())
                    .child(div().child(text("off"))),
            );
        }
        for (poll_idx, poll) in chat["polls"].as_array().into_iter().flatten().enumerate() {
            let platform = poll["platform"].as_str().unwrap_or("twitch");
            let is_prediction = poll["prediction"] == true;
            let kind = if is_prediction {
                text("prediction")
            } else {
                text("poll")
            };
            let title = poll["title"].as_str().unwrap_or_default().to_owned();
            let when = poll["when"].as_str().unwrap_or_default().to_owned();
            let icon_path = if is_prediction {
                "icons/trending-up.svg"
            } else {
                "icons/vote.svg"
            };
            let mut choices_col = div().flex().flex_col().gap_1();
            for choice in poll["choices"].as_array().into_iter().flatten() {
                let label = choice["label"].as_str().unwrap_or_default().to_owned();
                let votes = choice["votes"].as_u64().unwrap_or(0);
                let share = choice["share"].as_f64().unwrap_or(0.0) as f32;
                let is_best = choice["best"] == true && votes > 0;
                let stat = if is_prediction {
                    votes.to_string()
                } else {
                    format!("{}% ({votes})", (share * 100.0).round() as u64)
                };
                choices_col = choices_col.child(
                    div()
                        .relative()
                        .h(px(22.))
                        .rounded(px(4.))
                        .bg(theme.muted.opacity(0.5))
                        .overflow_hidden()
                        .child(
                            div()
                                .relative()
                                .flex()
                                .items_center()
                                .justify_between()
                                .px_2()
                                .size_full()
                                .text_xs()
                                .child(
                                    div()
                                        .min_w_0()
                                        .truncate()
                                        .when(is_best, |t| t.font_weight(FontWeight::BOLD))
                                        .child(label),
                                )
                                .child(
                                    div()
                                        .flex_shrink_0()
                                        .when(is_best, |t| t.font_weight(FontWeight::BOLD))
                                        .child(stat),
                                ),
                        ),
                );
            }
            page = page.child(
                div()
                    .id(SharedString::from(format!("poll-card-{poll_idx}")))
                    .flex()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .rounded(theme.radius)
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.table)
                    .child(
                        div()
                            .w(px(3.))
                            .rounded(px(2.))
                            .bg(brand_color(platform))
                            .flex_shrink_0(),
                    )
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap_1p5()
                            .min_w_0()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_1p5()
                                            .child(Icon::empty().path(icon_path).size_3p5().flex_shrink_0())
                                            .child(div().font_weight(FontWeight::BOLD).child(kind)),
                                    )
                                    .when(!when.is_empty(), |header| header.child(div().child(when))),
                            )
                            .child(div().font_weight(FontWeight::BOLD).text_sm().child(title))
                            .child(choices_col),
                    ),
            );
        }
        for (pin_idx, pin) in chat["pins"].as_array().into_iter().flatten().enumerate() {
            let platform = pin["platform"].as_str().unwrap_or("twitch");
            let user = pin["user"].as_str().unwrap_or_default().to_owned();
            let text_val = pin["text"].as_str().unwrap_or_default().to_owned();
            let by = pin["by"].as_str().unwrap_or_default().to_owned();
            let when = pin["when"].as_str().unwrap_or_default().to_owned();
            let can_unpin = pin["canUnpin"] == true;
            let msg_id = pin["msgId"].as_str().unwrap_or_default().to_owned();
            let plat_id = platform.to_owned();
            page = page.child(
                div()
                    .id(SharedString::from(format!("pin-card-{pin_idx}")))
                    .flex()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .rounded(theme.radius)
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.table)
                    .child(
                        div()
                            .w(px(3.))
                            .rounded(px(2.))
                            .bg(brand_color(platform))
                            .flex_shrink_0(),
                    )
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .min_w_0()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_1()
                                            .child(Icon::empty().path("icons/pin.svg").size_3().flex_shrink_0())
                                            .child(div().child(by)),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_2()
                                            .when(!when.is_empty(), |header| header.child(div().child(when)))
                                            .when(can_unpin && !msg_id.is_empty(), |header| {
                                                header.child(
                                                    Button::new(SharedString::from(format!("unpin-{pin_idx}")))
                                                        .xsmall()
                                                        .ghost()
                                                        .icon(Icon::empty().path("icons/pin-off.svg"))
                                                        .tooltip(text("unpin"))
                                                        .accessibility_label(text("unpin"))
                                                        .on_click(move |_, _, _| {
                                                            send(json!({
                                                                "request": "StreamerzChat",
                                                                "op": "act",
                                                                "platform": plat_id,
                                                                "msgId": msg_id,
                                                                "act": "unpin",
                                                                "id": "chat:act"
                                                            }));
                                                        }),
                                                )
                                            }),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .gap_1p5()
                                    .text_xs()
                                    .child(div().font_weight(FontWeight::BOLD).child(format!("{user}:")))
                                    .child(div().flex_1().min_w_0().truncate().child(text_val)),
                            ),
                    ),
            );
        }
        for line in chat["modes"].as_array().into_iter().flatten() {
            page = page.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(Icon::empty().path("icons/info.svg").size_3().flex_shrink_0())
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .child(line.as_str().unwrap_or_default().to_owned()),
                    ),
            );
        }
        let view = chat["view"].as_str().unwrap_or("all");
        let at_of = |id: &str| {
            PLATFORMS
                .iter()
                .position(|platform| *platform == id)
                .map_or(0, |at| at + 1)
        };
        let body = match view {
            "side" if !present.is_empty() => {
                let mut row = div().flex().gap_2().flex_1().min_h_0();
                for platform in present {
                    let pane = at_of(platform["id"].as_str().unwrap_or_default());
                    row = row.child(self.column(pane, Some(platform), cx));
                }
                row
            }
            id if present.iter().any(|platform| platform["id"] == id) => {
                let platform = present.iter().find(|platform| platform["id"] == id).cloned();
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(self.column(at_of(id), platform, cx))
            }
            _ => div().flex().flex_1().min_h_0().child(self.column(0, None, cx)),
        };
        page.child(body)
    }
}

#[cfg(test)]
mod tests {
    use super::emote_spans;
    use serde_json::json;

    fn url(name: &str) -> String {
        format!("https://e/{name}")
    }

    #[test]
    fn a_finished_emote_word_becomes_a_picture_and_the_one_being_typed_waits() {
        let text = "hi Kappa LUL";
        let parts = [
            json!({"t": "hi "}),
            json!({"e": "Kappa", "u": url("Kappa")}),
            json!({"t": " "}),
            json!({"e": "LUL", "u": url("LUL")}),
        ];
        let spans = emote_spans(text, &parts, &[], Some(text.len())).unwrap();
        assert_eq!(spans, vec![(3..8, url("Kappa"))]);
        // With the caret elsewhere both are pictures.
        let spans = emote_spans(text, &parts, &[], Some(0)).unwrap();
        assert_eq!(spans, vec![(3..8, url("Kappa")), (9..12, url("LUL"))]);
    }

    #[test]
    fn a_picture_keeps_its_own_url_and_one_the_core_does_not_know_stays_while_alone() {
        let text = "mine Kappa ";
        let old = vec![
            (0..4, "https://picked/mine".to_owned()),
            (5..10, "https://picked/kappa".to_owned()),
        ];
        let parts = [
            json!({"t": "mine "}),
            json!({"e": "Kappa", "u": url("Kappa")}),
            json!({"t": " "}),
        ];
        let spans = emote_spans(text, &parts, &old, Some(text.len())).unwrap();
        assert_eq!(spans, old);
    }

    #[test]
    fn a_picture_whose_word_grew_goes_back_to_text() {
        // "Kappa" was a picture and an "s" was typed right after it.
        let text = "Kappas";
        let old = vec![(0..5, url("Kappa"))];
        assert_eq!(emote_spans(text, &[], &old, Some(6)).unwrap(), vec![]);
        // Grown into another emote: that one, once it is not being typed.
        let parts = [json!({"e": "Kappas", "u": url("Kappas")})];
        assert_eq!(emote_spans(text, &parts, &old, Some(6)).unwrap(), vec![]);
        assert_eq!(
            emote_spans(text, &parts, &old, None).unwrap(),
            vec![(0..6, url("Kappas"))]
        );
    }

    #[test]
    fn parts_of_other_text_are_ignored() {
        let parts = [json!({"t": "hello "}), json!({"e": "Kappa", "u": url("Kappa")})];
        assert_eq!(emote_spans("hello Kapp", &parts, &[], None), None);
        assert_eq!(emote_spans("hello Kappa more", &parts, &[], None), None);
    }

    #[test]
    fn emotes_next_to_brackets_and_emoji_are_pictures_too() {
        let text = "RainTime[PETPET] 😀 ";
        let parts = [
            json!({"e": "RainTime", "u": url("RainTime")}),
            json!({"t": "["}),
            json!({"e": "PETPET", "u": url("PETPET")}),
            json!({"t": "] "}),
            json!({"e": "😀", "u": url("smile")}),
            json!({"t": " "}),
        ];
        let spans = emote_spans(text, &parts, &[], Some(text.len())).unwrap();
        assert_eq!(
            spans,
            vec![(0..8, url("RainTime")), (9..15, url("PETPET")), (17..21, url("smile"))]
        );
    }
}
