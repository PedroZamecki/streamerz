//! The chat window's side of the engine: every platform's chat (through `Engine::chat`) kept in one
//! bounded log, the emotes seen per platform, and sending to one platform or all of them with each
//! platform's emote format.

use super::Engine;
use crate::commands::Chat;
use std::collections::VecDeque;
use std::rc::Rc;

pub const PLATFORMS: [&str; 4] = ["twitch", "youtube", "kick", "tiktok"];

/// Lines kept (all platforms together).
const KEEP: usize = 500;
/// Emotes remembered per platform (the most recent).
const EMOTES: usize = 300;

/// One chat line, as the window shows it: emotes by their display name.
#[derive(Clone, Default)]
pub struct Line {
    pub seq: u64,
    pub platform: String,
    pub time: String,
    pub user: String,
    pub user_id: String,
    pub text: String,
    /// The emote names in `text`.
    pub emotes: Vec<String>,
    /// Not a chat message: a send that failed, or (dev mode) one kept instead of sent.
    pub note: bool,
}

#[derive(Default)]
pub struct Log {
    lines: VecDeque<Line>,
    seq: u64,
    /// Per platform, each emote's raw token (Twitch "Kappa", YouTube ":yt:", Kick "[emote:1:KEKW]").
    emotes: [VecDeque<String>; 4],
}

fn index(platform: &str) -> Option<usize> {
    PLATFORMS.iter().position(|p| *p == platform)
}

/// What an emote token shows as: Kick's "[emote:37226:KEKW]" is "KEKW"; the others as they are.
pub fn shown(token: &str) -> &str {
    token
        .strip_prefix("[emote:")
        .and_then(|t| t.strip_suffix(']'))
        .and_then(|t| t.split_once(':'))
        .map_or(token, |x| x.1)
}

/// `text` written for `platform`: Kick emotes as Kick's tags (from the names in `known`, Kick's
/// emotes seen), Kick tags pasted for another platform as their names. Unicode emoji go as they are.
pub fn for_platform(text: &str, platform: &str, kick_known: &[String]) -> String {
    text.split(' ')
        .map(|w| {
            if platform == "kick" {
                kick_known
                    .iter()
                    .find(|k| shown(k) == w && *k != w)
                    .map_or(w.to_string(), Clone::clone)
            } else {
                shown(w).to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

impl Log {
    fn push(&mut self, mut l: Line) {
        self.seq += 1;
        l.seq = self.seq;
        if self.lines.len() == KEEP {
            self.lines.pop_front();
        }
        self.lines.push_back(l);
    }
}

impl Engine {
    /// Keep a chat message for the window (called by `chat`).
    pub(super) fn log_chat(&self, c: &Chat) {
        let mut st = self.st.borrow_mut();
        let log = &mut st.chat_log;
        let Some(i) = index(&c.platform) else { return };
        let mut text = c.text.clone();
        for e in &c.emotes {
            if c.platform != "twitch" {
                // Spaced, so "[emote:1:A][emote:1:A]" or ":a::b:" shows as separate emotes.
                text = text.replace(e.as_str(), &format!(" {} ", shown(e)));
            }
            let seen = &mut log.emotes[i];
            if let Some(at) = seen.iter().position(|x| x == e) {
                seen.remove(at);
            } else if seen.len() == EMOTES {
                seen.pop_front();
            }
            seen.push_back(e.clone());
        }
        if c.platform != "twitch" && !c.emotes.is_empty() {
            text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        }
        log.push(Line {
            platform: c.platform.clone(),
            time: crate::fmt::date(&crate::fmt::now(), "HH:mm"),
            user: c.user.clone(),
            user_id: c.user_id.clone(),
            text,
            emotes: c.emotes.iter().map(|e| shown(e).to_string()).collect(),
            ..Default::default()
        });
        drop(st);
        crate::ui::chat();
    }

    /// A line from streamerz itself (a failed send, a dev-mode send).
    fn chat_note(&self, platform: &str, text: String) {
        self.st.borrow_mut().chat_log.push(Line {
            platform: platform.into(),
            time: crate::fmt::date(&crate::fmt::now(), "HH:mm"),
            user: "streamerz".into(),
            text,
            note: true,
            ..Default::default()
        });
        crate::ui::chat();
    }

    /// The lines after `seq`.
    pub fn chat_since(&self, seq: u64) -> Vec<Line> {
        let st = self.st.borrow();
        st.chat_log.lines.iter().filter(|l| l.seq > seq).cloned().collect()
    }

    /// The people who chatted on `platform` this session (newest first): name and id.
    pub fn chatters(&self, platform: &str) -> Vec<(String, String)> {
        let st = self.st.borrow();
        let mut seen = Vec::new();
        for l in st
            .chat_log
            .lines
            .iter()
            .rev()
            .filter(|l| l.platform == platform && !l.note)
        {
            if !seen.iter().any(|(_, id): &(String, String)| *id == l.user_id) {
                seen.push((l.user.clone(), l.user_id.clone()));
            }
        }
        seen
    }

    /// The emotes seen on `platform`, most recent first, by display name.
    pub fn chat_emotes(&self, platform: &str) -> Vec<String> {
        let st = self.st.borrow();
        let i = index(platform).unwrap_or(0);
        let mut v: Vec<String> = st.chat_log.emotes[i]
            .iter()
            .rev()
            .map(|e| shown(e).to_string())
            .collect();
        v.dedup();
        v
    }

    /// Whether a platform is reading chat.
    pub fn chat_connected(&self, platform: &str) -> bool {
        match platform {
            "twitch" => self.twitch_connected(),
            "youtube" => self.youtube_connected(),
            "kick" => self.kick_connected(),
            "tiktok" => self.tiktok_connected(),
            _ => false,
        }
    }

    /// Send `text` to `platform`, or to every connected one ("all"), as the bot (falling back to the
    /// broadcaster) or the broadcaster. Failures show up in the chat log. Where it went.
    pub fn send_chat(self: &Rc<Self>, platform: &str, text: &str, bot: bool) -> Vec<&'static str> {
        let to: Vec<&'static str> = PLATFORMS
            .into_iter()
            .filter(|p| {
                if platform == "all" {
                    self.chat_connected(p)
                } else {
                    *p == platform
                }
            })
            .collect();
        if to.is_empty() {
            self.chat_note(platform, "nothing connected to send to".into());
        }
        let kick_known: Vec<String> = self.st.borrow().chat_log.emotes[2].iter().cloned().collect();
        for &p in &to {
            let text = for_platform(text, p, &kick_known);
            if self.dev_out(p, &text, bot) {
                self.chat_note(p, format!("(dev, not sent) {text}"));
                continue;
            }
            let e = self.clone();
            tokio::task::spawn_local(async move {
                let r = match p {
                    "twitch" => match e.twitch() {
                        Some(t) => t.send_message(text, None, bot, true).await,
                        None => Err("Twitch isn't connected".into()),
                    },
                    "youtube" => e.youtube_start().send_message("", &text, bot, true).await,
                    "kick" => e.kick_start().send_message(&text, bot, true).await,
                    _ => e.tiktok_start().send_message(&text, bot, true).await,
                };
                if let Err(err) = r {
                    e.chat_note(p, format!("not sent: {err}"));
                }
            });
        }
        to
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emotes_per_platform() {
        let known = vec!["[emote:37226:KEKW]".to_string()];
        assert_eq!(shown("[emote:37226:KEKW]"), "KEKW");
        assert_eq!(shown(":yt:"), ":yt:");
        assert_eq!(for_platform("hi KEKW 😀", "kick", &known), "hi [emote:37226:KEKW] 😀");
        assert_eq!(for_platform("hi [emote:37226:KEKW] 😀", "twitch", &known), "hi KEKW 😀");
        assert_eq!(for_platform("hi KEKW", "youtube", &known), "hi KEKW");
        assert_eq!(for_platform("[emote:37226:KEKW]", "kick", &known), "[emote:37226:KEKW]");
        assert_eq!(
            crate::kick::kick_emotes("a [emote:1:A] b [emote:x:B] [emote:22:C]"),
            ["[emote:1:A]", "[emote:22:C]"]
        );
    }
}
