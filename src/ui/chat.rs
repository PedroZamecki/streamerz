//! The Chat tab:
//! - Every set-up platform's chat together, side by side, or individually
//! - Individual per-pane input bars: `[Account Avatar] [Textbox] [Emotes Icon] [Send Icon]`
//! - Account selector popup on avatar click (switches Broadcaster vs Bot per platform)
//! - Discrete `"Message all"` button & popup in the top bar
//! - Individual 7TV-style emote & emoji picker per platform and account, with search, provider tabs,
//!   collapsible sections, and right-click `★` favorites
//! - Platform-authentic badges & username colors, plus the platform's recent history (memory only) before a divider

use super::accounts::{platform_accounts, room, unlock_logins_interactive};
use super::emotes::{self, EmoteSection, Provider};
use super::fit::Ellipsis;
use super::img::{self, Pic};
use super::tip::{self, Tip};
use super::{button, icons, query, tell, ROW};
use crate::engine::chatlog::{ChatState, Line, Moderate, Pin, Poll, PLATFORMS};
use crate::i18n::{tr, trf};
use crate::Config;
use fltk::app;
use fltk::button::{Button, CheckButton};
use fltk::draw;
use fltk::enums::{Align, CallbackTrigger, Color, Damage, Event, Font, FrameType, Key};
use fltk::frame::Frame;
use fltk::group::{Flex, Wizard};
use fltk::image::SvgImage;
use fltk::input::Input;
use fltk::menu::Choice;
use fltk::prelude::*;
use fltk::valuator::Slider;
use fltk::window::DoubleWindow;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{mpsc::UnboundedSender, watch};

static VIEWERS_VISIBLE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
thread_local! {
    static DEV_VIEW: RefCell<Option<Choice>> = const { RefCell::new(None) };
}

/// Select a chat view for the development UI tool.
pub(super) fn dev_select_view(name: &str) -> bool {
    DEV_VIEW.with_borrow(|view| {
        let Some(mut choice) = view.clone() else { return false };
        let i = choice.find_index(name);
        if i < 0 {
            return false;
        }
        choice.set_value(i);
        choice.do_callback();
        true
    })
}
pub(crate) fn viewers_visible() -> bool {
    VIEWERS_VISIBLE.load(std::sync::atomic::Ordering::Relaxed)
}
pub(crate) fn set_viewers_visible(visible: bool) {
    VIEWERS_VISIBLE.store(visible, std::sync::atomic::Ordering::Relaxed);
}

type Cfg = Arc<watch::Sender<Config>>;
type Cmds = UnboundedSender<crate::engine::UiCmd>;

const NAMES: [&str; 4] = ["Twitch", "YouTube", "Kick", "TikTok"];

/// Compact a viewer count while retaining one decimal place where it helps.
fn compact_viewers(n: u64) -> String {
    for (unit, suffix) in [(1_000_000_000, "b"), (1_000_000, "m"), (1_000, "k")] {
        if n >= unit {
            let whole = n / unit;
            let tenth = (n % unit) * 10 / unit;
            return if whole < 100 && tenth != 0 {
                format!("{whole}.{tenth}{suffix}")
            } else {
                format!("{whole}{suffix}")
            };
        }
    }
    n.to_string()
}

fn viewer_counter() -> Frame {
    let mut frame = Frame::default();
    frame.set_label_type(fltk::enums::LabelType::None);
    frame.set_label_color(super::muted());
    frame.set_label_size(11);
    frame.draw(|f| {
        let text = f.label();
        draw::set_font(Font::Helvetica, 11);
        let full = text.trim_end_matches('+');
        let shown = if draw::width(&text) as i32 > (f.w() - 16).max(0) {
            full.parse::<u64>().map_or_else(
                |_| text.clone(),
                |n| format!("{}{}", compact_viewers(n), if text.ends_with('+') { "+" } else { "" }),
            )
        } else {
            text
        };
        let width = draw::width(&shown) as i32;
        let x = (f.x() + f.w() - width - 20).max(f.x());
        if let Some(mut icon) = icons::icon("users", f.label_color(), 12) {
            icon.draw(x, f.y() + (f.h() - 12) / 2, 12, 12);
        }
        draw::set_draw_color(f.label_color());
        draw::draw_text2(
            &shown,
            x + 16,
            f.y(),
            (f.x() + f.w() - x - 16).max(0),
            f.h(),
            Align::Left | Align::Inside | Align::Clip,
        );
    });
    frame
}
/// Text size, emote height, and a line's least height.
const TEXT: i32 = 14;
const EMOTE: i32 = 28;
/// A gigantified emote.
const HUGE: i32 = 72;
const LINE: i32 = 22;
/// Lines kept per view (the engine keeps as many).
/// Badge height, and the gap after each badge.
const BADGE: i32 = 18;
const GAP: i32 = 3;

/// Platform-specific vector badges (each platform has its own visual language; never cross-applied)
/// plus crisp UI icons for buttons and provider tabs.
const ICONS: &[(&str, &str)] = &[
    // Kick's own badges, from kick.com's chat.
    (
        "kick:broadcaster",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="none"><path fill="url(#HostBadge__a)" d="M10 13.46q1.43 0 2.44-1.02A3.3 3.3 0 0 0 13.46 10V4.46q0-1.43-1.02-2.44A3.3 3.3 0 0 0 10 1Q8.57 1 7.56 2.02a3.3 3.3 0 0 0-1.02 2.44V10q0 1.43 1.02 2.44 1.01 1.02 2.44 1.02"/><path fill="#fff" fill-opacity=".3" d="M10 13.46q1.43 0 2.44-1.02A3.3 3.3 0 0 0 13.46 10V4.46q0-1.43-1.02-2.44A3.3 3.3 0 0 0 10 1Q8.57 1 7.56 2.02a3.3 3.3 0 0 0-1.02 2.44V10q0 1.43 1.02 2.44 1.01 1.02 2.44 1.02"/><path fill="url(#HostBadge__b)" d="M16.02 8.13a.7.7 0 0 0-.48-.2.7.7 0 0 0-.49.2q-.2.2-.2.49V10q0 2-1.43 3.42A4.7 4.7 0 0 1 10 14.85a4.7 4.7 0 0 1-3.42-1.43A4.7 4.7 0 0 1 5.15 10V8.62a.7.7 0 0 0-.2-.5.7.7 0 0 0-.49-.2.7.7 0 0 0-.49.2q-.2.21-.2.5V10a6 6 0 0 0 1.6 4.16 6 6 0 0 0 3.94 2.03v1.43H6.54a.7.7 0 0 0-.49.2q-.2.2-.2.49t.2.48q.2.21.49.21h6.92q.28 0 .49-.2t.2-.5a.7.7 0 0 0-.2-.48.7.7 0 0 0-.49-.2H10.7v-1.43a6 6 0 0 0 3.95-2.03 6 6 0 0 0 1.6-4.16V8.62a.7.7 0 0 0-.21-.5"/><defs><linearGradient id="HostBadge__a" x1="-5.87" x2="19.51" y1="1" y2="32.63" gradientUnits="userSpaceOnUse"><stop stop-color="#ff1cd2"/><stop offset="1" stop-color="#b20dff"/></linearGradient><linearGradient id="HostBadge__b" x1="1.18" x2="3.84" y1="1" y2="21.04" gradientUnits="userSpaceOnUse"><stop stop-color="#ff1cd2"/><stop offset="1" stop-color="#b20dff"/></linearGradient></defs></svg>"##,
    ),
    (
        "kick:moderator",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="none"><path fill="url(#ModeratorBadge__a)" fill-rule="evenodd" d="M17.88 1C18.5 1 19 1.5 19 2.13v15.75c0 .62-.5 1.12-1.12 1.12H2.13C1.5 19 1 18.5 1 17.88V2.13C1 1.5 1.5 1 2.13 1zm-7.76 1.68a.56.56 0 0 0-.8 0L5.7 6.31a.56.56 0 0 0 0 .8l.98.97c.22.21.57.21.8 0l.04-.05 1.32 1.33-5.83 5.83a1.25 1.25 0 0 0 1.77 1.77l5.83-5.84 1.33 1.33-.05.04a.56.56 0 0 0 0 .8l.97.97c.22.22.58.22.8 0l3.62-3.62a.56.56 0 0 0 0-.8l-.97-.97a.56.56 0 0 0-.8 0l-.04.04-1.32-1.32a1.25 1.25 0 0 0-1.77-1.77L11.05 4.5l.05-.05a.56.56 0 0 0 0-.8z" clip-rule="evenodd"/><path fill="url(#ModeratorBadge__b)" fill-rule="evenodd" d="M17.88 1C18.5 1 19 1.5 19 2.13v15.75c0 .62-.5 1.12-1.12 1.12H2.13C1.5 19 1 18.5 1 17.88V2.13C1 1.5 1.5 1 2.13 1zm-7.76 1.68a.56.56 0 0 0-.8 0L5.7 6.31a.56.56 0 0 0 0 .8l.98.97c.22.21.57.21.8 0l.04-.05 1.32 1.33-5.83 5.83a1.25 1.25 0 0 0 1.77 1.77l5.83-5.84 1.33 1.33-.05.04a.56.56 0 0 0 0 .8l.97.97c.22.22.58.22.8 0l3.62-3.62a.56.56 0 0 0 0-.8l-.97-.97a.56.56 0 0 0-.8 0l-.04.04-1.32-1.32a1.25 1.25 0 0 0-1.77-1.77L11.05 4.5l.05-.05a.56.56 0 0 0 0-.8z" clip-rule="evenodd"/><path fill="url(#ModeratorBadge__c)" fill-rule="evenodd" d="M17.88 1C18.5 1 19 1.5 19 2.13v15.75c0 .62-.5 1.12-1.12 1.12H2.13C1.5 19 1 18.5 1 17.88V2.13C1 1.5 1.5 1 2.13 1zm-7.76 1.68a.56.56 0 0 0-.8 0L5.7 6.31a.56.56 0 0 0 0 .8l.98.97c.22.21.57.21.8 0l.04-.05 1.32 1.33-5.83 5.83a1.25 1.25 0 0 0 1.77 1.77l5.83-5.84 1.33 1.33-.05.04a.56.56 0 0 0 0 .8l.97.97c.22.22.58.22.8 0l3.62-3.62a.56.56 0 0 0 0-.8l-.97-.97a.56.56 0 0 0-.8 0l-.04.04-1.32-1.32a1.25 1.25 0 0 0-1.77-1.77L11.05 4.5l.05-.05a.56.56 0 0 0 0-.8z" clip-rule="evenodd"/><defs><linearGradient id="ModeratorBadge__a" x1="11.58" x2="2.62" y1="-6.16" y2="23" gradientUnits="userSpaceOnUse"><stop stop-color="#ff6a4a"/><stop offset="1" stop-color="#c70c00"/></linearGradient><linearGradient id="ModeratorBadge__b" x1="9.86" x2="10.18" y1="-1.68" y2="22.98" gradientUnits="userSpaceOnUse"><stop stop-color="#ffc900"/><stop offset=".99" stop-color="#ff9500"/></linearGradient><linearGradient id="ModeratorBadge__c" x1="-7.41" x2="19" y1="27.41" y2="1" gradientUnits="userSpaceOnUse"><stop stop-color="#0095ff"/><stop offset=".99" stop-color="#00c7ff"/></linearGradient></defs></svg>"##,
    ),
    (
        "kick:vip",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="none"><path fill="url(#VIPBadge__a)" fill-rule="evenodd" d="M17.88 1C18.5 1 19 1.5 19 2.13v15.75c0 .62-.5 1.12-1.12 1.12H2.13C1.5 19 1 18.5 1 17.88V2.13C1 1.5 1.5 1 2.13 1zm-7.9 2.81a.6.6 0 0 0-.53.32L6.53 9.45 3 7.69a.6.6 0 0 0-.64.07.6.6 0 0 0-.22.62l1.44 6.75q.04.22.21.36a.6.6 0 0 0 .4.13h11.58a.6.6 0 0 0 .38-.13.6.6 0 0 0 .22-.36l1.49-6.75a.6.6 0 0 0-.22-.64.6.6 0 0 0-.65-.06l-3.56 1.77-2.92-5.32a1 1 0 0 0-.23-.23 1 1 0 0 0-.3-.09" clip-rule="evenodd"/><path fill="url(#VIPBadge__b)" fill-rule="evenodd" d="M17.88 1C18.5 1 19 1.5 19 2.13v15.75c0 .62-.5 1.12-1.12 1.12H2.13C1.5 19 1 18.5 1 17.88V2.13C1 1.5 1.5 1 2.13 1zm-7.9 2.81a.6.6 0 0 0-.53.32L6.53 9.45 3 7.69a.6.6 0 0 0-.64.07.6.6 0 0 0-.22.62l1.44 6.75q.04.22.21.36a.6.6 0 0 0 .4.13h11.58a.6.6 0 0 0 .38-.13.6.6 0 0 0 .22-.36l1.49-6.75a.6.6 0 0 0-.22-.64.6.6 0 0 0-.65-.06l-3.56 1.77-2.92-5.32a1 1 0 0 0-.23-.23 1 1 0 0 0-.3-.09" clip-rule="evenodd"/><defs><linearGradient id="VIPBadge__a" x1="11.58" x2="2.62" y1="-6.16" y2="23" gradientUnits="userSpaceOnUse"><stop stop-color="#ff6a4a"/><stop offset="1" stop-color="#c70c00"/></linearGradient><linearGradient id="VIPBadge__b" x1="9.86" x2="10.18" y1="-1.68" y2="22.98" gradientUnits="userSpaceOnUse"><stop stop-color="#ffc900"/><stop offset=".99" stop-color="#ff9500"/></linearGradient></defs></svg>"##,
    ),
    (
        "kick:og",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="none"><path fill="#fff" d="M13.84 10.71v6.04q0 .18.12.28.12.09.37.08h.46V19h-1.5q-1.59 0-2.26-.56a2 2 0 0 1-.65-1.47V10.5q0-.91.65-1.48.67-.55 2.25-.55h1.59v1.88h-.54q-.26 0-.37.1-.12.09-.12.26M19 13.18V15h-4.45v-1.82zm-3.46-1.1V10.7q0-.17-.12-.26-.12-.1-.38-.1h-.53V8.47h1.58q1.59 0 2.24.55.67.57.67 1.48v1.58zm0 4.67v-2.31H19v2.53q0 .9-.67 1.47-.65.56-2.24.56h-1.5v-1.89h.45q.27 0 .38-.08a.3.3 0 0 0 .12-.28"/><path fill="url(#OGBadge__a)" d="M13.84 10.71v6.04q0 .18.12.28.12.09.37.08h.46V19h-1.5q-1.59 0-2.26-.56a2 2 0 0 1-.65-1.47V10.5q0-.91.65-1.48.67-.55 2.25-.55h1.59v1.88h-.54q-.26 0-.37.1-.12.09-.12.26M19 13.18V15h-4.45v-1.82zm-3.46-1.1V10.7q0-.17-.12-.26-.12-.1-.38-.1h-.53V8.47h1.58q1.59 0 2.24.55.67.57.67 1.48v1.58zm0 4.67v-2.31H19v2.53q0 .9-.67 1.47-.65.56-2.24.56h-1.5v-1.89h.45q.27 0 .38-.08a.3.3 0 0 0 .12-.28"/><path fill="#00fff2" d="M13.84 3.25v6.04q0 .17.12.27.12.09.37.09h.46v1.88h-1.5q-1.59 0-2.26-.55a2 2 0 0 1-.65-1.48V3.03q0-.9.65-1.47Q11.7 1 13.28 1h1.59v1.89h-.54q-.26 0-.37.1-.12.08-.12.26M19 5.7v1.82h-4.45V5.71zm-3.46-1.1V3.25q0-.18-.12-.26-.12-.1-.38-.1h-.53V1h1.58q1.59 0 2.24.56.67.55.67 1.47v1.58zm0 4.68V6.97H19V9.5q0 .91-.67 1.48-.66.55-2.24.55h-1.5V9.65h.45q.26 0 .38-.09a.3.3 0 0 0 .12-.27"/><path fill="#fff" d="M6.28 5.4v-2q0-.18-.12-.26-.12-.1-.37-.1h-.66V1h1.7q1.59 0 2.24.56.67.55.67 1.47V5.4zm0 3.72V5.18h3.46V9.5q0 .91-.67 1.48-.65.55-2.24.55h-1.7V9.48h.66q.26 0 .37-.08a.3.3 0 0 0 .12-.28m-1.82-5.7V5.4H1V3.03q0-.9.65-1.47Q2.32 1 3.91 1h1.7v2.05h-.65q-.26 0-.38.1-.12.08-.12.26m0 1.76v3.94q0 .19.12.28t.38.08h.65v2.05H3.9q-1.58 0-2.26-.55A2 2 0 0 1 1 9.5V5.18z"/><path fill="url(#OGBadge__b)" d="M6.28 5.4v-2q0-.18-.12-.26-.12-.1-.37-.1h-.66V1h1.7q1.59 0 2.24.56.67.55.67 1.47V5.4zm0 3.72V5.18h3.46V9.5q0 .91-.67 1.48-.65.55-2.24.55h-1.7V9.48h.66q.26 0 .37-.08a.3.3 0 0 0 .12-.28m-1.82-5.7V5.4H1V3.03q0-.9.65-1.47Q2.32 1 3.91 1h1.7v2.05h-.65q-.26 0-.38.1-.12.08-.12.26m0 1.76v3.94q0 .19.12.28t.38.08h.65v2.05H3.9q-1.58 0-2.26-.55A2 2 0 0 1 1 9.5V5.18z"/><path fill="#00fff2" d="M6.28 12.86v-1.98q0-.18-.12-.26-.12-.1-.37-.1h-.66V8.47h1.7q1.59 0 2.24.55.67.57.67 1.48v2.36zm0 3.73v-3.94h3.46v4.32q0 .9-.67 1.47-.65.56-2.24.56h-1.7v-2.05h.66q.26 0 .37-.08a.3.3 0 0 0 .12-.28m-1.82-5.71v1.98H1V10.5q0-.91.65-1.48.67-.55 2.26-.55h1.7v2.05h-.65q-.26 0-.38.1-.12.08-.12.26m0 1.77v3.94q0 .18.12.28.12.08.38.08h.65V19H3.9q-1.58 0-2.26-.56A2 2 0 0 1 1 16.97v-4.32z"/><defs><linearGradient id="OGBadge__a" x1="9.56" x2="9.93" y1="-11.09" y2="19" gradientUnits="userSpaceOnUse"><stop stop-color="#00fff2"/><stop offset="1" stop-color="#006399"/></linearGradient><linearGradient id="OGBadge__b" x1="10" x2="10.11" y1="1" y2="32.3" gradientUnits="userSpaceOnUse"><stop stop-color="#00fff2"/><stop offset="1" stop-color="#006399"/></linearGradient></defs></svg>"##,
    ),
    (
        "kick:founder",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="none"><path fill="url(#FounderBadge__a)" d="M10 19a9 9 0 1 0 0-18 9 9 0 0 0 0 18"/><path fill="url(#FounderBadge__b)" d="M10 19a9 9 0 1 0 0-18 9 9 0 0 0 0 18"/><path fill="#feb635" d="M10 17.33a7.33 7.33 0 1 0 0-14.66 7.33 7.33 0 0 0 0 14.66"/><path fill="url(#FounderBadge__c)" d="M10 17.33a7.33 7.33 0 1 0 0-14.66 7.33 7.33 0 0 0 0 14.66"/><path fill="#000" fill-opacity=".05" d="M17.33 10a7.34 7.34 0 0 1-10.95 6.37 7.34 7.34 0 0 0 6.9-12.93A7.3 7.3 0 0 1 17.32 10M10.5 3.01c1.44 0 2.8.42 3.92 1.14A7.33 7.33 0 1 0 6.1 16.21 7.32 7.32 0 0 1 10.5 3"/><path fill="#000" fill-opacity=".8" d="M11.46 6.32v8.18H9.24V8.36h-.05L7.4 9.43V7.55l2.02-1.23z"/><defs><linearGradient id="FounderBadge__a" x1="9.86" x2="10.14" y1="-1.51" y2="21.64" gradientUnits="userSpaceOnUse"><stop stop-color="#ffc900"/><stop offset=".99" stop-color="#ff9500"/></linearGradient><linearGradient id="FounderBadge__b" x1="10" x2="10" y1="1" y2="19" gradientUnits="userSpaceOnUse"><stop stop-color="#fff" stop-opacity=".3"/><stop offset="1" stop-color="#fff" stop-opacity=".15"/></linearGradient><linearGradient id="FounderBadge__c" x1="9.86" x2="10.14" y1="-1.51" y2="21.64" gradientUnits="userSpaceOnUse"><stop stop-color="#ffc900"/><stop offset=".99" stop-color="#ff9500"/></linearGradient></defs></svg>"##,
    ),
    (
        "kick:verified",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="none"><path fill="url(#VerifiedBadge__a)" fill-rule="evenodd" d="M9.15 1.43c.48-.57 1.18-.57 1.64 0l1.26 1.58c.1.1.18.14.33.1q.97-.29 1.96-.54c.7-.18 1.27.23 1.3.95q.09 1 .11 2.03 0 .22.19.27l1.86.7c.74.27.97.91.54 1.58l-1.1 1.67c-.08.13-.08.2 0 .33l1.12 1.72c.4.6.18 1.26-.48 1.6q-.94.38-1.88.72c-.12.04-.2.1-.2.27l-.11 2.07c-.02.7-.6 1.11-1.28.93l-1.99-.54c-.14-.04-.2 0-.29.1l-1.28 1.6c-.46.57-1.14.57-1.62 0l-1.28-1.6q-.1-.17-.29-.1c-.64.18-1.28.33-1.92.54-.8.25-1.39-.35-1.39-1.02 0-.66-.08-1.3-.1-1.96q.02-.22-.19-.27l-1.94-.75a.98.98 0 0 1-.5-1.47q.55-.89 1.14-1.75c.08-.15.08-.23 0-.36L1.64 8.12c-.4-.6-.18-1.24.48-1.51l1.92-.75c.13-.06.19-.12.19-.27q.03-1.04.1-2.07c.04-.74.58-1.1 1.47-.89l1.74.48q.23.08.37-.12zm5.36 5a1.4 1.4 0 0 0-1.98.09l-4.17 4.54-1.3-1.3a1.4 1.4 0 0 0-2.09 1.88l.1.1L7.4 14.1a1.4 1.4 0 0 0 2.03-.05l5.16-5.62a1.4 1.4 0 0 0-.09-1.99" clip-rule="evenodd"/><defs><linearGradient id="VerifiedBadge__a" x1="5.58" x2="14.78" y1="19.2" y2="1.51" gradientUnits="userSpaceOnUse"><stop stop-color="#1eff00"/><stop offset=".99" stop-color="#00ff8c"/></linearGradient></defs></svg>"##,
    ),
    (
        "kick:staff",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="none"><path fill="url(#StaffBadge__a)" fill-rule="evenodd" d="M17.88 1C18.5 1 19 1.5 19 2.13v15.75c0 .62-.5 1.12-1.12 1.12H2.13C1.5 19 1 18.5 1 17.88V2.13C1 1.5 1.5 1 2.13 1zM4.38 3.67v12.66H8.6v-2.81H10v1.4h1.4v1.4h4.22v-4.21h-1.4v-1.4h-1.41V9.3h1.41V7.9h1.4V3.66H11.4v1.4H10v1.41H8.6v-2.8z" clip-rule="evenodd"/><defs><linearGradient id="StaffBadge__a" x1="6.06" x2="12.3" y1="20.24" y2="-.28" gradientUnits="userSpaceOnUse"><stop stop-color="#1eff00"/><stop offset=".99" stop-color="#00ff8c"/></linearGradient></defs></svg>"##,
    ),
    (
        "kick:sub_gifter",
        r##"<svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 32 32"><g clip-path="url(#SubGifterBadgeA)"><path fill="#FF5008" d="M16 13.3 1 6.7v6l1.3.6v12.2L16 32V13.2Z"></path><path fill="#FF5008" d="M16 21.5v-2.3l-13.7-6v2.2l13.7 6Z"></path><path fill="#FFC800" d="M31 6.7v6l-1.3.6v12.2L16 32V13.4l-6.4-2.8-3-1.4L1 6.7 7.3 4l2.9-1.3L16 0l5.8 2.6 2.8 1.3L31 6.7Z"></path><path fill="#FF5008" d="M29.7 13.3v2.1l-13.7 6v-2.2l13.7-6Z"></path></g><defs ><clipPath id="SubGifterBadgeA"><path fill="#fff" d="M0 0h32v32H0z"></path></clipPath></defs></svg>"##,
    ),
    (
        "kick:subscriber",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="none"><path fill="#000" d="m10.6 2.4-.46-1.3a.15.15 0 0 0-.28 0L9.4 2.4c-1.2 3.4-3.6 5.8-7 7l-1.3.46a.15.15 0 0 0 0 .28l1.3.46c3.4 1.2 5.8 3.6 7 7l.46 1.3c.05.13.23.13.28 0l.46-1.3c1.2-3.4 3.6-5.8 7-7l1.3-.46a.15.15 0 0 0 0-.28l-1.3-.46c-3.4-1.2-5.8-3.6-7-7"/><path fill="url(#SubscriberBadge__a)" d="m10.6 2.4-.46-1.3a.15.15 0 0 0-.28 0L9.4 2.4c-1.2 3.4-3.6 5.8-7 7l-1.3.46a.15.15 0 0 0 0 .28l1.3.46c3.4 1.2 5.8 3.6 7 7l.46 1.3c.05.13.23.13.28 0l.46-1.3c1.2-3.4 3.6-5.8 7-7l1.3-.46a.15.15 0 0 0 0-.28l-1.3-.46c-3.4-1.2-5.8-3.6-7-7"/><defs><radialGradient id="SubscriberBadge__a" cx="0" cy="0" r="1" gradientTransform="rotate(90 0 10)scale(9.29102)" gradientUnits="userSpaceOnUse"><stop stop-color="#e1ff00"/><stop offset="1" stop-color="#2aa300"/></radialGradient></defs></svg>"##,
    ),
    (
        "kick:sidekick",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="none"><path fill="#000" d="M13.24 6.68c-1.38.37-3 1.76-3 1.76a.4.4 0 0 1-.48 0s-1.62-1.4-3-1.76c-2.71-.73-5.45-.17-5.45-.17a.4.4 0 0 0-.31.38s.16 5.49 3.93 6.64c2 .62 4.78-.7 4.78-.7a.8.8 0 0 1 .58 0s2.78 1.32 4.78.7C18.84 12.38 19 6.9 19 6.9a.4.4 0 0 0-.3-.38s-2.75-.56-5.46.17m-8.1 4.58C3.5 10.54 2.93 9 2.93 9a.47.47 0 0 1 .27-.6s1.52-.61 3.15.11a4.4 4.4 0 0 1 2.2 2.28c.07.23-.04.5-.27.59 0 0-1.52.62-3.16-.1m9.65.04c-1.64.73-3.16.1-3.16.1a.47.47 0 0 1-.26-.59s.55-1.54 2.19-2.27c1.63-.73 3.15-.1 3.15-.1.23.09.35.36.26.59 0 0-.55 1.54-2.18 2.27"/><path fill="url(#SidekickBadge__a)" d="M13.24 6.68c-1.38.37-3 1.76-3 1.76a.4.4 0 0 1-.48 0s-1.62-1.4-3-1.76c-2.71-.73-5.45-.17-5.45-.17a.4.4 0 0 0-.31.38s.16 5.49 3.93 6.64c2 .62 4.78-.7 4.78-.7a.8.8 0 0 1 .58 0s2.78 1.32 4.78.7C18.84 12.38 19 6.9 19 6.9a.4.4 0 0 0-.3-.38s-2.75-.56-5.46.17m-8.1 4.58C3.5 10.54 2.93 9 2.93 9a.47.47 0 0 1 .27-.6s1.52-.61 3.15.11a4.4 4.4 0 0 1 2.2 2.28c.07.23-.04.5-.27.59 0 0-1.52.62-3.16-.1m9.65.04c-1.64.73-3.16.1-3.16.1a.47.47 0 0 1-.26-.59s.55-1.54 2.19-2.27c1.63-.73 3.15-.1 3.15-.1.23.09.35.36.26.59 0 0-.55 1.54-2.18 2.27"/><defs><linearGradient id="SidekickBadge__a" x1="11.58" x2="9.95" y1="3.36" y2="16.26" gradientUnits="userSpaceOnUse"><stop stop-color="#ff6a4a"/><stop offset="1" stop-color="#c70c00"/></linearGradient></defs></svg>"##,
    ),
    // YouTube Live Chat official-style badges (#FFD600 owner, #5E84F1 mod wrench, #2BA640 member, gray check)
    (
        "youtube:owner",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 18 18"><circle cx="9" cy="9" r="8" fill="#FFD600"/><path d="M4.5 12.5h9l-.8-5-2.4 2L9 6l-1.3 3.5-2.4-2z" fill="#0F0F0F"/></svg>"##,
    ),
    (
        "youtube:moderator",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 18 18"><circle cx="9" cy="9" r="8" fill="#5E84F1"/><path d="M11.8 4.8a2.8 2.8 0 0 0-3.3 3.9l-3.7 3.7 1.4 1.4 3.7-3.7a2.8 2.8 0 0 0 3.9-3.3l-1.6 1.6-1.4-.6-.6-1.4z" fill="#fff"/></svg>"##,
    ),
    (
        "youtube:verified",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 18 18"><circle cx="9" cy="9" r="7.5" fill="#AAAAAA"/><path d="m5.6 9.2 2.2 2.2 4.6-4.6" stroke="#0F0F0F" stroke-width="2" fill="none" stroke-linecap="round" stroke-linejoin="round"/></svg>"##,
    ),
    (
        "youtube:member",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 18 18"><circle cx="9" cy="9" r="8" fill="#2BA640"/><path d="m9 4 1.5 3.1 3.4.5-2.5 2.4.6 3.4L9 11.8l-3 1.6.6-3.4-2.5-2.4 3.4-.5z" fill="#fff"/></svg>"##,
    ),
    // TikTok Live dark-mode role badges (#FE2C55 rose-red, #25F4EE cyan, #FF9F2E gold)
    (
        "tiktok:host",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 18 18"><rect width="18" height="18" rx="4" fill="#FE2C55"/><circle cx="9" cy="9" r="3.2" fill="#fff"/><path d="M4.2 5.5a6.5 6.5 0 0 0 0 7M13.8 5.5a6.5 6.5 0 0 1 0 7" stroke="#fff" stroke-width="1.5" fill="none" stroke-linecap="round"/></svg>"##,
    ),
    (
        "tiktok:moderator",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 18 18"><rect width="18" height="18" rx="4" fill="#16282C" stroke="#25F4EE" stroke-width="1.4"/><path d="M9 3.5 13.8 5.5v3.5c0 2.9-2 4.9-4.8 5.8-2.8-.9-4.8-2.9-4.8-5.8V5.5z" fill="#25F4EE"/></svg>"##,
    ),
    (
        "tiktok:subscriber",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 18 18"><rect width="18" height="18" rx="4" fill="#FF9F2E"/><path d="m9 3.6 1.6 3.3 3.6.5-2.6 2.5.6 3.6L9 11.8l-3.2 1.7.6-3.6-2.6-2.5 3.6-.5z" fill="#161823"/></svg>"##,
    ),
    (
        "tiktok:friend",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 18 18"><rect width="18" height="18" rx="4" fill="#2E1C2B"/><path d="M9 14.2 4.4 9.7a2.7 2.7 0 0 1 3.8-3.8L9 6.7l.8-.8a2.7 2.7 0 0 1 3.8 3.8z" fill="#FE2C55"/></svg>"##,
    ),
    // UI Icons: User avatar fallback, Smiley/Emotes, Send plane, Broadcast/Message-All, Star, Search, 7TV, BTTV, FFZ
    (
        "ui:7tv",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 20"><path d="M2 4h9.5L6.5 16H3.2l3.8-9H2z" fill="#29B6F6"/><path d="M12.5 7h3l1.7 5.2L19 7h3l-3.4 9h-2.8z" fill="#E8EAED"/></svg>"##,
    ),
    (
        "ui:bttv",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20"><circle cx="10" cy="10" r="8" fill="#D5001C"/><circle cx="7.3" cy="8.5" r="1.4" fill="#fff"/><circle cx="12.7" cy="8.5" r="1.4" fill="#fff"/><path d="M6.8 13c1.1 1.2 5.3 1.2 6.4 0" stroke="#fff" stroke-width="1.7" fill="none" stroke-linecap="round"/></svg>"##,
    ),
    (
        "ui:ffz",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20"><rect x="2" y="4" width="16" height="12" rx="4" fill="#7C5CBF"/><circle cx="7.2" cy="10" r="1.8" fill="#fff"/><circle cx="12.8" cy="10" r="1.8" fill="#fff"/><path d="M8.5 13.2h3" stroke="#fff" stroke-width="1.5" stroke-linecap="round"/></svg>"##,
    ),
];

/// A badge, platform logo, or UI icon, scaled to `size` px and cached per thread.
pub(super) fn icon_sized(key: &str, size: i32) -> Option<SvgImage> {
    thread_local!(static CACHE: RefCell<HashMap<(String, i32), Option<SvgImage>>> = RefCell::default());
    CACHE.with_borrow_mut(|m| {
        m.entry((key.to_string(), size))
            .or_insert_with(|| match ICONS.iter().find(|b| b.0 == key) {
                Some((_, raw)) => SvgImage::from_data(raw).ok().map(|mut s| {
                    s.scale(size, size, true, true);
                    s
                }),
                None => img::logo(key, size),
            })
            .clone()
    })
}

fn svg(key: &str) -> Option<SvgImage> {
    icon_sized(key, BADGE)
}

/// Draws a badge `size` px tall at `(x, y)`: its picture, else its platform's drawing. Its width (0
/// when there's nothing to draw yet).
pub(super) fn draw_badge(cmds: &Cmds, platform: &str, key: &str, url: &str, x: i32, y: i32, size: i32) -> i32 {
    if !url.is_empty() {
        if let Some(p) = img::get(cmds, url, size, false) {
            let mut i = p.first();
            i.draw(x, y + (size - i.h()) / 2, i.w(), i.h());
            return i.w();
        }
    }
    match badge_fallback(platform, key, size) {
        Some(mut s) => {
            let (w, h) = (s.w(), s.h());
            s.draw(x, y + (size - h) / 2, w, h);
            w
        }
        None => 0,
    }
}

/// Map a badge key on `platform` to its platform-specific vector SVG fallback when no image URL is set.
fn badge_fallback_svg(platform: &str, raw_key: &str) -> Option<SvgImage> {
    badge_fallback(platform, raw_key, BADGE)
}

fn badge_fallback(platform: &str, raw_key: &str, size: i32) -> Option<SvgImage> {
    let svg = |k: &str| icon_sized(k, size);
    if raw_key.contains(':') {
        if let Some(s) = svg(raw_key) {
            return Some(s);
        }
    }
    let base = raw_key.split('/').next().unwrap_or(raw_key).to_lowercase();
    let mapped = match (platform, base.as_str()) {
        ("kick", "broadcaster") => "kick:broadcaster",
        ("kick", "moderator") => "kick:moderator",
        ("kick", "vip") => "kick:vip",
        ("kick", "og") => "kick:og",
        ("kick", "founder") => "kick:founder",
        ("kick", "verified") => "kick:verified",
        ("kick", "staff") => "kick:staff",
        ("kick", k) if k.starts_with("sub_gifter") => "kick:sub_gifter",
        ("kick", "sidekick") => "kick:sidekick",
        ("kick", "subscriber") => "kick:subscriber",
        ("youtube", "owner" | "broadcaster") => "youtube:owner",
        ("youtube", "moderator") => "youtube:moderator",
        ("youtube", "verified") => "youtube:verified",
        ("youtube", "member" | "subscriber") => "youtube:member",
        ("tiktok", "host" | "broadcaster") => "tiktok:host",
        ("tiktok", "moderator") => "tiktok:moderator",
        ("tiktok", "subscriber") => "tiktok:subscriber",
        ("tiktok", "friend") => "tiktok:friend",
        _ => return None,
    };
    svg(mapped)
}

/// Milliseconds since start, for animations.
fn now_ms() -> u64 {
    thread_local!(static START: Instant = Instant::now());
    START.with(|s| s.elapsed().as_millis() as u64)
}

/// Milliseconds since the epoch, as pins' ends are.
fn epoch_ms() -> u64 {
    crate::fmt::unix_now().as_millis() as u64
}

/// The name's colour: comes directly from the platform (adjusted only when too dark to read on dark
/// backgrounds), or falls back to each platform's own default palette if an older cached line lacked one.
fn name_color(l: &Line) -> Color {
    let hex = l.color.trim_start_matches('#');
    let (r, g, b) = match u32::from_str_radix(hex, 16) {
        Ok(v) if hex.len() == 6 => ((v >> 16) as u8, (v >> 8) as u8, v as u8),
        _ => match l.platform.as_str() {
            "youtube" => {
                if l.broadcaster {
                    (0xff, 0xd6, 0x00)
                } else if l.moderator {
                    (0x5e, 0x84, 0xf1)
                } else if l.subscriber {
                    (0x2b, 0xa6, 0x40)
                } else {
                    (0xaa, 0xaa, 0xaa)
                }
            }
            "tiktok" => {
                if l.broadcaster {
                    (0xfe, 0x2c, 0x55)
                } else if l.moderator {
                    (0x25, 0xf4, 0xee)
                } else if l.subscriber {
                    (0xff, 0x9f, 0x2e)
                } else {
                    (0x8e, 0xc5, 0xff)
                }
            }
            "kick" => {
                // kick.com's name colours.
                const KICK_PALETTE: [(u8, u8, u8); 16] = [
                    (0x00, 0xf1, 0xff),
                    (0x4c, 0xff, 0x75),
                    (0x55, 0xff, 0xc7),
                    (0x6f, 0x87, 0xff),
                    (0xaa, 0xa9, 0xff),
                    (0xbd, 0xff, 0x28),
                    (0xe2, 0x6e, 0xff),
                    (0xe4, 0xd8, 0x8f),
                    (0xe5, 0xff, 0xab),
                    (0xfe, 0xa0, 0xcf),
                    (0xff, 0x2c, 0x56),
                    (0xff, 0x41, 0x17),
                    (0xff, 0x55, 0xb3),
                    (0xff, 0xa6, 0x00),
                    (0xff, 0xae, 0x76),
                    (0xff, 0xff, 0xff),
                ];
                let h = l
                    .user
                    .bytes()
                    .fold(0u32, |h, b| h.wrapping_mul(31).wrapping_add(u32::from(b)));
                KICK_PALETTE[h as usize % KICK_PALETTE.len()]
            }
            _ => {
                let d = crate::twitch::default_user_color(if l.user.is_empty() { &l.user_id } else { &l.user });
                let u = u32::from_str_radix(d.trim_start_matches('#'), 16).unwrap_or(0x9acd32);
                ((u >> 16) as u8, (u >> 8) as u8, u as u8)
            }
        },
    };
    let lum = 0.299 * f32::from(r) + 0.587 * f32::from(g) + 0.114 * f32::from(b);
    if lum < 75.0 {
        Color::from_rgb(r / 2 + 115, g / 2 + 115, b / 2 + 115)
    } else {
        Color::from_rgb(r, g, b)
    }
}

/// Pinned, and not run out yet.
fn pinned(l: &Line) -> bool {
    l.pinned.is_some_and(|until| until > epoch_ms())
}

/// One thing on a line.
enum It {
    Text(String, Font, Color),
    /// Text struck through: a deleted message's.
    Struck(String),
    /// A picture, with what pointing at it says (emotes, badges).
    Pic(Pic, Option<Rc<Tip>>),
    Svg(SvgImage),
    /// A drawn badge, with what pointing at it says.
    SvgTip(SvgImage, Rc<Tip>),
    /// A word's space: where the line may wrap.
    Space,
    /// A few pixels between pictures (no wrap).
    Gap(i32),
    /// What follows starts a new row (after a reply's header).
    Break,
    /// A line across the chat with words in its middle: where history ends and live messages begin.
    Rule(String),
    /// An `@name` that tags one of our accounts: on a chip, so it stands out.
    Mention(String),
}

impl It {
    fn size(&self) -> (i32, i32) {
        match self {
            It::Text(t, f, _) => {
                draw::set_font(*f, TEXT);
                (draw::width(t) as i32, TEXT + 4)
            }
            It::Struck(t) => {
                draw::set_font(Font::Helvetica, TEXT);
                (draw::width(t) as i32, TEXT + 4)
            }
            It::Pic(p, _) => (p.w(), p.h().max(1)),
            It::Svg(s) | It::SvgTip(s, _) => (s.w(), s.h()),
            It::Space => {
                draw::set_font(Font::Helvetica, TEXT);
                (draw::width(" ") as i32, 0)
            }
            It::Gap(w) => (*w, 0),
            It::Break => (0, 0),
            It::Rule(_) => (0, TEXT + 12),
            It::Mention(t) => {
                draw::set_font(Font::Helvetica, TEXT);
                (draw::width(t) as i32 + 2 * MENTION_PAD, TEXT + 4)
            }
        }
    }
}

/// The room left and right of a mention's words on its chip.
const MENTION_PAD: i32 = 5;

thread_local! {
    /// The names (lowercase) of the accounts logged in, on any platform: what an `@name` is checked against.
    static ME: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    /// What lines show: (the time, the badges, emotes moving, youtube @handle). Chat's options; all panes follow.
    static SHOW: Cell<(bool, bool, bool, bool)> = const { Cell::new((true, true, true, true)) };
    /// How many lines the panes keep, for how long, and how they fade (the config's).
    static LIMITS: Cell<crate::ChatLimits> = const {
        Cell::new(crate::ChatLimits {
            save_history: false,
            per_platform: 400,
            total: 1000,
            lifetime: 0,
            fade_in: 0,
            fade_out: 400,
        })
    };
}

fn emote_tip(platform: &str, name: &str, url: &str) -> Rc<Tip> {
    Rc::new(Tip::Emote {
        platform: platform.into(),
        name: name.into(),
        url: url.into(),
    })
}

fn badge_tip(platform: &str, key: &str, url: &str) -> Rc<Tip> {
    Rc::new(Tip::Badge {
        platform: platform.into(),
        key: key.into(),
        url: url.into(),
    })
}

fn display_user(l: &Line, yt_tag: bool) -> String {
    if l.platform == "youtube" {
        if yt_tag {
            if l.user.starts_with('@') || l.user.contains(' ') {
                l.user.clone()
            } else {
                format!("@{}", l.user)
            }
        } else {
            l.user.trim_start_matches('@').to_string()
        }
    } else {
        l.user.clone()
    }
}

/// A line's things, in order.
fn items(cmds: &Cmds, l: &Line, tag: bool, width: i32) -> Vec<It> {
    let gray = Color::from_rgb(0x88, 0x88, 0x88);
    let (time, badges, _, yt_tag) = SHOW.get();
    let mut v = Vec::new();
    // The history divider: a line across, no name, no time.
    if l.welcome {
        return vec![It::Rule(l.text.clone())];
    }
    // What sets the message apart (highlighted, a first message...), above it.
    if !l.tag.is_empty() {
        let ink = accent(l).unwrap_or(gray);
        if let Some(p) = img::get(cmds, &l.picture, BADGE + 4, false) {
            v.push(It::Pic(p, None));
            v.push(It::Gap(6));
        }
        v.push(It::Text(l.tag.clone(), Font::HelveticaBold, ink));
        v.push(It::Gap(8));
        price(cmds, l, ink, &mut v);
        v.push(It::Break);
    }
    if let Some(r) = &l.reply {
        if let Some(s) = icons::icon("corner-up-left", gray, 14) {
            v.push(It::Svg(s));
            v.push(It::Gap(4));
        }
        reply_preview(r, gray, &mut v);
        v.push(It::Break);
    }
    if pinned(l) {
        if let Some(s) = icons::icon("pin", Color::Selection, 14) {
            v.push(It::Svg(s));
            v.push(It::Space);
        }
    }
    if time && !l.time.is_empty() {
        v.push(It::Text(l.time.clone(), Font::HelveticaItalic, gray));
        v.push(It::Space);
    }
    if tag {
        let p = PLATFORMS.iter().position(|x| *x == l.platform).unwrap_or(0);
        if let Some(s) = svg(NAMES[p]) {
            v.push(It::Svg(s));
            v.push(It::Space);
        }
    }
    if l.note {
        v.push(It::Text(l.text.clone(), Font::HelveticaItalic, gray));
        return v;
    }
    if !l.event.is_empty() {
        return event_items(cmds, l, v);
    }
    if badges {
        let before = v.len();
        if l.badges.is_empty() {
            // Role fallback using strictly this platform's own badge design. Twitch has real pictures only:
            // a drawn stand-in beside the real ones looked wrong.
            let mods = l.moderator && !l.broadcaster;
            for (role, on) in [
                ("broadcaster", l.broadcaster),
                ("moderator", mods),
                ("vip", l.vip),
                ("subscriber", l.subscriber),
            ] {
                if on && l.platform != "twitch" {
                    if let Some(s) = badge_fallback_svg(&l.platform, role) {
                        v.push(It::SvgTip(s, badge_tip(&l.platform, role, "")));
                        v.push(It::Gap(GAP));
                    }
                }
            }
        } else {
            for (name, url) in &l.badges {
                if !url.is_empty() {
                    if let Some(p) = img::get(cmds, url, BADGE, false) {
                        v.push(It::Pic(p, Some(badge_tip(&l.platform, name, url))));
                        v.push(It::Gap(GAP));
                        continue;
                    }
                }
                if l.platform == "twitch" {
                    continue;
                }
                if let Some(s) = badge_fallback_svg(&l.platform, name) {
                    v.push(It::SvgTip(s, badge_tip(&l.platform, name, url)));
                    v.push(It::Gap(GAP));
                }
            }
        }
        if v.len() > before {
            v.pop();
            v.push(It::Space);
        }
    }
    let name = display_user(l, yt_tag);
    v.push(It::Text(format!("{name}:"), Font::HelveticaBold, name_color(l)));
    if !l.deleted && !l.gif.is_empty() {
        if let Some(mut p) = img::get(cmds, &l.gif, 140, false) {
            if p.w() > width.max(1) {
                let h = (140 * width.max(1) / p.w()).max(1);
                if let Some(smaller) = img::get(cmds, &l.gif, h, false) {
                    p = smaller;
                } else {
                    words(cmds, l, &mut v);
                    return v;
                }
            }
            v.push(It::Break);
            v.push(It::Pic(p, None));
            return v;
        }
    }
    words(cmds, l, &mut v);
    v
}

/// Text on one line: line breaks and other control characters (a "\r" draws as "^M" and a "\n" as
/// overlapping rows) become spaces.
fn flat(s: &str) -> std::borrow::Cow<'_, str> {
    if s.chars().any(char::is_control) {
        s.chars()
            .map(|c| if c.is_control() { ' ' } else { c })
            .collect::<String>()
            .into()
    } else {
        s.into()
    }
}

/// What a line's sender wrote: its words, the emotes as pictures (struck through when deleted).
fn words(cmds: &Cmds, l: &Line, v: &mut Vec<It>) {
    let text = flat(&l.text);
    let fg = Color::Foreground;
    let pic = |url: &str| img::get(cmds, url, EMOTE, false);
    emotes::want_personal(cmds, &l.platform, &l.user_id);
    if l.deleted {
        for w in text.split(' ') {
            v.push(It::Space);
            v.push(It::Struck(w.to_string()));
        }
        return;
    }
    // A `/me` line reads in italics, in the sender's colour (as Twitch shows it).
    let (font, fg) = if l.action {
        (Font::HelveticaItalic, name_color(l))
    } else {
        (Font::Helvetica, fg)
    };
    // A gigantified message: its last emote is drawn big.
    let big = |w: &str| l.emotes.iter().any(|e| e.0 == w);
    let last = if l.huge {
        l.text
            .split(' ')
            .enumerate()
            .filter(|(_, w)| big(w))
            .map(|(i, _)| i)
            .last()
    } else {
        None
    };
    for (i, w) in text.split(' ').enumerate() {
        v.push(It::Space);
        if last == Some(i) {
            if let Some((_, u)) = l.emotes.iter().find(|e| e.0 == w) {
                if let Some(p) = img::get(cmds, u, HUGE, false) {
                    v.push(It::Pic(p, Some(emote_tip(&l.platform, w, u))));
                    continue;
                }
            }
        }
        if mentions_me(w) {
            // The chip is the name; a comma or "!" after it stays outside.
            let name = w.trim_end_matches(|c: char| !c.is_alphanumeric());
            v.push(It::Mention(name.to_string()));
            if name.len() < w.len() {
                v.push(It::Text(w[name.len()..].to_string(), font, fg));
            }
            continue;
        }
        let known = l.emotes.iter().find(|e| e.0 == w).map(|e| e.1.clone());
        if let Some((p, u)) = known
            .or_else(|| emotes::personal(&l.platform, &l.user_id, w))
            .or_else(|| emotes::third(&l.platform, w))
            .and_then(|u| pic(&u).map(|p| (p, u)))
        {
            v.push(It::Pic(p, Some(emote_tip(&l.platform, w, &u))));
            continue;
        }
        for (part, url) in emotes::split(w) {
            match url.and_then(|u| pic(&u).map(|p| (p, u))) {
                Some((p, u)) => v.push(It::Pic(p, Some(emote_tip(&l.platform, part, &u)))),
                None => v.push(It::Text(
                    super::tip::emote_name(&l.platform, part).to_string(),
                    font,
                    fg,
                )),
            }
        }
    }
}

/// Add a reply preview as separate words so it can wrap beside its icon.
fn reply_preview(r: &crate::engine::chatlog::Reply, gray: Color, v: &mut Vec<It>) {
    let reply = flat(&r.text);
    let mut said: String = reply.chars().take(60).collect();
    if said.len() < reply.len() {
        said.push('…');
    }
    let preview = trf("chat.replying_to", &[("user", &r.user), ("said", &said)]);
    for (i, word) in preview.split_whitespace().enumerate() {
        if i > 0 {
            v.push(It::Space);
        }
        if mentions_me(word) {
            v.push(It::Mention(word.to_string()));
        } else {
            v.push(It::Text(word.to_string(), Font::Helvetica, gray));
        }
    }
}

/// Whether `word` is an `@name` of one of our accounts ("@name," and "@name!" too).
fn mentions_me(word: &str) -> bool {
    let Some(name) = word.strip_prefix('@') else {
        return false;
    };
    let name = name.trim_end_matches(|c: char| !c.is_alphanumeric()).to_lowercase();
    !name.is_empty() && ME.with_borrow(|me| me.contains(&name))
}

/// The names of the accounts logged in (the ones `@name` tags are checked against).
fn my_names(c: &crate::Config) -> Vec<String> {
    let mut v: Vec<String> = [
        c.twitch
            .broadcaster
            .as_ref()
            .map(|a| (a.login.clone(), a.display_name.clone())),
        c.twitch.bot.as_ref().map(|a| (a.login.clone(), a.display_name.clone())),
        c.youtube.broadcaster.as_ref().map(|a| (a.name.clone(), String::new())),
        c.youtube.bot.as_ref().map(|a| (a.name.clone(), String::new())),
        c.kick.broadcaster.as_ref().map(|a| (a.username.clone(), String::new())),
        c.kick.bot.as_ref().map(|a| (a.username.clone(), String::new())),
    ]
    .into_iter()
    .flatten()
    .flat_map(|(a, b)| [a, b])
    .map(|n| n.trim().trim_start_matches('@').to_lowercase())
    .filter(|n| !n.is_empty())
    .collect();
    v.sort();
    v.dedup();
    v
}

/// A line's accent ("#RRGGBB") as a colour.
fn accent(l: &Line) -> Option<Color> {
    let hex = l.accent.trim_start_matches('#');
    let v = u32::from_str_radix(hex, 16).ok().filter(|_| hex.len() == 6)?;
    Some(Color::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

/// What a reward or a power-up cost, as Twitch's chat shows it: the coin (the channel's own picture of
/// its points when known, else a drawn one) and the number.
fn price(cmds: &Cmds, l: &Line, ink: Color, v: &mut Vec<It>) {
    if l.cost == 0 {
        return;
    }
    if l.cost_icon.starts_with("http") {
        if let Some(p) = img::get(cmds, &l.cost_icon, 16, false) {
            v.push(It::Pic(p, None));
            v.push(It::Gap(3));
        }
    } else {
        let coin = if l.cost_icon.is_empty() { "coins" } else { &l.cost_icon };
        if let Some(s) = icons::icon(coin, ink, 14) {
            v.push(It::Svg(s));
            v.push(It::Gap(3));
        }
    }
    v.push(It::Text(l.cost.to_string(), Font::HelveticaBold, ink));
}

/// An event's line: its icon, who, what happened; under it, what they wrote with it.
fn event_items(cmds: &Cmds, l: &Line, mut v: Vec<It>) -> Vec<It> {
    let c = accent(l).unwrap_or(Color::Foreground);
    if let Some(s) = icons::icon(&l.icon, c, 16) {
        v.push(It::Svg(s));
        v.push(It::Gap(6));
    }
    // What it was for, in a picture: a reward's icon, the emote it unlocked (big when gigantified).
    let pic = (!l.picture.is_empty() && !l.huge)
        .then(|| img::get(cmds, &l.picture, EMOTE, false))
        .flatten();
    if let Some(p) = pic {
        v.push(It::Pic(p, None));
        v.push(It::Gap(6));
    }
    if !l.user.is_empty() {
        let (_, _, _, yt_tag) = SHOW.get();
        v.push(It::Text(display_user(l, yt_tag), Font::HelveticaBold, name_color(l)));
    }
    for w in l.event.split(' ') {
        v.push(It::Space);
        v.push(It::Text(w.to_string(), Font::Helvetica, Color::Foreground));
    }
    if l.cost > 0 {
        v.push(It::Break);
        price(cmds, l, Color::Foreground, &mut v);
    }
    if l.huge {
        if let Some(p) = img::get(cmds, &l.picture, HUGE, false) {
            v.push(It::Break);
            v.push(It::Pic(p, None));
        }
    }
    if !l.text.is_empty() {
        v.push(It::Break);
        words(cmds, l, &mut v);
    }
    v
}

/// A line laid out: its rows (items with their x, and the row's height).
type Rows = Vec<(Vec<(It, i32)>, i32)>;

/// A piece of text wider than `w` (a long name in a narrow column) cut into pieces that fit, each on its
/// own row: left whole, the end of it is out of sight.
fn fitted(it: It, w: i32) -> Vec<It> {
    let It::Text(t, font, color) = &it else { return vec![it] };
    // Letters are never narrower than 3 px: short text needn't be measured.
    if w < 20 || (t.chars().count() as i32) * 3 <= w || it.size().0 <= w {
        return vec![it];
    }
    draw::set_font(*font, TEXT);
    let mut out = Vec::new();
    let mut piece = String::new();
    for c in t.chars() {
        piece.push(c);
        if draw::width(&piece) as i32 > w && piece.chars().count() > 1 {
            piece.pop();
            out.push(It::Text(std::mem::take(&mut piece), *font, *color));
            out.push(It::Break);
            piece.push(c);
        }
    }
    out.push(It::Text(piece, *font, *color));
    out
}

/// Lay a line out in `w` px. The words of a message that follow each other, in one style, are one piece
/// of text on a row: drawing text is most of what drawing the chat costs, and it costs by the piece.
fn rows(items: Vec<It>, w: i32) -> Rows {
    let mut out: Rows = vec![(Vec::new(), LINE)];
    let mut x = 0;
    // Spaces since the last thing put on the row (one is just a space between words).
    let mut spaces = 0;
    for it in items.into_iter().flat_map(|it| fitted(it, w)) {
        let (iw, ih) = it.size();
        if matches!(it, It::Break) {
            out.push((Vec::new(), LINE));
            x = 0;
            continue;
        }
        if matches!(it, It::Space | It::Gap(_)) {
            x += iw;
            spaces += if matches!(it, It::Space) { 1 } else { 2 };
            continue;
        }
        let row = out.last_mut().expect("a row");
        if x + iw > w && !row.0.is_empty() {
            out.push((Vec::new(), LINE));
            x = 0;
            spaces = 0;
        }
        let row = out.last_mut().expect("a row");
        row.1 = row.1.max(ih + 2);
        if let (It::Text(t, font, color), 1, Some((It::Text(before, f, c), _))) = (&it, spaces, row.0.last_mut()) {
            if f == font && c == color && *font != Font::HelveticaBold {
                before.push(' ');
                before.push_str(t);
                x += iw;
                spaces = 0;
                continue;
            }
        }
        spaces = 0;
        row.0.push((it, x));
        x += iw;
    }
    out
}

/// How much of a line shows, 0 to 1: coming in (from when it came to the pane, `born`), and going out
/// as its lifetime ends.
fn opacity(l: &Line, born: u64, now: u64, lim: &crate::ChatLimits) -> f32 {
    let mut o = if l.local { 0.55f32 } else { 1.0f32 };
    if lim.fade_in > 0 && born > 0 {
        o = o.min((now.saturating_sub(born)) as f32 / lim.fade_in as f32);
    }
    if lim.lifetime > 0 && l.ts > 0 {
        let left = (l.ts as u64 + lim.lifetime * 1000).saturating_sub(now);
        o = o.min(if lim.fade_out > 0 {
            left as f32 / lim.fade_out as f32
        } else if left > 0 {
            1.0
        } else {
            0.0
        });
    }
    o.clamp(0.0, 1.0)
}

/// Whether a line is past its lifetime.
fn st_ages_out(l: &Line, now: u64, lim: &crate::ChatLimits) -> bool {
    lim.lifetime > 0 && l.ts > 0 && now >= l.ts as u64 + lim.lifetime * 1000
}

/// `c` at opacity `o` over the chat's background.
fn dim(c: Color, o: f32) -> Color {
    if o >= 0.99 {
        c
    } else {
        Color::color_average(c, Color::Background2, o)
    }
}

struct State {
    lines: VecDeque<Line>,
    /// Each line laid out at `width`, and its height.
    laid: VecDeque<(Rows, i32)>,
    width: i32,
    tag: bool,
    /// Scrolled to (px from the top); following the newest lines when at the bottom.
    top: i32,
    follow: bool,
    /// Lines that came while scrolled up (the pill at the bottom counts them).
    unread: usize,
    /// An animation frame is due.
    ticking: bool,
    /// Where the emotes that move are drawn (in the pane), so a frame redraws only those.
    moving: Vec<(i32, i32, i32, i32)>,
    /// When each line came to this pane (ms; 0: it was there already), for its fade in.
    born: VecDeque<u64>,
    /// This pane's lines age: they fade in, and go when their lifetime is up (a user card's don't).
    ages: bool,
    /// The next time lines are looked at for fading and aging (epoch ms), and its timer.
    pulse: Option<(u64, app::TimeoutHandle)>,
    /// The line under the pointer, which shows its actions, and the action under it.
    hover: Option<usize>,
    act: Option<usize>,
    /// A name opens its user card (not inside a card: it's that user already).
    cards: bool,
    /// The messages pinned in the chats shown, drawn over the top; and a redraw due for their countdown.
    pins: Vec<Pin>,
    /// The polls and predictions going on, over the top too.
    polls: Vec<Poll>,
    pin_tick: bool,
}

/// What a notice shows: the platform it is about and its state.
type Shown = Option<(Option<&'static str>, ChatState)>;

/// A card over the top of the chat: a pinned message, or a poll or prediction going on.
enum Card<'a> {
    Pin(&'a Pin),
    Poll(&'a Poll),
}

/// Space around the cards, and inside them.
const CARD_GAP: i32 = 8;
const CARD_PAD: i32 = 12;
const CHOICE_H: i32 = 26;

/// The same bounds are used to draw cards and shield the chat beneath them.
fn card_contains(x: i32, y: i32, width: i32, top: i32, height: i32, px: i32, py: i32) -> bool {
    (x + CARD_GAP..x + width - 2).contains(&px) && (y + top..y + top + height).contains(&py)
}

impl Card<'_> {
    fn height(&self) -> i32 {
        match self {
            Card::Pin(_) => 70,
            Card::Poll(p) => CARD_PAD * 2 + 22 + 24 + p.choices.len() as i32 * (CHOICE_H + 4),
        }
    }
}

/// The platform's brand colour: a card's bar.
fn brand(platform: &str) -> Color {
    match platform {
        "youtube" => Color::from_rgb(0xff, 0x00, 0x33),
        "kick" => Color::from_rgb(0x53, 0xfc, 0x18),
        "tiktok" => Color::from_rgb(0xfe, 0x2c, 0x55),
        _ => Color::from_rgb(0x91, 0x46, 0xff),
    }
}

/// "4:05", "1:02:09": what's left of a pin.
fn left(ms: u64) -> String {
    let s = ms.div_ceil(1000);
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

/// A message's hover actions.
#[derive(Clone, Copy)]
enum Act {
    Reply,
    Copy,
    Mod(Moderate),
}

/// An action's square in the bar.
const ACT: i32 = 28;

/// The actions `l` offers (with their icon and tooltip): only what our role in that chat allows
/// (roles.rs): nothing on the broadcaster, nothing but copy without a login.
fn acts(l: &Line, reply: bool) -> Vec<(Act, &'static str, &'static str)> {
    if l.note || l.deleted {
        return Vec::new();
    }
    let p = l.platform.as_str();
    let (me, my_id) = crate::roles::get(p);
    let may = crate::roles::allowed(
        p,
        me,
        &my_id,
        crate::roles::Target {
            user_id: &l.user_id,
            broadcaster: l.broadcaster,
            moderator: l.moderator,
        },
    );
    let mut v = vec![(Act::Copy, "copy", tr("chat.act_copy"))];
    // An event isn't a message to answer, and a pane with no box under it has nowhere to answer.
    if reply && may.reply && l.event.is_empty() {
        v.insert(0, (Act::Reply, "corner-up-left", tr("chat.act_reply")));
    }
    if may.pin && !l.msg_id.is_empty() {
        v.push(if pinned(l) {
            (Act::Mod(Moderate::Unpin), "pin-off", tr("chat.act_unpin"))
        } else {
            (Act::Mod(Moderate::Pin(None)), "pin", tr("chat.act_pin"))
        });
    }
    if may.delete && !l.msg_id.is_empty() {
        v.push((Act::Mod(Moderate::Delete), "trash-2", tr("chat.act_delete_message")));
    }
    if may.timeout {
        v.push((Act::Mod(Moderate::Timeout(0)), "clock", tr("chat.act_timeout")));
    }
    if may.ban {
        v.push((Act::Mod(Moderate::Ban), "ban", tr("chat.act_ban")));
    }
    v
}

/// What Reply does: the chat box under the pane answers `l`.
type OnReply = Rc<RefCell<Option<Rc<dyn Fn(&Line)>>>>;
/// Answer a line in a chat box (None: stop answering).
pub(super) type SetReply = Rc<dyn Fn(Option<&Line>)>;

/// One chat view: the lines, drawn, with a scrollbar. The chat's columns have one, and so has a user card.
#[derive(Clone)]
pub(super) struct Pane {
    area: Frame,
    bar: Slider,
    st: Rc<RefCell<State>>,
    cmds: Cmds,
    pub(super) on_reply: OnReply,
    disconnected: Rc<Cell<bool>>,
    /// The "disconnected" is because no chat is connected at all (so the text says that).
    nothing: Rc<Cell<bool>>,
}

impl Pane {
    fn set_disconnected(&self, disconnected: bool) {
        if self.disconnected.replace(disconnected) != disconnected {
            self.area.clone().redraw();
        }
    }
    fn set_nothing(&self, nothing: bool) {
        if self.nothing.replace(nothing) != nothing {
            self.area.clone().redraw();
        }
    }
    pub(super) fn new(cmds: &Cmds) -> (Flex, Pane) {
        let mut group = Flex::default().row();
        group.set_pad(0);
        group.set_frame(FrameType::DownBox);
        group.set_color(Color::Background2);
        group.set_margin(2);
        let mut area = Frame::default();
        area.set_frame(FrameType::FlatBox);
        area.set_color(Color::Background2);
        // A slider has no arrows: drawn over as a thin rounded bar, like the lists'.
        let mut bar = Slider::default();
        bar.draw(|b| {
            let (x, y, w, h) = (b.x(), b.y(), b.w(), b.h());
            draw::draw_rect_fill(x, y, w, h, Color::Background2);
            let max = b.maximum() - b.minimum();
            if max <= 0.0 {
                return;
            }
            let th = ((h as f32 * b.slider_size()) as i32).clamp(24.min(h), h);
            let ty = y + ((h - th) as f64 * (b.value() - b.minimum()) / max) as i32;
            let c = Color::color_average(Color::Foreground, Color::Background, 0.3);
            draw::set_draw_color(c);
            draw::draw_rounded_rectf(x + (w - 6) / 2, ty, 6, th, 3);
        });
        group.fixed(&bar, 12);
        group.end();
        let p = Pane {
            area: area.clone(),
            bar: bar.clone(),
            st: Rc::new(RefCell::new(State {
                lines: VecDeque::new(),
                laid: VecDeque::new(),
                width: -1,
                tag: false,
                top: 0,
                follow: true,
                unread: 0,
                ticking: false,
                moving: Vec::new(),
                born: VecDeque::new(),
                ages: false,
                pulse: None,
                hover: None,
                act: None,
                cards: true,
                pins: Vec::new(),
                polls: Vec::new(),
                pin_tick: false,
            })),
            cmds: cmds.clone(),
            on_reply: Rc::default(),
            disconnected: Rc::default(),
            nothing: Rc::default(),
        };
        area.draw({
            let p = p.clone();
            move |f| p.draw(f)
        });
        area.handle({
            let p = p.clone();
            move |f, ev| p.handle(f, ev)
        });
        bar.set_callback({
            let p = p.clone();
            move |b| {
                let mut st = p.st.borrow_mut();
                st.top = b.value() as i32;
                st.follow = st.top >= p.total(&st) - p.area.h() + 4;
                drop(st);
                p.area.clone().redraw();
            }
        });
        (group, p)
    }

    fn total(&self, st: &State) -> i32 {
        st.laid.iter().map(|l| l.1).sum::<i32>() + 8 + self.head(st)
    }

    /// The cards at the top (polls, then pins), each with its distance from the top.
    fn cards<'a>(&self, st: &'a State) -> Vec<(Card<'a>, i32)> {
        let ms = epoch_ms();
        let mut y = CARD_GAP;
        let mut out = Vec::new();
        for c in st
            .polls
            .iter()
            .filter(|p| p.live(ms))
            .map(Card::Poll)
            .chain(st.pins.iter().filter(|p| p.live(ms)).map(Card::Pin))
        {
            let h = c.height();
            out.push((c, y));
            y += h + CARD_GAP;
        }
        out
    }

    /// The room the cards take at the top: the lines start under them.
    fn head(&self, st: &State) -> i32 {
        self.cards(st).last().map_or(0, |(c, y)| y + c.height() + CARD_GAP)
    }

    fn lay(&self, l: &Line, tag: bool, w: i32) -> (Rows, i32) {
        let r = rows(items(&self.cmds, l, tag, w), w);
        let h = r.iter().map(|r| r.1).sum();
        (r, h)
    }

    /// Lay the lines out again when the width changed (or pictures came in).
    fn measure(&self, st: &mut State) {
        let w = self.area.w() - 12;
        if st.width == w && st.laid.len() == st.lines.len() {
            return;
        }
        st.width = w;
        let tag = st.tag;
        st.laid = st.lines.iter().map(|l| self.lay(l, tag, w)).collect();
    }

    /// Scrollbar and position after the lines or the size changed.
    fn settle(&self, st: &mut State) {
        let mut bar = self.bar.clone();
        if !LIMITS.get().save_history {
            bar.set_bounds(0.0, 0.0);
            bar.set_slider_size(1.0);
            bar.set_value(0.0);
            return;
        }
        self.measure(st);
        let (total, h) = (self.total(st), self.area.h());
        let max = (total - h).max(0);
        if st.follow {
            st.top = max;
            st.unread = 0;
        }
        st.top = st.top.clamp(0, max);
        bar.set_bounds(0.0, f64::from(max));
        bar.set_slider_size(if total > 0 {
            (h as f32 / total as f32).min(1.0)
        } else {
            1.0
        });
        bar.set_value(f64::from(st.top));
        bar.set_step(1.0, 1);
    }

    fn draw(&self, f: &mut Frame) {
        if self.disconnected.get() {
            draw::draw_box(f.frame(), f.x(), f.y(), f.w(), f.h(), f.color());
            draw::set_font(Font::HelveticaBold, 13);
            draw::set_draw_color(super::muted());
            draw::draw_text2(
                if self.nothing.get() {
                    tr("chat.no_connection")
                } else {
                    tr("chat.disconnected")
                },
                f.x() + 4,
                f.y(),
                (f.w() - 8).max(1),
                f.h(),
                Align::Center | Align::Inside | Align::Clip,
            );
            return;
        }
        let mut st = self.st.borrow_mut();
        self.settle(&mut st);
        draw::draw_box(f.frame(), f.x(), f.y(), f.w(), f.h(), f.color());
        draw::push_clip(f.x() + 2, f.y() + 2, f.w() - 4, f.h() - 4);
        if !LIMITS.get().save_history {
            let cx = f.x() + 8;
            let cw = (f.w() - 16).max(10);
            let cy = f.y() + (f.h() - 160).max(10) / 2;
            if let Some(mut icon) = icons::icon("eye-off", super::muted(), 32) {
                icon.draw(f.x() + (f.w() - 32) / 2, cy, 32, 32);
            }
            draw::set_font(Font::HelveticaBold, 13);
            draw::set_draw_color(Color::Foreground);
            let title = tr("chat.history_disabled_title");
            draw::draw_text2(title, cx, cy + 40, cw, 36, Align::Center | Align::Wrap);
            draw::set_font(Font::Helvetica, 11);
            draw::set_draw_color(super::muted());
            let desc = tr("chat.history_disabled_desc");
            draw::draw_text2(desc, cx, cy + 80, cw, 80, Align::Center | Align::Wrap);
            draw::pop_clip();
            return;
        }
        let (ms, animate) = (now_ms(), SHOW.get().2 && !self.disconnected.get());
        // An animation frame redraws only the moving emotes (the text is most of the work): it damages
        // just their squares. Anything else damages it all, and finds the moving emotes again.
        let partial = f.damage_type() == Damage::User1;
        let (lim, now) = (LIMITS.get().clamped(), epoch_ms());
        let mut found = Vec::new();
        let mut y = f.y() + 4 + self.head(&st) - st.top;
        for (i, (laid, h)) in st.laid.iter().enumerate() {
            if y + h < f.y() {
                y += h;
                continue;
            }
            if y > f.y() + f.h() {
                break;
            }
            if partial && !draw::not_clipped(f.x(), y, f.w(), *h) {
                y += h;
                continue;
            }
            // A line coming or going is damaged whole (to its right edge): drawn whole, dimmed; the others
            // in a partial draw are only their moving emotes.
            let whole = !partial || draw::not_clipped(f.x() + f.w() - 8, y, 4, *h);
            let o = match (st.lines.get(i), st.born.get(i)) {
                (Some(l), Some(b)) => opacity(l, *b, now, &lim),
                _ => 1.0,
            };
            // Events and set-apart messages: a tint of their colour, and a bar at the left.
            if let Some(c) = st.lines.get(i).and_then(accent) {
                draw::draw_rect_fill(
                    f.x() + 2,
                    y,
                    f.w() - 4,
                    *h,
                    Color::color_average(c, Color::Background2, 0.14 * o),
                );
                draw::draw_rect_fill(f.x() + 2, y, 3, *h, dim(c, o));
            }
            if st.hover == Some(i) {
                draw::draw_rect_fill(f.x() + 2, y, f.w() - 4, *h, Color::from_rgb(0x2c, 0x2e, 0x34));
            }
            for (row, rh) in laid {
                let rh = *rh;
                for (it, x) in row {
                    let x = f.x() + 6 + x;
                    // A frame of the animation only needs the moving emotes.
                    if !whole && !matches!(it, It::Pic(..)) {
                        continue;
                    }
                    match it {
                        It::Text(t, font, color) => {
                            draw::set_font(*font, TEXT);
                            draw::set_draw_color(dim(*color, o));
                            draw::draw_text2(t, x, y, 0, rh, Align::Left);
                        }
                        // Pictures can't be dimmed: they come in once the line is half there.
                        It::Pic(..) | It::Svg(_) | It::SvgTip(..) if o < 0.5 => {}
                        It::Pic(p, _) => {
                            let mut i = if animate { p.at(ms).clone() } else { p.first() };
                            let at = y + (rh - i.h()) / 2;
                            if animate && p.animated() {
                                found.push((x - f.x(), at - f.y(), i.w(), i.h()));
                            }
                            i.draw(x, at, i.w(), i.h());
                        }
                        It::Svg(s) | It::SvgTip(s, _) => {
                            let (w, h) = (s.w(), s.h());
                            s.clone().draw(x, y + (rh - h) / 2, w, h);
                        }
                        It::Struck(t) => {
                            draw::set_font(Font::Helvetica, TEXT);
                            draw::set_draw_color(dim(Color::from_rgb(0x88, 0x88, 0x88), o));
                            draw::draw_text2(t, x, y, 0, rh, Align::Left);
                            let w = draw::width(t) as i32;
                            // Through the space before it too: one line across the message.
                            let sp = draw::width(" ") as i32;
                            draw::draw_line(x - sp, y + rh / 2, x + w, y + rh / 2);
                        }
                        It::Rule(t) => {
                            // Across the whole chat, the words in the middle on a gap in the line.
                            draw::set_font(Font::Helvetica, 12);
                            let tw = draw::width(t) as i32;
                            let (left, right) = (f.x() + 8, f.x() + f.w() - 20);
                            let mid = (left + right) / 2;
                            let ink = Color::color_average(Color::Foreground, Color::Background2, 0.35);
                            draw::set_draw_color(ink);
                            draw::draw_line(left, y + rh / 2, mid - tw / 2 - 10, y + rh / 2);
                            draw::draw_line(mid + tw / 2 + 10, y + rh / 2, right, y + rh / 2);
                            draw::draw_text2(t, mid - tw / 2, y, tw, rh, Align::Center);
                        }
                        It::Mention(t) => {
                            draw::set_font(Font::Helvetica, TEXT);
                            let w = draw::width(t) as i32 + 2 * MENTION_PAD;
                            // A tint of the text's own colour over the background: right in light and dark.
                            draw::set_draw_color(Color::color_average(Color::Foreground, Color::Background2, 0.2));
                            draw::draw_rounded_rectf(x, y + (rh - TEXT - 4) / 2, w, TEXT + 4, 4);
                            draw::set_draw_color(Color::Foreground);
                            draw::draw_text2(t, x + MENTION_PAD, y, 0, rh, Align::Left);
                        }
                        It::Space | It::Gap(_) | It::Break => {}
                    }
                }
                y += rh;
            }
        }
        if let Some((l, bx, by)) = st.hover.and_then(|i| self.bar_at(f, &st, i)) {
            let a = acts(l, self.on_reply.borrow().is_some());
            let (fg, bg) = (Color::Foreground, Color::Background);
            let n = a.len() as i32;
            draw::set_draw_color(bg);
            draw::draw_rounded_rectf(bx, by, n * ACT, ACT, 6);
            draw::set_draw_color(Color::color_average(fg, bg, 0.15));
            draw::draw_rounded_rect(bx, by, n * ACT, ACT, 6);
            for (k, (_, icon, _)) in a.iter().enumerate() {
                let x = bx + k as i32 * ACT;
                if st.act == Some(k) {
                    draw::set_draw_color(Color::color_average(fg, bg, 0.2));
                    draw::draw_rounded_rectf(x + 2, by + 2, ACT - 4, ACT - 4, 5);
                }
                let c = match *icon {
                    "ban" => Color::from_rgb(0xf0, 0x6a, 0x6a),
                    _ if st.act == Some(k) => fg,
                    _ => Color::color_average(fg, bg, 0.75),
                };
                if let Some(mut s) = icons::icon(icon, c, 16) {
                    s.draw(x + (ACT - 16) / 2, by + (ACT - 16) / 2, 16, 16);
                }
            }
        }
        let counting = self.draw_cards(f, &st);
        if let Some((x, y, w, h)) = self.pill(f, &st) {
            let (fg, bg) = (Color::Foreground, Color::Background);
            draw::set_draw_color(Color::color_average(fg, bg, 0.12));
            draw::draw_rounded_rectf(x, y, w, h, h / 2);
            draw::set_draw_color(Color::color_average(fg, bg, 0.3));
            draw::draw_rounded_rect(x, y, w, h, h / 2);
            if let Some(mut i) = icons::icon("chevron-down", fg, 14) {
                i.draw(x + 10, y + (h - 14) / 2, 14, 14);
            }
            draw::set_font(Font::HelveticaBold, 12);
            draw::set_draw_color(fg);
            draw::draw_text2(&self.pill_text(&st), x + 28, y, w - 36, h, Align::Left | Align::Inside);
        }
        draw::pop_clip();
        // A pin's countdown: a redraw a second, only while one with an end is shown.
        if counting && !st.pin_tick {
            st.pin_tick = true;
            let p = self.clone();
            app::add_timeout3(1.0, move |_| {
                p.st.borrow_mut().pin_tick = false;
                if !p.area.was_deleted() {
                    p.area.clone().redraw();
                }
            });
        }
        // Animated emotes on screen: their squares redrawn ~20 times a second, only while this is drawn.
        if !partial {
            st.moving = found;
        }
        if !st.moving.is_empty() && !st.ticking {
            st.ticking = true;
            let p = self.clone();
            app::add_timeout3(0.05, move |_| {
                let moving = {
                    let mut st = p.st.borrow_mut();
                    st.ticking = false;
                    st.moving.clone()
                };
                if !p.area.was_deleted() {
                    let mut a = p.area.clone();
                    for (x, y, w, h) in moving {
                        a.set_damage_area(Damage::User1, a.x() + x, a.y() + y, w, h);
                    }
                }
            });
        }
    }

    fn pill_text(&self, st: &State) -> String {
        match st.unread {
            1 => tr("chat.new_message").to_string(),
            n => trf("chat.new_messages", &[("n", &n.to_string())]),
        }
    }

    /// The "new messages" pill (x, y, w, h), when scrolled up and lines came.
    fn pill(&self, f: &Frame, st: &State) -> Option<(i32, i32, i32, i32)> {
        if st.follow || st.unread == 0 {
            return None;
        }
        draw::set_font(Font::HelveticaBold, 12);
        let w = (draw::width(&self.pill_text(st)) as i32 + 44).min(f.w() - 16);
        let h = 28;
        Some((f.x() + (f.w() - w) / 2, f.y() + f.h() - h - 8, w, h))
    }

    /// The cards over the top of the chat. Whether one counts down (a redraw a second).
    fn draw_cards(&self, f: &Frame, st: &State) -> bool {
        let (fg, bg) = (Color::Foreground, Color::Background);
        let muted = Color::color_average(fg, bg, 0.6);
        let ms = epoch_ms();
        let mut counting = false;
        for (card, top) in self.cards(st) {
            let (x, y, w, h) = (
                f.x() + CARD_GAP,
                f.y() + top,
                (f.w() - CARD_GAP - 2).max(0),
                card.height(),
            );
            let platform = match &card {
                Card::Pin(p) => p.platform.as_str(),
                Card::Poll(p) => p.platform.as_str(),
            };
            draw::set_draw_color(Color::color_average(fg, bg, 0.07));
            draw::draw_rounded_rectf(x, y, w, h, 8);
            draw::set_draw_color(brand(platform));
            draw::draw_rounded_rectf(x, y + 8, 3, h - 16, 1);
            let ix = x + CARD_PAD + 4;
            let room = w - CARD_PAD * 2 - 4;
            match card {
                Card::Pin(p) => {
                    if let Some(mut i) = icons::icon("pin", muted, 14) {
                        i.draw(ix, y + CARD_PAD + 1, 14, 14);
                    }
                    let who = if p.by.is_empty() {
                        tr("chat.pinned").to_string()
                    } else {
                        trf("chat.pinned_by", &[("user", &p.by)])
                    };
                    let when = match p.until {
                        Some(u) => {
                            counting = true;
                            trf("chat.pin_left", &[("time", &left(u.saturating_sub(ms)))])
                        }
                        None if p.platform == "youtube" => tr("chat.pin_until_unpinned").to_string(),
                        None => tr("chat.pin_until_end").to_string(),
                    };
                    draw::set_font(Font::Helvetica, 12);
                    draw::set_draw_color(muted);
                    let ww = draw::width(&when) as i32;
                    draw::draw_text2(&when, x + w - ww - CARD_PAD - 24, y + CARD_PAD, ww, 16, Align::Left);
                    if p.platform == "twitch" && crate::roles::get(&p.platform).0.moderates() && !p.msg_id.is_empty() {
                        if let Some(mut i) = icons::icon("ellipsis", muted, 16) {
                            i.draw(x + w - CARD_PAD - 16, y + CARD_PAD, 16, 16);
                        }
                    }
                    draw::draw_text2(
                        &who,
                        ix + 20,
                        y + CARD_PAD,
                        (room - ww - 52).max(0),
                        16,
                        Align::Left | Align::Clip,
                    );
                    draw::set_font(Font::HelveticaBold, TEXT);
                    draw::set_draw_color(fg);
                    let name = format!("{}:", p.user);
                    let nw = draw::width(&name) as i32;
                    draw::draw_text2(&name, ix, y + CARD_PAD + 26, nw, 22, Align::Left);
                    draw::set_font(Font::Helvetica, TEXT);
                    let left_room = (room - nw - 6).max(0);
                    let said = super::fit::ellipsize(&p.text, left_room, |t| draw::width(t) as i32);
                    draw::draw_text2(
                        &said,
                        ix + nw + 6,
                        y + CARD_PAD + 26,
                        left_room,
                        22,
                        Align::Left | Align::Clip,
                    );
                }
                Card::Poll(p) => {
                    if let Some(mut i) = icons::icon(if p.prediction { "trending-up" } else { "vote" }, muted, 14) {
                        i.draw(ix, y + CARD_PAD + 1, 14, 14);
                    }
                    let kind = if p.prediction {
                        tr("chat.prediction")
                    } else {
                        tr("chat.poll")
                    };
                    let when = match (p.over, p.ends) {
                        (Some(_), _) if p.prediction => tr("chat.poll_locked").to_string(),
                        (Some(_), _) => tr("chat.poll_ended").to_string(),
                        (None, Some(e)) => {
                            counting = true;
                            trf("chat.pin_left", &[("time", &left(e.saturating_sub(ms)))])
                        }
                        _ => String::new(),
                    };
                    if p.over.is_some() {
                        counting = true;
                    }
                    draw::set_font(Font::Helvetica, 12);
                    draw::set_draw_color(muted);
                    let ww = draw::width(&when) as i32;
                    draw::draw_text2(&when, x + w - ww - CARD_PAD, y + CARD_PAD, ww, 16, Align::Left);
                    draw::draw_text2(
                        kind,
                        ix + 20,
                        y + CARD_PAD,
                        (room - ww - 28).max(0),
                        16,
                        Align::Left | Align::Clip,
                    );
                    draw::set_font(Font::HelveticaBold, TEXT);
                    draw::set_draw_color(fg);
                    let title = super::fit::ellipsize(&p.title, room, |t| draw::width(t) as i32);
                    draw::draw_text2(&title, ix, y + CARD_PAD + 22, room, 22, Align::Left | Align::Clip);
                    let total: u64 = p.choices.iter().map(|c| c.1).sum();
                    let best = p.choices.iter().map(|c| c.1).max().unwrap_or(0);
                    for (i, (label, n)) in p.choices.iter().enumerate() {
                        let cy = y + CARD_PAD + 22 + 28 + i as i32 * (CHOICE_H + 4);
                        draw::set_draw_color(Color::color_average(fg, bg, 0.07));
                        draw::draw_rounded_rectf(ix, cy, room, CHOICE_H, 6);
                        let share = if total == 0 { 0.0 } else { *n as f32 / total as f32 };
                        let fill = (room as f32 * share) as i32;
                        if fill > 0 {
                            draw::set_draw_color(Color::color_average(brand(&p.platform), bg, 0.35));
                            draw::draw_rounded_rectf(ix, cy, fill.max(8), CHOICE_H, 6);
                        }
                        let stat = if p.prediction {
                            n.to_string()
                        } else {
                            format!("{}% ({n})", (share * 100.0).round() as u64)
                        };
                        let font = if *n == best && best > 0 {
                            Font::HelveticaBold
                        } else {
                            Font::Helvetica
                        };
                        draw::set_font(font, 13);
                        draw::set_draw_color(fg);
                        let sw = draw::width(&stat) as i32;
                        draw::draw_text2(&stat, ix + room - sw - 8, cy, sw, CHOICE_H, Align::Left);
                        let lab = super::fit::ellipsize(label, (room - sw - 28).max(0), |t| draw::width(t) as i32);
                        draw::draw_text2(
                            &lab,
                            ix + 8,
                            cy,
                            (room - sw - 24).max(0),
                            CHOICE_H,
                            Align::Left | Align::Clip,
                        );
                    }
                }
            }
        }
        counting
    }

    /// The pin under the pointer, if any.
    fn pin_at(&self, f: &Frame) -> Option<Pin> {
        let st = self.st.borrow();
        self.cards(&st).into_iter().find_map(|(c, top)| match c {
            Card::Pin(p) if card_contains(f.x(), f.y(), f.w(), top, c.height(), app::event_x(), app::event_y()) => {
                Some(p.clone())
            }
            _ => None,
        })
    }

    /// The polls and predictions of the chats this pane shows.
    pub(super) fn set_polls(&self, polls: Vec<Poll>) {
        let mut st = self.st.borrow_mut();
        if st.polls != polls {
            st.polls = polls;
            drop(st);
            self.area.clone().redraw();
        }
    }

    /// The pins of the chats this pane shows.
    pub(super) fn set_pins(&self, pins: Vec<Pin>) {
        let mut st = self.st.borrow_mut();
        if st.pins != pins {
            st.pins = pins;
            drop(st);
            self.area.clone().redraw();
        }
    }

    /// The line under the pointer's y.
    fn line_at(&self, f: &Frame, st: &State) -> Option<usize> {
        let mut y = f.y() + 4 + self.head(st) - st.top;
        for (i, (_, h)) in st.laid.iter().enumerate() {
            if (y..y + h).contains(&app::event_y()) {
                return Some(i);
            }
            y += h;
        }
        None
    }

    /// Line `i` and where its action bar goes: at the right of its first row, kept inside the view.
    fn bar_at<'a>(&self, f: &Frame, st: &'a State, i: usize) -> Option<(&'a Line, i32, i32)> {
        let l = st.lines.get(i)?;
        let n = acts(l, self.on_reply.borrow().is_some()).len() as i32;
        if n == 0 {
            return None;
        }
        let y = f.y() + 4 + self.head(st) - st.top + st.laid.iter().take(i).map(|l| l.1).sum::<i32>();
        let first = st.laid.get(i)?.0.first().map_or(LINE, |r| r.1);
        let by = (y + (first - ACT) / 2).clamp(f.y() + 4, f.y() + f.h() - ACT - 4);
        Some((l, f.x() + f.w() - n * ACT - 8, by))
    }

    /// The hovered line's action under the pointer: its index in `acts`.
    fn act_index(&self, f: &Frame, st: &State) -> Option<usize> {
        let (l, bx, by) = self.bar_at(f, st, st.hover?)?;
        let (ex, ey) = (app::event_x(), app::event_y());
        let k = (ex - bx).div_euclid(ACT);
        ((by..by + ACT).contains(&ey) && (0..acts(l, self.on_reply.borrow().is_some()).len() as i32).contains(&k))
            .then_some(k as usize)
    }

    /// The action under the pointer, on the hovered line: the line and its `acts` entry.
    fn act_at(&self, f: &Frame) -> Option<(Line, (Act, &'static str, &'static str))> {
        let st = self.st.borrow();
        let k = self.act_index(f, &st)?;
        let l = st.lines.get(st.hover?)?;
        Some((l.clone(), acts(l, self.on_reply.borrow().is_some())[k]))
    }

    fn run(&self, l: &Line, a: Act) {
        match a {
            Act::Reply => {
                let f = self.on_reply.borrow().clone();
                if let Some(f) = f {
                    f(l);
                }
            }
            Act::Mod(Moderate::Pin(_)) => {
                const SECONDS: [Option<u64>; 7] =
                    [Some(30), Some(60), Some(120), Some(300), Some(600), Some(1800), None];
                let labels = [
                    tr("chat.pin_30s"),
                    tr("chat.pin_1m"),
                    tr("chat.pin_2m"),
                    tr("chat.pin_5m"),
                    tr("chat.pin_10m"),
                    tr("chat.pin_30m"),
                    tr("chat.pin_stream_ends"),
                ];
                if let Some(idx) = super::pick(&labels) {
                    if let Some(&secs) = SECONDS.get(idx) {
                        let l = l.clone();
                        tell(&self.cmds, move |e| e.moderate(&l, Moderate::Pin(secs)));
                    }
                }
            }
            Act::Copy => app::copy(&l.text),
            Act::Mod(Moderate::Timeout(_)) => {
                const TIMEOUTS: [u64; 5] = [60, 600, 3600, 86_400, 604_800];
                let labels = [
                    tr("chat.timeout_1m"),
                    tr("chat.timeout_10m"),
                    tr("chat.timeout_1h"),
                    tr("chat.timeout_1d"),
                    tr("chat.timeout_1w"),
                ];
                if let Some(idx) = super::pick(&labels) {
                    if let Some(&secs) = TIMEOUTS.get(idx) {
                        let l = l.clone();
                        tell(&self.cmds, move |e| e.moderate(&l, Moderate::Timeout(secs)));
                    }
                }
            }
            Act::Mod(Moderate::Ban) => {
                let q = trf("chat.ban_confirm", &[("user", &l.user), ("platform", &l.platform)]).replace('@', "@@");
                if super::dialogs::choice(&q, tr("dialogs.cancel"), tr("btn.ban")) {
                    let l = l.clone();
                    tell(&self.cmds, move |e| e.moderate(&l, Moderate::Ban));
                }
            }
            Act::Mod(m) => {
                let l = l.clone();
                tell(&self.cmds, move |e| e.moderate(&l, m));
            }
        }
    }

    /// The line whose name is under the pointer.
    fn name_at(&self, f: &Frame) -> Option<Line> {
        let (ex, ey) = (app::event_x() - f.x() - 6, app::event_y());
        let st = self.st.borrow();
        if !st.cards {
            return None;
        }
        let mut y = f.y() + 4 + self.head(&st) - st.top;
        for (l, (laid, h)) in st.lines.iter().zip(&st.laid) {
            if !(y..y + h).contains(&ey) {
                y += h;
                continue;
            }
            for (row, rh) in laid {
                if (y..y + rh).contains(&ey) {
                    let name = row.iter().find(|(it, x)| {
                        matches!(it, It::Text(_, Font::HelveticaBold, _)) && (*x..x + it.size().0).contains(&ex)
                    });
                    return name.filter(|_| !l.note && !l.user_id.is_empty()).map(|_| l.clone());
                }
                y += rh;
            }
            return None;
        }
        None
    }

    /// The emote or badge under the pointer.
    fn tip_at(&self, f: &Frame) -> Option<Rc<Tip>> {
        let (ex, ey) = (app::event_x() - f.x() - 6, app::event_y());
        let st = self.st.borrow();
        let mut y = f.y() + 4 + self.head(&st) - st.top;
        for (laid, h) in &st.laid {
            if !(y..y + h).contains(&ey) {
                y += h;
                continue;
            }
            for (row, rh) in laid {
                if (y..y + rh).contains(&ey) {
                    return row.iter().find_map(|(it, x)| match it {
                        It::Pic(p, Some(t)) if (*x..x + p.w()).contains(&ex) => Some(t.clone()),
                        It::SvgTip(s, t) if (*x..x + s.w()).contains(&ex) => Some(t.clone()),
                        _ => None,
                    });
                }
                y += rh;
            }
            return None;
        }
        None
    }

    fn handle(&self, f: &mut Frame, ev: Event) -> bool {
        if self.disconnected.get() {
            return false;
        }
        if !LIMITS.get().save_history {
            return false;
        }
        if matches!(ev, Event::Push | Event::MouseWheel | Event::Leave | Event::Hide) {
            tip::point(&self.cmds, f, None);
        }
        match ev {
            Event::Enter => true,
            Event::Move => {
                let over_card = {
                    let st = self.st.borrow();
                    self.cards(&st).iter().any(|(card, top)| {
                        card_contains(f.x(), f.y(), f.w(), *top, card.height(), app::event_x(), app::event_y())
                    })
                };
                if over_card {
                    let mut st = self.st.borrow_mut();
                    let changed = st.hover.take().is_some() | st.act.take().is_some();
                    if changed {
                        f.redraw();
                    }
                    drop(st);
                    f.set_tooltip("");
                    tip::point(&self.cmds, f, None);
                    if let Some(mut w) = f.window() {
                        w.set_cursor(fltk::enums::Cursor::Default);
                    }
                    return true;
                }
                {
                    let mut st = self.st.borrow_mut();
                    // On its action bar (which may stick out over the next line), the line stays.
                    let act = self.act_index(f, &st);
                    let hover = if act.is_some() { st.hover } else { self.line_at(f, &st) };
                    if (st.hover, st.act) != (hover, act) {
                        (st.hover, st.act) = (hover, act);
                        f.redraw();
                    }
                }
                let act = self.act_at(f);
                // A deleted message the moderator can still read: its words show on hover.
                let old = {
                    let st = self.st.borrow();
                    st.hover
                        .and_then(|i| st.lines.get(i))
                        .filter(|l| l.deleted && l.text != "message deleted")
                        .map(|l| flat(&l.text).to_string())
                };
                f.set_tooltip(act.as_ref().map_or(old.as_deref().unwrap_or(""), |a| a.1 .2));
                let over = if act.is_some() { None } else { self.tip_at(f) };
                tip::point(&self.cmds, f, over);
                let hand = act.is_some() || self.name_at(f).is_some();
                if let Some(mut w) = f.window() {
                    w.set_cursor(if hand {
                        fltk::enums::Cursor::Hand
                    } else {
                        fltk::enums::Cursor::Default
                    });
                }
                true
            }
            Event::Leave => {
                let mut st = self.st.borrow_mut();
                st.act = None;
                if st.hover.take().is_some() {
                    f.redraw();
                }
                drop(st);
                if let Some(mut w) = f.window() {
                    w.set_cursor(fltk::enums::Cursor::Default);
                }
                true
            }
            Event::Push if super::left_button() && self.on_pill(f) => {
                let mut st = self.st.borrow_mut();
                st.follow = true;
                self.settle(&mut st);
                drop(st);
                f.redraw();
                true
            }
            // The menu is also available by right-clicking the card.
            Event::Push if self.pin_at(f).is_some() => {
                let pin = self.pin_at(f).expect("a pin");
                let menu_click = app::event_x() >= f.x() + f.w() - 64;
                if super::right_button() || menu_click {
                    use fltk::menu::{MenuButton, MenuFlag};
                    let mut m = super::menu_at_pointer();
                    let (role, _) = crate::roles::get(&pin.platform);
                    if pin.platform == "twitch" && role.moderates() && !pin.msg_id.is_empty() {
                        let l = Line {
                            platform: pin.platform.clone(),
                            msg_id: pin.msg_id.clone(),
                            ..Default::default()
                        };
                        let cmds = self.cmds.clone();
                        let unpin = l.clone();
                        let i = m.add(
                            tr("chat.act_unpin"),
                            fltk::enums::Shortcut::None,
                            MenuFlag::Normal,
                            move |_| {
                                let l = unpin.clone();
                                tell(&cmds, move |e| e.moderate(&l, Moderate::Unpin));
                            },
                        );
                        if let Some(mut item) = m.at(i) {
                            item.add_image(icons::before_label("pin-off", super::muted(), 14), true);
                        }
                        let pane = self.clone();
                        if pin.platform == "twitch" {
                            m.add(
                                tr("chat.pin_change_duration"),
                                fltk::enums::Shortcut::None,
                                MenuFlag::Normal,
                                move |_| {
                                    pane.run(&l, Act::Mod(Moderate::Pin(None)));
                                },
                            );
                        }
                    }
                    if m.size() > 0 {
                        m.popup();
                    }
                    MenuButton::delete(m);
                }
                true
            }
            Event::Push if super::right_button() => {
                let l = {
                    let st = self.st.borrow();
                    self.line_at(f, &st).and_then(|i| st.lines.get(i).cloned())
                };
                let Some(l) = l.filter(|l| !l.note) else { return true };
                use fltk::menu::{MenuButton, MenuFlag};
                let mut m = super::menu_at_pointer();
                for (act, icon, name) in acts(&l, self.on_reply.borrow().is_some()) {
                    let (p, l) = (self.clone(), l.clone());
                    let i = m.add(name, fltk::enums::Shortcut::None, MenuFlag::Normal, move |_| {
                        p.run(&l, act)
                    });
                    if let Some(mut item) = m.at(i) {
                        item.add_image(icons::before_label(icon, super::muted(), 14), true);
                    }
                }
                if !l.user_id.is_empty() && self.st.borrow().cards {
                    let (cmds, l) = (self.cmds.clone(), l.clone());
                    m.add(
                        tr("chat.user_info"),
                        fltk::enums::Shortcut::None,
                        MenuFlag::Normal,
                        move |_| {
                            super::card::open(
                                &cmds,
                                super::card::Who::from_line(&l, name_color(&l)),
                                (app::event_x(), app::event_y()),
                            )
                        },
                    );
                }
                if m.size() > 0 {
                    m.popup();
                }
                MenuButton::delete(m);
                true
            }
            Event::Push if super::left_button() => {
                if let Some((l, a)) = self.act_at(f) {
                    self.run(&l, a.0);
                } else if let Some(l) = self.name_at(f) {
                    super::card::open(
                        &self.cmds,
                        super::card::Who::from_line(&l, name_color(&l)),
                        (app::event_x(), app::event_y()),
                    );
                }
                true
            }
            Event::MouseWheel => {
                let mut st = self.st.borrow_mut();
                st.top += app::event_dy_value() * 3 * LINE;
                let max = (self.total(&st) - f.h()).max(0);
                st.top = st.top.clamp(0, max);
                st.follow = st.top >= max;
                self.settle(&mut st);
                drop(st);
                f.redraw();
                true
            }
            Event::Resize => {
                self.st.borrow_mut().width = -1;
                false
            }
            _ => false,
        }
    }

    fn on_pill(&self, f: &Frame) -> bool {
        let st = self.st.borrow();
        self.pill(f, &st).is_some_and(|(x, y, w, h)| {
            let (mx, my) = (app::event_x(), app::event_y());
            mx >= x && mx < x + w && my >= y && my < y + h
        })
    }

    /// Names don't open user cards (the pane is in one).
    pub(super) fn no_cards(&self) {
        self.st.borrow_mut().cards = false;
    }

    /// This pane's lines come and go over time (the chat's columns; a user card's lines stay).
    pub(super) fn ages(&self) {
        self.st.borrow_mut().ages = true;
    }

    /// The limits changed: what is over them goes now.
    fn apply_limits(&self) {
        let mut st = self.st.borrow_mut();
        self.trim(&mut st);
        self.pulse_soon(&mut st, 0);
        drop(st);
        self.area.clone().redraw();
    }

    pub(super) fn add(&self, l: &Line) {
        let mut st = self.st.borrow_mut();
        st.lines.push_back(l.clone());
        if !st.follow {
            st.unread += 1;
        }
        let born = if st.ages { epoch_ms() } else { 0 };
        st.born.push_back(born);
        if st.width > 0 && st.laid.len() + 1 == st.lines.len() {
            let laid = self.lay(l, st.tag, st.width);
            st.laid.push_back(laid);
        }
        self.trim(&mut st);
        self.pulse_soon(&mut st, 0);
        if st.ages {
            KEPT.store(st.lines.len(), std::sync::atomic::Ordering::Relaxed);
        }
        drop(st);
        self.area.clone().redraw();
    }

    pub(super) fn replace_all(&self, lines: impl Iterator<Item = Line>) {
        let mut st = self.st.borrow_mut();
        st.lines.clear();
        st.born.clear();
        st.hover = None;
        for l in lines {
            st.lines.push_back(l);
            st.born.push_back(0);
        }
        st.laid.clear();
        st.width = -1;
        self.trim(&mut st);
        self.pulse_soon(&mut st, 0);
        if st.ages {
            KEPT.store(st.lines.len(), std::sync::atomic::Ordering::Relaxed);
        }
        drop(st);
        self.area.clone().redraw();
    }

    fn clear(&self) {
        let mut st = self.st.borrow_mut();
        st.lines.clear();
        st.pins.clear();
        st.polls.clear();
        st.born.clear();
        st.laid.clear();
        st.hover = None;
        st.top = 0;
        st.follow = true;
        st.unread = 0;
        drop(st);
        self.area.clone().redraw();
    }

    /// The window was hidden: nothing is kept here (the engine has the lines; the pane fills again when
    /// the window is shown) and nothing ticks.
    pub(super) fn release(&self) {
        self.clear();
        let mut st = self.st.borrow_mut();
        if let Some((_, h)) = st.pulse.take() {
            app::remove_timeout3(h);
        }
        st.moving = Vec::new();
        st.lines.shrink_to_fit();
        st.laid.shrink_to_fit();
        st.born.shrink_to_fit();
        st.pins.shrink_to_fit();
        st.polls.shrink_to_fit();
    }

    /// Lines over the limits go (the oldest first), and those past their lifetime.
    fn trim(&self, st: &mut State) {
        let lim = LIMITS.get().clamped();
        let now = epoch_ms();
        let aged = |l: &Line| st_ages_out(l, now, &lim);
        let by_age: Vec<bool> = if st.ages && lim.lifetime > 0 {
            st.lines.iter().map(aged).collect()
        } else {
            Vec::new()
        };
        let mut gone = lim.excess(st.lines.iter().map(|l| PLATFORMS.iter().position(|p| *p == l.platform)));
        gone.extend(by_age.iter().enumerate().filter(|(_, g)| **g).map(|(i, _)| i));
        gone.sort_unstable();
        gone.dedup();
        if gone.is_empty() {
            return;
        }
        let in_step = st.laid.len() == st.lines.len();
        for at in gone.into_iter().rev() {
            st.lines.remove(at);
            st.born.remove(at);
            if in_step {
                st.laid.remove(at);
            }
            st.hover = match st.hover {
                Some(h) if h == at => None,
                Some(h) if h > at => Some(h - 1),
                h => h,
            };
        }
        if !in_step {
            // Lines laid out for others than these now: laid out again when drawn.
            st.laid.clear();
        }
    }

    /// Looks at the lines again `after` ms from now (sooner than a look already due replaces it).
    fn pulse_soon(&self, st: &mut State, after: u64) {
        if !st.ages {
            return;
        }
        let lim = LIMITS.get().clamped();
        let now = epoch_ms();
        // What is coming: a line fading in (now), or the first one to start leaving.
        let fading = st
            .lines
            .iter()
            .zip(&st.born)
            .any(|(l, b)| !l.local && opacity(l, *b, now, &lim) < 1.0);
        let at = if fading {
            now + after.max(50)
        } else if lim.lifetime > 0 {
            let first_out = st
                .lines
                .iter()
                .filter(|l| l.ts > 0)
                .map(|l| (l.ts as u64 + lim.lifetime * 1000).saturating_sub(u64::from(lim.fade_out)))
                .min();
            match first_out {
                Some(t) => t.max(now + 50),
                None => return,
            }
        } else {
            return;
        };
        if st.pulse.is_some_and(|(due, _)| due <= at) {
            return;
        }
        if let Some((_, h)) = st.pulse.take() {
            app::remove_timeout3(h);
        }
        let p = self.clone();
        let h = app::add_timeout3((at - now) as f64 / 1000.0, move |_| p.pulse());
        st.pulse = Some((at, h));
    }

    /// A moment to look at the lines: those past their lifetime go, and those coming or going are redrawn.
    fn pulse(&self) {
        if self.area.was_deleted() {
            return;
        }
        let mut st = self.st.borrow_mut();
        st.pulse = None;
        let before = st.lines.len();
        self.trim(&mut st);
        let lim = LIMITS.get().clamped();
        let now = epoch_ms();
        if st.lines.len() != before {
            KEPT.store(st.lines.len(), std::sync::atomic::Ordering::Relaxed);
            drop(st);
            self.area.clone().redraw();
            st = self.st.borrow_mut();
        } else if st.width > 0 && st.laid.len() == st.lines.len() {
            // Only the lines that are fading: each its whole width.
            let mut a = self.area.clone();
            let mut y = a.y() + 4 + self.head(&st) - st.top;
            for (i, (_, h)) in st.laid.iter().enumerate() {
                if y + h >= a.y() && y <= a.y() + a.h() {
                    let o = opacity(&st.lines[i], st.born[i], now, &lim);
                    if !st.lines[i].local && o < 1.0 {
                        a.set_damage_area(Damage::User1, a.x(), y, a.w(), *h);
                    }
                } else if y > a.y() + a.h() {
                    break;
                }
                y += h;
            }
        }
        self.pulse_soon(&mut st, 0);
    }

    /// Pictures came in: lines may be taller.
    fn remeasure(&self) {
        self.st.borrow_mut().width = -1;
        self.area.clone().redraw();
    }
}

/// Resolve the effective `(use_bot, as_broadcaster)` for `plat_idx` given the user's preference and
/// which accounts are actually logged in on that platform.
fn effective_sender_role(
    cfg: &Cfg,
    cmds: &Cmds,
    plat_idx: usize,
) -> (bool, Option<String>, Option<fltk::image::RgbImage>) {
    // TikTok sends as whoever is logged in to the browser with the extension.
    if PLATFORMS[plat_idx] == "tiktok" {
        if !crate::engine::can_send(&cfg.borrow(), "tiktok") {
            return (false, None, None);
        }
        let (ready, user) = query(cmds, |e| (e.tiktok_bridge.ready(), e.tiktok_bridge.user())).unwrap_or_default();
        let pic = super::accounts::browser_account(cmds, 22).and_then(|a| a.pic);
        return (false, (ready && !user.is_empty()).then_some(user), pic);
    }
    let want_broadcaster = cfg
        .borrow()
        .window
        .chat_as_broadcaster
        .iter()
        .any(|s| s == PLATFORMS[plat_idx]);
    let slots = platform_accounts(cmds, cfg, PLATFORMS[plat_idx], 22);
    // Pick preferred slot if logged in, otherwise fall back to the other slot if logged in.
    let use_bot = if want_broadcaster {
        slots[0].is_none() && slots[1].is_some()
    } else {
        slots[1].is_some() || slots[0].is_none()
    };
    let chosen = &slots[usize::from(use_bot)];
    let name = chosen.as_ref().map(|s| s.name.clone());
    let pic = chosen.as_ref().and_then(|s| s.pic.clone());
    (use_bot, name, pic)
}

/// Self-contained chat column: message `Pane` + individual bottom bar `[Account Avatar] [Input] [Emotes] [Send]`.
#[derive(Clone)]
struct ChatColumn {
    col: Flex,
    /// Which chat this is: the platform's logo, the channel's picture and its name.
    head: Flex,
    head_logo: Frame,
    head_pic: Frame,
    head_name: Frame,
    viewers: Frame,
    pane: Pane,
    chat_stack: Wizard,
    chat_group: Flex,
    down_group: Flex,
    acct_btn: Button,
    input: Input,
    target_plat: Rc<Cell<usize>>,
    /// The chat box and what it answers (hidden in the all-platforms view).
    bar: Flex,
    down_bar: Flex,
    read_only: Frame,
    modes: Frame,
    set_reply: SetReply,
    /// What is wrong with the chat (the stream ended, not connected), with what to do about it.
    notice: Flex,
    notice_icon: Frame,
    notice_title_row: Flex,
    notice_logo: Frame,
    notice_title: Frame,
    reconnect_btn: Button,
    hide_btn: Button,
    notice_plat: Rc<Cell<Option<&'static str>>>,
    notice_targets: Rc<RefCell<Vec<&'static str>>>,
    notice_group: Rc<Cell<u8>>,
    /// What the notice shows now, so it is only laid out again when that changes.
    notice_last: Rc<Cell<Shown>>,
}

impl ChatColumn {
    fn show_modes(&self, text: &str) {
        let mut frame = self.modes.clone();
        let changed = frame.label() != text;
        if changed {
            frame.set_label(text);
        }
        let height = if text.is_empty() {
            0
        } else {
            draw::set_font(frame.label_font(), frame.label_size());
            let width = frame.w().max(1);
            let (_, h) = draw::measure(text, true);
            // Allow space for wrapped text even before the first layout.
            let lines = (draw::width(text) / f64::from(width)).ceil() as i32;
            h.max(lines * 16).max(20)
        };
        if changed || frame.h() != height {
            let mut col = self.col.clone();
            if height == 0 {
                frame.hide();
            } else {
                frame.show();
            }
            col.fixed(&frame, height);
            col.layout();
        }
    }

    fn show_viewers(&self, count: Option<u64>, state: ChatState, ended: bool) {
        let mut frame = self.viewers.clone();
        let show = !ended && (state == ChatState::Down || (state == ChatState::Connected && count.is_some()));
        if frame.visible() != show {
            let mut head = self.head.clone();
            if show {
                frame.show();
            } else {
                frame.hide();
            }
            head.fixed(&frame, if show { 40 } else { 0 });
            head.layout();
        }
        let color = if state == ChatState::Down {
            Color::color_average(super::muted(), Color::Background2, 0.7)
        } else {
            super::muted()
        };
        if frame.label_color() != color {
            frame.set_label_color(color);
        }
        let label = if !ended && state == ChatState::Down && count.is_none() && frame.label().parse::<u64>().is_ok() {
            frame.label()
        } else {
            count.map_or_else(|| "?".into(), |n| n.to_string())
        };
        if frame.label() != label {
            frame.set_label(&label);
        }
        frame.set_tooltip(&format!(
            "{}: {}",
            tr("chat.viewers"),
            if label == "?" {
                tr("chat.viewers_unavailable").to_string()
            } else {
                label
            }
        ));
        frame.redraw();
    }
    fn new(cfg: &Cfg, cmds: &Cmds, fixed_plat: Option<usize>, go_accounts: Rc<dyn Fn()>) -> Self {
        let mut col = Flex::default().column();
        col.set_pad(4);

        let mut head = Flex::default().row();
        head.set_pad(6);
        let head_logo = Frame::default();
        head.fixed(&head_logo, 20);
        let head_pic = Frame::default();
        head.fixed(&head_pic, 22);
        let mut head_name = Frame::default().with_align(Align::Left | Align::Inside | Align::Clip);
        head_name.set_label_font(Font::HelveticaBold);
        head_name.set_label_type(fltk::enums::LabelType::None);
        head_name.draw(|f| {
            let label = f.label().replace("@@", "@");
            draw::set_font(f.label_font(), f.label_size());
            if draw::width(&label) <= f64::from(f.w().max(0)) {
                draw::set_draw_color(f.label_color());
                draw::draw_text2(
                    &label,
                    f.x(),
                    f.y(),
                    f.w(),
                    f.h(),
                    Align::Left | Align::Inside | Align::Clip,
                );
            }
        });
        let viewers = viewer_counter();
        head.fixed(&viewers, 40);
        head.end();
        col.fixed(&head, 24);

        let mut chat_stack = Wizard::default();
        let (chat_group, pane) = Pane::new(cmds);
        pane.ages();
        let mut down_group = Flex::default().column();
        down_group.set_frame(FrameType::DownBox);
        down_group.set_color(Color::Background2);
        let _empty_chat = Frame::default();
        down_group.end();
        chat_stack.end();
        chat_stack.resize_callback({
            let (mut chat, mut down) = (chat_group.clone(), down_group.clone());
            move |_, x, y, w, h| {
                chat.resize(x, y, w, h);
                chat.layout();
                down.resize(x, y, w, h);
                down.layout();
            }
        });
        chat_stack.set_current_widget(&chat_group);

        // "Replying to @name: text" with an X, while answering one.
        let mut replying = Flex::default().row();
        replying.set_pad(6);
        let mut r_icon = Frame::default();
        r_icon.set_image(icons::icon("corner-up-left", super::muted(), 14));
        replying.fixed(&r_icon, 18);
        let mut r_text = Frame::default()
            .with_align(Align::Left | Align::Inside | Align::Clip)
            .ellipsis();
        r_text.set_label_size(12);
        r_text.set_label_color(super::muted());
        let mut r_close = Button::default();
        r_close.set_frame(FrameType::FlatBox);
        r_close.set_image(icons::icon("x", super::muted(), 14));
        r_close.set_tooltip(tr("chat.dont_reply_tooltip"));
        replying.fixed(&r_close, 24);
        replying.end();
        col.fixed(&replying, 0);
        replying.hide();

        // A compact row for chat connection actions.
        let mut notice = Flex::default().row();
        notice.set_pad(3);
        notice.set_margin(3);
        notice.set_frame(FrameType::RFlatBox);
        notice.set_color(Color::Background2);
        let mut notice_title_row = Flex::default().row();
        let notice_icon = Frame::default();
        notice_title_row.fixed(&notice_icon, 26);
        let notice_logo = Frame::default();
        notice_title_row.fixed(&notice_logo, 20);
        let mut notice_title = Frame::default()
            .with_align(Align::Left | Align::Inside | Align::Clip)
            .ellipsis();
        notice_title.set_label_font(Font::HelveticaBold);
        notice_title.set_label_size(13);
        notice_title_row.end();
        let mut btn_row = Flex::default().row();
        btn_row.set_pad(4);
        let mut reconnect_btn = Button::default().with_label(tr("chat.reconnect"));
        let mut hide_btn = Button::default().with_label(tr("chat.hide_chat"));
        reconnect_btn.set_label("");
        reconnect_btn.set_image(icons::icon("refresh-cw", super::button_ink(), 16));
        reconnect_btn.set_align(Align::Center | Align::Inside | Align::ImageNextToText);
        reconnect_btn.set_tooltip(tr("chat.reconnect"));
        hide_btn.set_label("");
        hide_btn.set_image(icons::icon("eye-off", super::button_ink(), 16));
        hide_btn.set_align(Align::Center | Align::Inside | Align::ImageNextToText);
        hide_btn.set_tooltip(tr("chat.hide_chat"));
        btn_row.end();
        notice.fixed(&btn_row, 52);
        notice.end();
        col.fixed(&notice, 0);
        notice.hide();

        let mut modes = Frame::default().with_align(Align::Left | Align::Inside | Align::Wrap);
        modes.set_label_size(12);
        modes.set_label_color(super::muted());
        col.fixed(&modes, 0);
        modes.hide();

        let mut bar = Flex::default().row();
        bar.set_pad(4);

        let mut acct_btn = Button::default();
        acct_btn.set_frame(FrameType::FlatBox);
        acct_btn.set_image(icons::icon("circle-user-round", super::muted(), 20));
        bar.fixed(&acct_btn, 32);

        let mut input = Input::default();

        let mut emote_btn = Button::default();
        emote_btn.set_frame(FrameType::FlatBox);
        emote_btn.set_image(icons::icon("smile", super::muted(), 18));
        emote_btn.set_tooltip(tr("chat.emote_btn_tooltip"));
        bar.fixed(&emote_btn, 32);

        let mut send_btn = Button::default();
        send_btn.set_frame(FrameType::FlatBox);
        send_btn.set_image(icons::icon("send-horizontal", Color::Selection, 18));
        send_btn.set_tooltip(tr("chat.send_btn_tooltip"));
        bar.fixed(&send_btn, 32);

        bar.end();
        col.fixed(&bar, ROW + 4);
        let mut read_only = Frame::default().with_label(tr("chat.tiktok_read_only"));
        read_only.set_label_color(super::muted());
        read_only.set_tooltip(tr("chat.tiktok_read_only"));
        read_only.set_label_type(fltk::enums::LabelType::None);
        read_only.draw(|f| {
            let text = f.label();
            draw::set_font(f.label_font(), f.label_size());
            let fits = draw::width(&text) + 26.0 <= f64::from(f.w());
            let x = if fits {
                f.x() + 4
            } else {
                f.x() + (f.w() - 16).max(0) / 2
            };
            if let Some(mut icon) = icons::icon("ban", Color::from_rgb(0xf0, 0x6a, 0x6a), 16) {
                icon.draw(x, f.y() + (f.h() - 16) / 2, 16, 16);
            }
            if fits {
                draw::set_draw_color(f.label_color());
                draw::draw_text2(
                    &text,
                    x + 22,
                    f.y(),
                    f.w() - 26,
                    f.h(),
                    Align::Left | Align::Inside | Align::Clip,
                );
            }
        });
        col.fixed(&read_only, 0);
        read_only.hide();
        let mut down_row = Flex::default().row();
        down_row.set_pad(2);
        down_row.set_margin(2);
        let mut down_label = Frame::default().with_label(tr("chat.disconnected"));
        down_label.set_label_size(10);
        down_label.set_label_color(super::muted());
        down_label.set_align(Align::Right | Align::Inside | Align::Clip);
        let mut down_reconnect = Button::default();
        down_reconnect.set_image(icons::icon("refresh-cw", super::button_ink(), 16));
        down_reconnect.set_align(Align::Center | Align::Inside | Align::ImageNextToText);
        down_reconnect.set_tooltip(tr("chat.reconnect"));
        down_row.fixed(&down_reconnect, 22);
        let mut down_hide = Button::default();
        down_hide.set_image(icons::icon("eye-off", super::button_ink(), 16));
        down_hide.set_align(Align::Center | Align::Inside | Align::ImageNextToText);
        down_hide.set_tooltip(tr("chat.hide_chat"));
        down_row.fixed(&down_hide, 22);
        down_row.end();
        down_row.resize_callback({
            let (mut label, mut reconnect, mut hide) = (down_label.clone(), down_reconnect.clone(), down_hide.clone());
            move |_, x, y, w, h| {
                draw::set_font(Font::Helvetica, 10);
                let label_w = (draw::width(&label.label()).ceil() as i32 + 8).min((w - 52).max(0));
                let left = x + (w - label_w - 48).max(0) / 2;
                label.resize(left, y, label_w, h);
                reconnect.resize(left + label_w + 2, y + (h - 22) / 2, 22, 22);
                hide.resize(left + label_w + 26, y + (h - 22) / 2, 22, 22);
            }
        });
        col.fixed(&down_row, 0);
        down_row.hide();
        col.end();

        let init_idx = fixed_plat.unwrap_or_else(|| {
            let saved = cfg.borrow().window.chat_all_platform.clone();
            PLATFORMS.iter().position(|p| *p == saved).unwrap_or(0)
        });
        let target_plat = Rc::new(Cell::new(init_idx));
        let reply: Rc<RefCell<Option<Line>>> = Rc::default();
        let click = emote_field(&mut input, cmds, target_plat.clone(), tr("chat.send_placeholder"));
        // Answer `l` (None: stop); in the all-platforms box, to `l`'s platform.
        let set_reply: SetReply = {
            let (reply, col, replying, r_text) = (reply.clone(), col.clone(), replying.clone(), r_text.clone());
            let input = input.clone();
            Rc::new(move |l: Option<&Line>| {
                let (mut col, mut replying, mut r_text, mut input) =
                    (col.clone(), replying.clone(), r_text.clone(), input.clone());
                *reply.borrow_mut() = l.cloned();
                match l {
                    Some(l) => {
                        let said = l.text.replace('@', "@@");
                        r_text.set_label(&trf(
                            "chat.replying_to",
                            &[("user", &l.user.replace('@', "@@")), ("said", &said)],
                        ));
                        replying.show();
                        col.fixed(&replying, 22);
                        let _ = input.take_focus();
                    }
                    None => {
                        replying.hide();
                        col.fixed(&replying, 0);
                    }
                }
                col.layout();
                col.redraw();
            })
        };
        pane.on_reply.replace(Some({
            let set_reply = set_reply.clone();
            Rc::new(move |l: &Line| set_reply(Some(l)))
        }));
        r_close.set_callback({
            let set_reply = set_reply.clone();
            move |_| set_reply(None)
        });

        let me = Self {
            col,
            head,
            head_logo,
            head_pic,
            head_name,
            viewers,
            pane,
            chat_stack,
            chat_group,
            down_group,
            acct_btn: acct_btn.clone(),
            input: input.clone(),
            target_plat: target_plat.clone(),
            bar: bar.clone(),
            down_bar: down_row,
            read_only,
            modes,
            set_reply: set_reply.clone(),
            notice: notice.clone(),
            notice_icon: notice_icon.clone(),
            notice_title_row: notice_title_row.clone(),
            notice_logo: notice_logo.clone(),
            notice_title: notice_title.clone(),
            reconnect_btn: reconnect_btn.clone(),
            hide_btn: hide_btn.clone(),
            notice_plat: Rc::default(),
            notice_targets: Rc::default(),
            notice_group: Rc::default(),
            notice_last: Rc::default(),
        };
        reconnect_btn.set_callback({
            let (cmds, at) = (cmds.clone(), me.notice_targets.clone());
            move |_| {
                let targets = at.borrow().clone();
                tell(&cmds, move |e| {
                    for p in targets {
                        e.chat_reconnect(p);
                    }
                });
            }
        });
        down_reconnect.set_callback({
            let (cmds, at) = (cmds.clone(), me.notice_targets.clone());
            move |_| {
                let targets = at.borrow().clone();
                tell(&cmds, move |e| {
                    for p in targets {
                        e.chat_reconnect(p);
                    }
                });
            }
        });
        down_hide.set_callback({
            let (cfg, at) = (cfg.clone(), me.notice_targets.clone());
            move |_| {
                for p in at.borrow().iter().copied() {
                    cfg.send_modify(|c| {
                        if !c.window.chat_hidden.iter().any(|h| h == p) {
                            c.window.chat_hidden.push(p.to_string());
                        }
                    });
                    crate::ui::changed();
                }
            }
        });
        hide_btn.set_callback({
            let (cfg, at) = (cfg.clone(), me.notice_targets.clone());
            move |_| {
                for p in at.borrow().iter().copied() {
                    cfg.send_modify(|c| {
                        if !c.window.chat_hidden.iter().any(|h| h == p) {
                            c.window.chat_hidden.push(p.to_string());
                        }
                    });
                    crate::ui::changed();
                }
            }
        });

        let do_send = {
            let (cfg, cmds, input, target_plat) = (cfg.clone(), cmds.clone(), input.clone(), target_plat.clone());
            let (reply, set_reply) = (reply.clone(), set_reply.clone());
            move || {
                let text = input.value().trim().to_string();
                if text.is_empty() {
                    return;
                }
                let p_idx = target_plat.get().min(3);
                let plat = PLATFORMS[p_idx];
                if plat == "tiktok" {
                    if let Err(error) = crate::tiktok_bridge::validate_message(&text) {
                        super::dialogs::message(&error);
                        return;
                    }
                }
                if !crate::engine::can_send(&cfg.borrow(), plat)
                    || !query(&cmds, move |e| e.chat_connected(plat)).unwrap_or(false)
                {
                    return;
                }
                let want_broadcaster = cfg.borrow().window.chat_as_broadcaster.iter().any(|s| s == plat);
                let use_bot = !want_broadcaster;
                let to = reply.borrow_mut().take();
                tell(&cmds, move |e| {
                    if e.chat_connected(plat) {
                        let _ = e.send_chat_list(&[plat], &text, use_bot, true, to.as_ref());
                    }
                });
                set_reply(None);
                input.clone().set_value("");
            }
        };

        send_btn.set_callback({
            let s = do_send.clone();
            move |_| s()
        });
        let (me_reply, set_reply_esc) = (reply.clone(), set_reply.clone());
        let reply_on = |r: &Rc<RefCell<Option<Line>>>| r.borrow().is_some();
        let suggest = super::suggest::Suggest::default();
        me.input.clone().handle({
            let s = do_send;
            let target_plat = target_plat.clone();
            let cmds = cmds.clone();
            move |i, ev| {
                let plat = PLATFORMS[target_plat.get().min(3)];
                // Twitch's and Kick's chat commands, listed as they are typed.
                if matches!(ev, Event::KeyDown | Event::KeyUp | Event::Unfocus) {
                    let users = if ev == Event::KeyUp && i.value().contains('@') {
                        query(&cmds, move |e| e.chatters(plat)).unwrap_or_default()
                    } else {
                        Vec::new()
                    };
                    match ev {
                        Event::KeyDown if suggest.key(i, plat) => return true,
                        Event::KeyUp
                            if !matches!(
                                app::event_key(),
                                Key::Escape | Key::Tab | Key::Up | Key::Down | Key::Enter | Key::ShiftL | Key::ShiftR
                            ) =>
                        {
                            suggest.update(i, plat, &users);
                        }
                        Event::Unfocus => suggest.hide(),
                        _ => {}
                    }
                } else if suggest.is_open() {
                    suggest.hide();
                }
                if click(i, ev) {
                    return true;
                }
                // Enter, the numpad's too, or whatever sends a carriage return (some Wayland setups).
                let enter =
                    app::event_key() == Key::Enter || app::event_key().bits() == 0xff8d || app::event_text() == "\r";
                if ev == Event::KeyDown && enter {
                    s();
                    return true;
                }
                if ev == Event::KeyDown && app::event_key() == Key::Escape && reply_on(&me_reply) {
                    set_reply_esc(None);
                    return true;
                }
                false
            }
        });

        acct_btn.set_callback({
            let me_c = me.clone();
            let (cfg, cmds, go_accounts) = (cfg.clone(), cmds.clone(), go_accounts.clone());
            move |_| {
                account_selector_popup(&cfg, &cmds, &me_c, go_accounts.clone());
                me_c.refresh_sender(&cfg, &cmds);
            }
        });

        emote_btn.set_callback({
            let me_c = me.clone();
            let (cfg, cmds) = (cfg.clone(), cmds.clone());
            let mut input_c = me.input.clone();
            move |_| {
                let p_idx = me_c.target_plat.get().min(3);
                let plat = PLATFORMS[p_idx];
                let (use_bot, sender_name, _) = effective_sender_role(&cfg, &cmds, p_idx);
                emote_picker_popup(&cfg, &cmds, plat, !use_bot, sender_name.as_deref(), &mut input_c);
            }
        });

        me
    }

    /// Shows what is wrong with `platform`'s chat (`state`), or takes the notice away.
    fn show_notice(&self, platform: Option<&'static str>, state: ChatState) {
        self.notice_targets
            .replace(platform.filter(|_| state != ChatState::Connected).into_iter().collect());
        let mut stack = self.chat_stack.clone();
        if self.pane.disconnected.get() {
            if stack.index() != Some(1) {
                stack.set_current_widget(&self.down_group);
            }
        } else if stack.index() != Some(0) {
            stack.set_current_widget(&self.chat_group);
        }
        let now = Some((platform.filter(|_| state != ChatState::Connected), state));
        if self.notice_last.get() == now
            || (state == ChatState::Connected && self.notice_last.get().is_none() && !self.notice.visible())
        {
            return;
        }
        self.notice_last.set(now);
        self.notice_group.set(0);
        let mut notice = self.notice.clone();
        let mut col = self.col.clone();
        self.notice_plat.set(platform.filter(|_| state != ChatState::Connected));
        if self.pane.disconnected.get() {
            if notice.visible() {
                notice.hide();
                col.fixed(&notice, 0);
                col.layout();
            }
            return;
        }
        let Some(p) = self.notice_plat.get() else {
            if notice.visible() {
                notice.hide();
                col.fixed(&notice, 0);
                col.layout();
            }
            return;
        };
        let name = NAMES[PLATFORMS.iter().position(|x| *x == p).unwrap_or(0)];
        let title_key = "chat.state_down_title";
        let mut title = self.notice_title.clone();
        let mut title_row = self.notice_title_row.clone();
        let mut logo = self.notice_logo.clone();
        if state == ChatState::Down {
            title.set_label(tr("chat.disconnected"));
            logo.set_image(img::logo(name, 14));
            logo.show();
            title_row.fixed(&logo, 20);
            title.set_tooltip(name);
        } else {
            logo.hide();
            title_row.fixed(&logo, 0);
            title.set_label(&trf(title_key, &[("platform", name)]));
            title.set_tooltip("");
        }
        title_row.layout();
        title.set_label_color(Color::Foreground);
        let mut ic = self.notice_icon.clone();
        ic.set_image(icons::icon(
            if state == ChatState::Down { "unplug" } else { "info" },
            super::muted(),
            22,
        ));
        let mut re = self.reconnect_btn.clone();
        re.show();
        if !notice.visible() {
            notice.show();
        }
        col.fixed(&notice, 32);
        col.layout();
        notice.redraw();
    }

    /// Group disconnected platforms in the combined view, including both actions.
    fn show_notice_group(&self, platforms: Vec<&'static str>) {
        let mask = platforms.iter().fold(0u8, |m, p| {
            m | PLATFORMS.iter().position(|x| x == p).map_or(0, |i| 1 << i)
        });
        self.notice_targets.replace(platforms.clone());
        if self.notice_group.replace(mask) == mask {
            return;
        }
        let mut logo = self.notice_logo.clone();
        let mut row = self.notice_title_row.clone();
        logo.set_image(img::platform_logos(&platforms, 14));
        row.fixed(&logo, (platforms.len() as i32 * 18).max(0));
        row.layout();
        self.notice_title.clone().set_tooltip(
            &platforms
                .iter()
                .map(|p| img::platform_name(p))
                .collect::<Vec<_>>()
                .join(", "),
        );
        self.reconnect_btn.clone().set_tooltip(if platforms.len() > 1 {
            tr("chat.reconnect_all")
        } else {
            tr("chat.reconnect")
        });
        self.hide_btn.clone().set_tooltip(if platforms.len() > 1 {
            tr("chat.hide_chats")
        } else {
            tr("chat.hide_chat")
        });
        self.notice.clone().redraw();
    }

    fn refresh_sender(&self, cfg: &Cfg, cmds: &Cmds) {
        let p_idx = self.target_plat.get().min(3);
        let plat_name = NAMES[p_idx];
        let (mut logo, mut face, mut name) = (self.head_logo.clone(), self.head_pic.clone(), self.head_name.clone());
        let (room_name, room_pic) = room(cmds, cfg, PLATFORMS[p_idx], 22);
        logo.set_image(img::logo(plat_name, 18));
        if let Some(pic) = room_pic {
            face.set_image(Some(pic));
        } else {
            face.set_image(icons::icon("circle-user-round", super::muted(), 18));
        }
        let shown = if room_name.is_empty() {
            plat_name.to_string()
        } else {
            format!("{plat_name}: {room_name}")
        };
        name.set_label(&shown.replace('@', "@@"));
        name.set_tooltip(&shown);
        let (use_bot, sender_name, pic) = effective_sender_role(cfg, cmds, p_idx);
        let mut btn = self.acct_btn.clone();
        match pic {
            Some(img) => btn.set_image(Some(img)),
            None => btn.set_image(icons::icon("circle-user-round", super::muted(), 20)),
        }
        let role_str = if use_bot {
            tr("chat.role_bot")
        } else {
            tr("chat.role_broadcaster")
        };
        let plat = PLATFORMS[p_idx];
        let connected = query(cmds, move |e| e.chat_connected(plat)).unwrap_or(false);
        let (browser_ready, browser_connected, browser_user, browser_error) = if plat == "tiktok" {
            query(cmds, |e| {
                (
                    e.tiktok_bridge.ready(),
                    e.tiktok_bridge.is_connected(),
                    e.tiktok_bridge.user(),
                    e.tiktok_bridge.error(),
                )
            })
            .unwrap_or_default()
        } else {
            (true, true, String::new(), String::new())
        };
        let browser_hint = if !browser_error.is_empty() {
            browser_error
        } else if !browser_connected {
            tr("accounts.tiktok_sending.waiting").to_string()
        } else if !browser_ready {
            tr("accounts.tiktok_sending.loading").to_string()
        } else if browser_user.is_empty() {
            tr("accounts.tiktok_sending.login_needed").to_string()
        } else {
            String::new()
        };
        let can_send_plat = crate::engine::can_send(&cfg.borrow(), plat) && connected && browser_ready;
        let mut bar = self.bar.clone();
        let mut space = self.down_bar.clone();
        let mut read_only = self.read_only.clone();
        // TikTok reads without a login; sending needs the browser set up (Accounts > TikTok).
        let show_read_only = connected && plat == "tiktok" && !crate::engine::can_send(&cfg.borrow(), plat);
        let show_bar = connected && !show_read_only;
        if show_bar {
            bar.show();
        } else {
            bar.hide();
        }
        if show_read_only {
            read_only.show();
        } else {
            read_only.hide();
        }
        if connected {
            space.hide();
        } else {
            space.show();
        }
        let mut col = self.col.clone();
        col.fixed(&bar, if show_bar { ROW + 4 } else { 0 });
        col.fixed(&read_only, if show_read_only { ROW + 4 } else { 0 });
        col.fixed(&space, if connected { 0 } else { ROW + 4 });
        col.layout();
        let tip = if show_read_only {
            tr("chat.tiktok_read_only_tip").to_string()
        } else if plat == "tiktok" && !browser_hint.is_empty() {
            browser_hint.clone()
        } else if plat == "tiktok" {
            let name = sender_name.clone().unwrap_or_default();
            trf("chat.tiktok_sending_as_tip", &[("name", &name.replace('@', "@@"))])
        } else {
            match &sender_name {
                Some(name) => trf(
                    "chat.sending_as_tip",
                    &[
                        ("platform", plat_name),
                        ("name", &name.replace('@', "@@")),
                        ("role", role_str),
                    ],
                ),
                None => trf("chat.nobody_logged_in_tip", &[("platform", plat_name)]),
            }
        };
        btn.set_tooltip(&tip);
        // Nobody logged in or platform read-only: the box and its buttons are off.
        for i in 1..self.bar.children() {
            if let Some(mut w) = self.bar.child(i) {
                if can_send_plat && sender_name.is_some() {
                    w.activate();
                } else {
                    w.deactivate();
                }
            }
        }
        let mut inp = self.input.clone();
        let placeholder_tip = if show_read_only {
            tr("chat.tiktok_read_only").to_string()
        } else if plat == "tiktok" && !browser_hint.is_empty() {
            browser_hint.clone()
        } else if plat == "tiktok" {
            let name = sender_name.clone().unwrap_or_default();
            trf("chat.tiktok_sending_as_tip", &[("name", &name.replace('@', "@@"))])
        } else {
            match &sender_name {
                Some(name) => trf(
                    "chat.message_as_tip",
                    &[
                        ("platform", plat_name),
                        ("name", &name.replace('@', "@@")),
                        ("role", role_str),
                    ],
                ),
                None => trf("chat.message_plat_tip", &[("platform", plat_name)]),
            }
        };
        inp.set_tooltip(&placeholder_tip);
        btn.redraw();
        inp.redraw();
    }
}

pub struct Chat {
    pub tab: Flex,
    cfg: Cfg,
    cmds: Cmds,
    view: Choice,
    viewers: Frame,
    /// The set-up platforms (indexes in PLATFORMS), as the view list shows them.
    plats: Rc<RefCell<Vec<usize>>>,
    /// All together / single platform column, plus one individual column per platform for side by side.
    main: ChatColumn,
    side: Vec<ChatColumn>,
    client_gen: u64,
    seen: u64,
    combined_connections: [bool; 4],
}

/// The last chat line the page took in (its `seq`), for dev mode's `chat` command.
pub static SHOWN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// How many lines the chat's panes hold (the last to change), for dev mode's `chatlines` command.
pub static KEPT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

impl Chat {
    /// Sync new lines (or full history resync when pre-join history / badges updated) and refresh avatars.
    pub fn refresh(&mut self) {
        emotes::load_for_config(&self.cmds, &self.cfg);
        let mut limits = self.cfg.borrow().window.chat.clamped();
        if crate::memory_saver() {
            limits.save_history = false;
        }
        // Disable each composer, including its emote button, when chat is disabled.
        for column in std::iter::once(&self.main).chain(self.side.iter()) {
            let mut bar = column.bar.clone();
            if limits.save_history {
                bar.activate();
            } else {
                bar.deactivate();
            }
        }
        if LIMITS.get() != limits {
            LIMITS.set(limits);
            self.main.pane.apply_limits();
            for p in &self.side {
                p.pane.apply_limits();
            }
        }
        let mut me = my_names(&self.cfg.borrow());
        if let Some(user) = query(&self.cmds, |e| e.tiktok_bridge.user()).filter(|u| !u.is_empty()) {
            me.push(user.to_lowercase());
        }
        if ME.with_borrow(|m| *m != me) {
            ME.set(me);
            self.main.pane.remeasure();
            for p in &self.side {
                p.pane.remeasure();
            }
        }
        let plats = shown_platforms(&self.cmds);
        if *self.plats.borrow() != plats {
            *self.plats.borrow_mut() = plats;
            self.lists();
            self.rebuild(true);
            return;
        }
        self.refresh_senders();
        let connected = if self.view.value() == 0 {
            query(&self.cmds, |e| PLATFORMS.map(|p| e.chat_connected(p))).unwrap_or([false; 4])
        } else {
            [true; 4]
        };
        if self.view.value() == 0 && self.combined_connections != connected {
            self.combined_connections = connected;
            self.client_gen = u64::MAX;
            self.seen = 0;
        }
        let (cgen, seen) = (self.client_gen, self.seen);
        let Some((new_gen, full, lines)) = query(&self.cmds, move |e| e.chat_sync(cgen, seen)) else {
            return;
        };
        self.client_gen = new_gen;
        if let Some(l) = lines.last() {
            SHOWN.store(l.seq, std::sync::atomic::Ordering::Relaxed);
        }
        let v = self.view.value();
        self.show_pins(v);
        let hidden = self.cfg.borrow().window.chat_hidden.clone();
        let last_seq = lines.last().map_or(self.seen, |l| l.seq);
        let lines: Vec<Line> = lines
            .into_iter()
            .filter(|l| {
                !hidden.contains(&l.platform)
                    && (v != 0
                        || PLATFORMS
                            .iter()
                            .position(|p| *p == l.platform)
                            .is_none_or(|i| connected[i]))
            })
            .collect();
        if full {
            self.seen = last_seq;
            match v {
                1 => {
                    for (i, col) in self.side.iter().enumerate() {
                        let p = PLATFORMS[i];
                        col.pane.replace_all(lines.iter().filter(|l| l.platform == p).cloned());
                    }
                }
                0 => self.main.pane.replace_all(lines.into_iter()),
                _ => {
                    let want = self.shown().unwrap_or("");
                    self.main
                        .pane
                        .replace_all(lines.into_iter().filter(|l| l.platform == want));
                }
            }
            return;
        }
        for l in &lines {
            match v {
                1 => {
                    if let Some(p) = PLATFORMS.iter().position(|x| *x == l.platform) {
                        self.side[p].pane.add(l);
                    }
                }
                0 => self.main.pane.add(l),
                _ if self.shown() == Some(l.platform.as_str()) => self.main.pane.add(l),
                _ => {}
            }
            self.seen = l.seq;
        }
        self.seen = last_seq;
    }

    /// Each pane gets the pins of the chats it shows (Twitch is asked, now and then, for pins made on
    /// its site).
    fn show_pins(&self, v: i32) {
        tell(&self.cmds, |e| e.twitch_check_pin());
        let pins = query(&self.cmds, |e| e.chat_pins()).unwrap_or_default();
        let polls = query(&self.cmds, |e| e.chat_polls()).unwrap_or_default();
        // Not a hidden chat's.
        let hidden = self.cfg.borrow().window.chat_hidden.clone();
        let pins: Vec<Pin> = pins.into_iter().filter(|p| !hidden.contains(&p.platform)).collect();
        let polls: Vec<Poll> = polls.into_iter().filter(|p| !hidden.contains(&p.platform)).collect();
        let of = |p: &str| pins.iter().filter(|x| x.platform == p).cloned().collect::<Vec<_>>();
        let polls_of = |p: &str| polls.iter().filter(|x| x.platform == p).cloned().collect::<Vec<_>>();
        match v {
            1 => {
                for (i, col) in self.side.iter().enumerate() {
                    col.pane.set_pins(of(PLATFORMS[i]));
                    col.pane.set_polls(polls_of(PLATFORMS[i]));
                }
            }
            0 => {
                let connected = self.combined_connections;
                self.main.pane.set_pins(
                    pins.into_iter()
                        .filter(|p| {
                            PLATFORMS
                                .iter()
                                .position(|x| *x == p.platform)
                                .is_none_or(|i| connected[i])
                        })
                        .collect(),
                );
                self.main.pane.set_polls(
                    polls
                        .into_iter()
                        .filter(|p| {
                            PLATFORMS
                                .iter()
                                .position(|x| *x == p.platform)
                                .is_none_or(|i| connected[i])
                        })
                        .collect(),
                );
            }
            _ => {
                self.main.pane.set_pins(self.shown().map(of).unwrap_or_default());
                self.main.pane.set_polls(self.shown().map(polls_of).unwrap_or_default());
            }
        }
    }

    /// Each column follows the connection state of the chat it shows.
    fn refresh_notices(&self) {
        let states: Vec<ChatState> =
            query(&self.cmds, |e| PLATFORMS.map(|p| e.chat_state(p)).to_vec()).unwrap_or_default();
        let state = |i: usize| states.get(i).copied().unwrap_or(ChatState::Connected);
        if self.view.value() == 1 {
            for (i, col) in self.side.iter().enumerate() {
                col.pane.set_disconnected(state(i) == ChatState::Down);
                col.show_notice(Some(PLATFORMS[i]), state(i));
            }
            self.main.pane.set_disconnected(false);
            self.main.show_notice(None, ChatState::Connected);
            return;
        }
        let shown: Vec<usize> = match self.shown().and_then(|s| PLATFORMS.iter().position(|p| *p == s)) {
            Some(i) => vec![i],
            None => self.plats.borrow().clone(),
        };
        let none = self.shown().is_none() && shown.is_empty();
        self.main.pane.set_nothing(none);
        self.main.pane.set_disconnected(
            none || self
                .shown()
                .and_then(|p| PLATFORMS.iter().position(|x| *x == p))
                .is_some_and(|i| state(i) == ChatState::Down),
        );
        let disconnected: Vec<&'static str> = shown
            .iter()
            .copied()
            .filter(|i| state(*i) == ChatState::Down)
            .map(|i| PLATFORMS[i])
            .collect();
        match disconnected.first() {
            Some(p) => {
                self.main.show_notice(Some(*p), ChatState::Down);
                if self.shown().is_none() {
                    self.main.show_notice_group(disconnected);
                }
            }
            None => self.main.show_notice(None, ChatState::Connected),
        }
    }

    fn refresh_modes(&self) {
        let (modes, connected) = query(&self.cmds, |e| (e.chat_modes(), PLATFORMS.map(|p| e.chat_connected(p))))
            .unwrap_or(([None; 2], [false; 4]));
        let hidden = self.cfg.borrow().window.chat_hidden.clone();
        let label = |platform: &str| -> String {
            let Some(i) = ["twitch", "kick"].iter().position(|p| *p == platform) else {
                return String::new();
            };
            let Some(m) = modes[i] else { return String::new() };
            let Some(p) = PLATFORMS.iter().position(|p| *p == platform) else {
                return String::new();
            };
            if !connected[p] || hidden.contains(&platform.to_string()) {
                return String::new();
            }
            let mut labels = Vec::new();
            if m.emote_only {
                labels.push(tr("chat.emote_only").to_string());
            }
            if let Some(seconds) = m.slow_seconds {
                labels.push(trf("chat.slow_mode_seconds", &[("seconds", &seconds.to_string())]));
            }
            if labels.is_empty() {
                String::new()
            } else {
                format!("{}: {}", NAMES[p], labels.join(", "))
            }
        };
        if self.view.value() == 1 {
            for (i, col) in self.side.iter().enumerate() {
                col.show_modes(&label(PLATFORMS[i]));
            }
        } else if let Some(platform) = self.shown() {
            self.main.show_modes(&label(platform));
        } else {
            let labels: Vec<_> = self
                .plats
                .borrow()
                .iter()
                .map(|i| label(PLATFORMS[*i]))
                .filter(|s| !s.is_empty())
                .collect();
            self.main.show_modes(&labels.join("  "));
        }
    }

    fn refresh_senders(&self) {
        self.refresh_notices();
        self.refresh_modes();
        let (counts, states, ended) = query(&self.cmds, |e| {
            (
                e.chat_viewers(),
                PLATFORMS.map(|p| e.chat_state(p)),
                PLATFORMS.map(|p| e.chat_stream_ended(p)),
            )
        })
        .unwrap_or(([None; 4], [ChatState::Down; 4], [false; 4]));
        let shown = self.plats.borrow();
        let format_count = |i: usize| counts[i].map_or_else(|| "?".to_string(), |n| n.to_string());
        for (i, col) in self.side.iter().enumerate() {
            col.show_viewers(counts[i], states[i], ended[i]);
        }
        if let Some(i) = self.shown().and_then(|s| PLATFORMS.iter().position(|p| *p == s)) {
            self.main.show_viewers(counts[i], states[i], ended[i]);
        }
        let mut total = self.viewers.clone();
        let known: Vec<_> = shown.iter().filter_map(|i| counts[*i]).collect();
        let label = if known.is_empty() {
            "?".to_string()
        } else if known.len() < shown.len() {
            format!("{}+", known.iter().sum::<u64>())
        } else {
            known.iter().sum::<u64>().to_string()
        };
        total.set_label(&label);
        total.set_tooltip(
            &shown
                .iter()
                .map(|i| format!("{}: {}", NAMES[*i], format_count(*i)))
                .collect::<Vec<_>>()
                .join("\n"),
        );
        if self.view.value() == 0 && shown.iter().any(|i| !ended[*i]) {
            let color = if shown.iter().all(|i| states[*i] != ChatState::Connected) {
                Color::color_average(super::muted(), Color::Background2, 0.7)
            } else {
                super::muted()
            };
            if total.label_color() != color {
                total.set_label_color(color);
            }
            total.show();
        } else {
            total.hide();
        }
        drop(shown);
        if self.view.value() == 1 {
            for col in &self.side {
                col.refresh_sender(&self.cfg, &self.cmds);
            }
        } else if let Some(i) = self.shown().and_then(|s| PLATFORMS.iter().position(|p| *p == s)) {
            self.main.target_plat.set(i);
            self.main.refresh_sender(&self.cfg, &self.cmds);
        }
    }

    /// The window was hidden: the panes keep nothing, and fill again from the engine when it is shown.
    pub fn release(&mut self) {
        self.main.pane.release();
        for p in &self.side {
            p.pane.release();
        }
        self.client_gen = 0;
        self.seen = 0;
    }

    /// Pictures came in (emotes, badges, or sender avatars).
    pub fn images(&self) {
        self.main.pane.remeasure();
        for p in &self.side {
            p.pane.remeasure();
        }
        self.refresh_senders();
    }

    /// The one platform the view shows, if it shows one.
    fn shown(&self) -> Option<&'static str> {
        let v = self.view.value();
        (v >= 2)
            .then(|| self.plats.borrow().get((v - 2) as usize).map(|i| PLATFORMS[*i]))
            .flatten()
    }

    /// Rebuild the view dropdown for the set-up platforms, preserving current selection when possible.
    fn lists(&mut self) {
        let plats = self.plats.borrow();
        let names: Vec<&str> = plats.iter().map(|i| NAMES[*i]).collect();
        let cur = self.view.value();
        let view = if cur < 2 {
            cur.max(0)
        } else {
            self.view
                .choice()
                .and_then(|n| names.iter().position(|x| *x == n))
                .map_or(0, |i| i as i32 + 2)
        };
        self.view.clear();
        self.view.add_choice(
            &[tr("chat.view_all"), tr("chat.view_side_by_side")]
                .iter()
                .chain(&names)
                .copied()
                .collect::<Vec<_>>()
                .join("|"),
        );
        self.view.set_value(view);
    }

    /// Lay the view out again, and fill it (only while the window is up: filling loads the emotes).
    fn rebuild(&mut self, fill: bool) {
        let side = self.view.value() == 1;
        self.main.pane.clear();
        self.main.pane.st.borrow_mut().tag = self.view.value() == 0;
        for p in &mut self.side {
            p.pane.clear();
        }
        if side {
            self.main.col.hide();
            let plats = self.plats.borrow();
            for (i, p) in self.side.iter_mut().enumerate() {
                if plats.contains(&i) || plats.is_empty() && i == 0 {
                    p.col.show();
                } else {
                    p.col.hide();
                }
            }
        } else {
            self.main.col.show();
            for p in &mut self.side {
                p.col.hide();
            }
        }
        // All together: no chat box (Message all sends); Reply opens it for that line.
        let together = self.view.value() == 0;
        let mut bar = self.main.bar.clone();
        let mut head = self.main.head.clone();
        if together {
            head.hide();
            self.main.col.fixed(&head, 0);
        } else {
            head.show();
            self.main.col.fixed(&head, 24);
        }
        if together {
            (self.main.set_reply)(None);
            bar.hide();
            self.main.col.fixed(&bar, 0);
            let mut read_only = self.main.read_only.clone();
            read_only.hide();
            self.main.col.fixed(&read_only, 0);
            let mut space = self.main.down_bar.clone();
            space.hide();
            self.main.col.fixed(&space, 0);
            let (cfg, cmds) = (self.cfg.clone(), self.cmds.clone());
            self.main.pane.on_reply.replace(Some(Rc::new(move |l: &Line| {
                message_all_popup(&cfg, &cmds, Some(l.clone()))
            })));
        } else {
            bar.show();
            self.main.col.fixed(&bar, ROW + 4);
            let set_reply = self.main.set_reply.clone();
            self.main
                .pane
                .on_reply
                .replace(Some(Rc::new(move |l: &Line| set_reply(Some(l)))));
        }
        self.main.col.layout();
        self.client_gen = 0;
        self.seen = 0;
        self.refresh_senders();
        if fill {
            self.refresh();
        }
        self.tab.layout();
        self.tab.redraw();
    }
}

pub fn tab(tabs: &Wizard, cfg: &Cfg, cmds: &Cmds) -> Rc<RefCell<Chat>> {
    let mut col = super::tab(tabs, "Chat");
    let mut top = Flex::default().row();
    top.set_pad(6);
    let l = Frame::default().with_label(tr("chat.show"));
    top.fixed(&l, 46);
    let mut view = super::fit::choice();
    view.set_tooltip(tr("chat.view_label"));
    top.fixed(&view, 136);
    let mut hint = Frame::default()
        .with_label(tr("chat.hint"))
        .with_align(Align::Right | Align::Inside | Align::Clip)
        .ellipsis();
    hint.set_label_size(12);
    hint.set_label_color(super::muted());

    let viewers = viewer_counter();
    top.fixed(&viewers, 58);

    // Discrete "Message all" button with broadcast icon
    let mut msg_all = Button::default().with_label(tr("chat.message_all_btn"));
    msg_all.set_image(icons::before_label("megaphone", Color::Foreground, 14));
    super::fit::adaptive(&mut msg_all);
    msg_all.set_tooltip(tr("chat.message_all_tooltip"));
    let msg_all_w = super::fit::grown(114, tr("chat.message_all_btn"), true);
    top.fixed(&msg_all, msg_all_w);

    let mut opts = fltk::menu::MenuButton::default().with_label(tr("chat.options"));
    top.fixed(&opts, 84);
    let mut clear = button(&mut top, tr("btn.clear"), 64);
    top.end();
    col.fixed(&top, ROW);
    // Narrow: the buttons keep only their icons, and the hint goes.
    top.resize_callback({
        let (msg_all, clear, mut hint) = (msg_all.clone(), clear.clone(), hint.clone());
        // As wide as `button` makes them.
        let clear_w = super::fit::grown(84, tr("btn.clear"), true);
        move |t, _, _, w, _| {
            let narrow = w < 600;
            t.fixed(&msg_all, if narrow { 36 } else { msg_all_w });
            t.fixed(&clear, if narrow { 36 } else { clear_w });
            if narrow {
                hint.hide();
            } else {
                hint.show();
            }
        }
    });

    let go_accounts: Rc<dyn Fn()> = {
        let tabs = tabs.clone();
        Rc::new(move || {
            super::nav::go(&tabs, "Accounts");
        })
    };

    let mut panes = Flex::default().row();
    panes.set_pad(6);
    let main = ChatColumn::new(cfg, cmds, None, go_accounts.clone());
    main.pane.st.borrow_mut().tag = true;
    let mut side = Vec::new();
    for idx in 0..NAMES.len() {
        let mut p = ChatColumn::new(cfg, cmds, Some(idx), go_accounts.clone());
        p.col.hide();
        side.push(p);
    }
    panes.end();
    col.end();

    let me = Rc::new(RefCell::new(Chat {
        tab: col,
        cfg: cfg.clone(),
        cmds: cmds.clone(),
        view: view.clone(),
        viewers,
        plats: Rc::default(),
        main,
        side,
        client_gen: 0,
        seen: 0,
        combined_connections: [false; 4],
    }));
    {
        let mut m = me.borrow_mut();
        *m.plats.borrow_mut() = shown_platforms(cmds);
        m.lists();
        let was = cfg.borrow().window.chat_view.clone();
        let at = m.view.find_index(&was);
        m.view.set_value(at.max(0));
        if at > 0 {
            m.rebuild(false);
        }
    }

    view.set_callback({
        let me = me.clone();
        let cfg = cfg.clone();
        move |v| {
            let view = v.choice().unwrap_or_default();
            cfg.send_if_modified(|c| {
                c.window.chat_view = view;
                false
            });
            me.borrow_mut().rebuild(true);
        }
    });
    DEV_VIEW.with_borrow_mut(|saved| *saved = Some(view.clone()));

    msg_all.set_callback({
        let (cfg, cmds) = (cfg.clone(), cmds.clone());
        move |_| message_all_popup(&cfg, &cmds, None)
    });

    {
        let w = &cfg.borrow().window;
        SHOW.set((
            !w.chat_no_time,
            !w.chat_no_badges,
            !w.chat_still_emotes,
            w.chat_youtube_tag,
        ));
    }
    let (time, badges, moving, yt_tag) = SHOW.get();
    for (i, (label, on)) in [
        (tr("chat.opt_time"), time),
        (tr("chat.opt_badges"), badges),
        (tr("chat.opt_animated"), moving),
        (tr("chat.opt_yt_tag"), yt_tag),
    ]
    .into_iter()
    .enumerate()
    {
        let flag = if on {
            fltk::menu::MenuFlag::Toggle | fltk::menu::MenuFlag::Value
        } else {
            fltk::menu::MenuFlag::Toggle
        };
        let (me, cfg) = (me.clone(), cfg.clone());
        opts.add(label, fltk::enums::Shortcut::None, flag, move |m| {
            let on = m.at(i as i32).is_some_and(|x| x.value());
            let (mut time, mut badges, mut moving, mut yt_tag) = SHOW.get();
            match i {
                0 => time = on,
                1 => badges = on,
                2 => moving = on,
                _ => yt_tag = on,
            }
            SHOW.set((time, badges, moving, yt_tag));
            cfg.send_if_modified(|c| {
                let w = &mut c.window;
                (
                    w.chat_no_time,
                    w.chat_no_badges,
                    w.chat_still_emotes,
                    w.chat_youtube_tag,
                ) = (!time, !badges, !moving, yt_tag);
                false
            });
            me.borrow().images();
        });
    }
    // Which chats the views show: each platform's, to put one back after Hide chat.
    for (i, name) in NAMES.into_iter().enumerate() {
        let hidden = cfg.borrow().window.chat_hidden.iter().any(|h| h == PLATFORMS[i]);
        let flag = if hidden {
            fltk::menu::MenuFlag::Toggle
        } else {
            fltk::menu::MenuFlag::Toggle | fltk::menu::MenuFlag::Value
        };
        let cfg = cfg.clone();
        opts.add(
            &trf("chat.opt_show_chat", &[("platform", name)]),
            fltk::enums::Shortcut::None,
            flag,
            move |m| {
                let on = m.at(4 + i as i32).is_some_and(|x| x.value());
                cfg.send_modify(|c| {
                    let h = &mut c.window.chat_hidden;
                    h.retain(|x| x != PLATFORMS[i]);
                    if !on {
                        h.push(PLATFORMS[i].to_string());
                    }
                });
                crate::ui::changed();
            },
        );
    }
    clear.set_callback({
        let me = me.clone();
        move |_| {
            let m = me.borrow();
            m.main.pane.clear();
            for p in &m.side {
                p.pane.clear();
            }
        }
    });

    me
}

/// Popup opened when clicking the Account Indicator button on the left of a chat textbox:
/// shows account details, connection status, and lets the user switch between Broadcaster and Bot
/// (and in `"All together"` mode, pick which platform the single input sends to).
fn account_selector_popup(cfg: &Cfg, cmds: &Cmds, col_ui: &ChatColumn, go_accounts: Rc<dyn Fn()>) {
    let browser = PLATFORMS[col_ui.target_plat.get().min(3)] == "tiktok";
    let h = if browser { 180 } else { 236 } + i32::from(crate::secrets::is_locked()) * 34;
    let mut win = DoubleWindow::default()
        .with_size(430, h)
        .with_label(tr("chat.acct_win_title"));
    win.set_xclass("streamerz");
    win.make_modal(true);

    let mut root = Flex::new(10, 10, 410, h - 20, None).column();
    root.set_pad(8);

    if crate::secrets::is_locked() {
        let mut lrow = Flex::default().row();
        lrow.set_pad(8);
        let mut lbl = Frame::default()
            .with_label(tr("chat.acct_keyring_locked"))
            .with_align(Align::Left | Align::Inside)
            .ellipsis();
        lbl.set_label_color(Color::from_rgb(0xe0, 0xa8, 0x30));
        let mut ubtn = button(&mut lrow, tr("btn.unlock"), 80);
        lrow.end();
        root.fixed(&lrow, ROW);
        let cmds_c = cmds.clone();
        let mut win_c = win.clone();
        ubtn.set_callback(move |_| {
            unlock_logins_interactive(&cmds_c);
            win_c.hide();
        });
    }

    let mut head = Flex::default().row();
    head.set_pad(8);
    let head_logo = Frame::default();
    head.fixed(&head_logo, 24);
    let mut head_title = Frame::default()
        .with_align(Align::Left | Align::Inside | Align::Clip)
        .ellipsis();
    head_title.set_label_font(Font::HelveticaBold);
    head.end();
    root.fixed(&head, ROW);

    // One browser account on TikTok; broadcaster and bot on other platforms.
    let mut slot_uis = Vec::new();
    for _ in 0..if browser { 1 } else { 2 } {
        let mut row = Flex::default().row();
        row.set_pad(8);
        let pic = Frame::default();
        row.fixed(&pic, 36);
        let info = Frame::default().with_align(Align::Left | Align::Inside | Align::Wrap);
        let mut pick_btn = button(&mut row, tr("btn.select"), 96);
        pick_btn.set_frame(FrameType::RFlatBox);
        if browser {
            row.fixed(&pick_btn, 0);
            pick_btn.hide();
        }

        row.end();
        root.fixed(&row, 48);
        slot_uis.push((pic, info, pick_btn));
    }

    let note = Frame::default()
        .with_label(tr("chat.acct_note"))
        .with_align(Align::Left | Align::Inside | Align::Wrap);
    root.fixed(&note, 28);

    let mut bottom = Flex::default().row();
    bottom.set_pad(8);
    let mut manage = button(&mut bottom, tr("chat.manage_accounts"), 110);
    Frame::default();
    let mut close = button(&mut bottom, tr("dialogs.close"), 80);
    bottom.end();
    root.fixed(&bottom, ROW);
    root.end();
    win.end();

    let done = Rc::new(Cell::new(false));
    let mut populate = {
        let (cfg, cmds) = (cfg.clone(), cmds.clone());
        let mut head_logo = head_logo.clone();
        let mut head_title = head_title.clone();
        let mut note = note.clone();
        let slot_uis: Vec<_> = slot_uis.clone();
        let target_plat = col_ui.target_plat.clone();
        let mut last = None;
        move || {
            let p_idx = target_plat.get().min(3);
            let slug = PLATFORMS[p_idx];
            let pname = NAMES[p_idx];
            let (conn, standby) = query(&cmds, move |e| {
                (
                    e.chat_connected(slug),
                    match slug {
                        "youtube" => e.youtube_standby(),
                        "tiktok" => e.tiktok_standby(),
                        _ => false,
                    },
                )
            })
            .unwrap_or((false, false));
            let bridge = (slug == "tiktok").then(|| {
                query(&cmds, |e| {
                    (
                        e.tiktok_bridge.ready(),
                        e.tiktok_bridge.user(),
                        e.tiktok_bridge.error(),
                        e.tiktok_bridge.is_connected(),
                        e.tiktok_bridge.avatar(),
                    )
                })
                .unwrap_or_default()
            });
            let want_broadcaster = cfg.borrow().window.chat_as_broadcaster.iter().any(|s| s == slug);
            let slots = platform_accounts(&cmds, &cfg, slug, 32);
            let (eff_bot, _, _) = effective_sender_role(&cfg, &cmds, p_idx);
            let state = (
                p_idx,
                conn,
                standby,
                want_broadcaster,
                eff_bot,
                bridge.clone(),
                slots
                    .each_ref()
                    .map(|s| s.as_ref().map(|a| (a.name.clone(), a.note, a.pic.is_some()))),
                img::ARRIVED.load(std::sync::atomic::Ordering::Relaxed),
            );
            // Redrawing on each wait wakes this loop again. Only changed account data or pictures
            // need a refresh; decoded pictures are observed by platform_accounts above.
            if last.as_ref() == Some(&state) {
                return;
            }
            last = Some(state);
            head_logo.set_image(img::logo(pname, 18));
            let (h, st_txt) = if conn {
                (super::Health::Up, tr("conns.connected"))
            } else if standby {
                (super::Health::Standby, tr("accounts.standby_default"))
            } else {
                (super::Health::Off, tr("conns.off"))
            };
            if let Some((ready, user, error, connected, _)) = bridge {
                let (health, detail) = if !error.is_empty() {
                    (super::Health::Off, error)
                } else if !connected {
                    (super::Health::Off, tr("accounts.tiktok_sending.waiting").to_string())
                } else if ready && !user.is_empty() {
                    (super::Health::Up, tr("conns.connected").to_string())
                } else {
                    (
                        super::Health::Standby,
                        tr("accounts.tiktok_sending.login_needed").to_string(),
                    )
                };
                health.show(&mut head_title, &format!("{pname}: {detail}"));
            } else {
                h.show(&mut head_title, &format!("{pname}: {st_txt}"));
            }
            head_title.set_label_font(Font::HelveticaBold);
            note.set_label(if slug == "tiktok" {
                tr("chat.tiktok_account_note")
            } else {
                tr("chat.acct_note")
            });

            for (bot_i, ((mut pic_f, mut info_f, mut btn), slot)) in
                slot_uis.iter().cloned().zip(slots.iter()).enumerate()
            {
                let is_bot = bot_i == 1;
                let role = if slug == "tiktok" {
                    tr("accounts.row_sending")
                } else if is_bot {
                    tr("chat.role_bot")
                } else {
                    tr("chat.role_broadcaster")
                };
                let is_active = eff_bot == is_bot && slot.is_some();
                let is_pref = want_broadcaster == !is_bot;
                match slot {
                    Some(s) => {
                        match &s.pic {
                            Some(im) => pic_f.set_image(Some(im.clone())),
                            None => pic_f.set_image(icons::icon("circle-user-round", super::muted(), 28)),
                        }
                        let label = match s.note {
                            Some(n) => format!("{role}: {} ({n})", s.name.replace('@', "@@")),
                            None => format!("{role}: {}", s.name.replace('@', "@@")),
                        };
                        info_f.set_label(&label);
                        btn.activate();
                        if is_active {
                            btn.set_label(tr("chat.btn_active"));
                            btn.set_image(icons::before_label("check", Color::White, 14));
                            btn.set_color(Color::from_rgb(0x2e, 0x6b, 0x47));
                        } else {
                            btn.set_label(tr("chat.btn_use_account"));
                            btn.set_image(None::<SvgImage>);
                            btn.set_color(Color::Background);
                        }
                    }
                    None => {
                        pic_f.set_image(icons::icon("circle-user-round", super::muted(), 28));
                        let suffix = if is_pref { tr("chat.preferred_other_suffix") } else { "" };
                        info_f.set_label(&if slug == "tiktok" {
                            tr("accounts.not_logged_in").to_string()
                        } else {
                            trf("chat.role_not_logged_in", &[("role", role), ("suffix", suffix)])
                        });
                        btn.set_label(tr("chat.btn_not_logged_in"));
                        btn.set_color(Color::Background);
                        btn.deactivate();
                    }
                }
                if slug == "tiktok" {
                    btn.deactivate();
                    btn.set_tooltip(tr("chat.tiktok_browser_tip"));
                }
                pic_f.redraw();
                info_f.redraw();
                btn.redraw();
            }
        }
    };

    for (bot_i, (_, _, mut btn)) in slot_uis.into_iter().enumerate() {
        let is_bot = bot_i == 1;
        let cfg = cfg.clone();
        let target_plat = col_ui.target_plat.clone();
        let done = done.clone();
        btn.set_callback(move |_| {
            let p_idx = target_plat.get().min(3);
            let slug = PLATFORMS[p_idx];
            cfg.send_modify(|c| {
                c.window.chat_as_broadcaster.retain(|x| x != slug);
                if !is_bot {
                    c.window.chat_as_broadcaster.push(slug.to_string());
                }
            });
            crate::save_later(&cfg.borrow());
            done.set(true);
        });
    }

    manage.set_callback({
        let done = done.clone();
        move |_| {
            go_accounts();
            done.set(true);
        }
    });
    close.set_callback({
        let done = done.clone();
        move |_| done.set(true)
    });
    win.set_callback({
        let done = done.clone();
        move |_| done.set(true)
    });

    populate();
    win.show();
    while win.shown() && !done.get() {
        super::nested_wait(None);
        if win.shown() && !done.get() {
            populate();
        }
    }
    win.hide();
    app::delete_widget(win);
}

/// Popup opened by the discrete `"Message all"` button in the top bar:
/// lets the user pick target platforms, bot/broadcaster + fallback behavior, emotes, and send.
fn message_all_popup(cfg: &Cfg, cmds: &Cmds, reply: Option<Line>) {
    if crate::memory_saver() || !cfg.borrow().window.chat.save_history {
        return;
    }
    let conn_states: [bool; 4] = query(cmds, |e| {
        [
            e.twitch_connected(),
            e.youtube_connected(),
            e.kick_connected(),
            e.tiktok_connected(),
        ]
    })
    .unwrap_or_default();
    // Only where an account is logged in: a chat read without one can't be sent to.
    let mut set_up = PLATFORMS.map(|p| crate::engine::can_send(&cfg.borrow(), p));
    set_up[3] &= query(cmds, |e| e.tiktok_bridge.ready() && !e.tiktok_bridge.user().is_empty()).unwrap_or(false);
    let conn_states: [bool; 4] = std::array::from_fn(|i| conn_states[i] && set_up[i]);

    let mut win = DoubleWindow::default()
        .with_size(520, 210)
        .with_label(tr("chat.msg_all_title"));
    win.set_xclass("streamerz");
    win.make_modal(true);
    let mut col = Flex::new(12, 12, 496, 186, None).column();
    col.set_pad(8);

    let mut title = Frame::default()
        .with_label(tr("chat.send_to"))
        .with_align(Align::Left | Align::Inside | Align::Clip)
        .ellipsis();
    if let Some(l) = &reply {
        let said = trf("chat.replying_to", &[("user", &l.user), ("said", &l.text)]);
        title.set_label(&said.replace('@', "@@"));
        title.set_image(icons::before_label("corner-up-left", super::muted(), 14));
        title.set_align(Align::Left | Align::Inside | Align::Clip | Align::ImageNextToText);
        title.set_label_color(super::muted());
    }
    col.fixed(&title, 22);

    let mut plat_row = Flex::default().row();
    plat_row.set_pad(8);
    let mut checks = Vec::new();
    for i in 0..PLATFORMS.len() {
        let suffix = if conn_states[i] { "" } else { tr("chat.offline_suffix") };
        let mut c = CheckButton::default().with_label(&format!("{}{suffix}", NAMES[i]));
        let any_conn = conn_states.iter().any(|&x| x);
        c.set_checked(
            set_up[i]
                && match &reply {
                    Some(l) => l.platform == PLATFORMS[i],
                    None if any_conn => conn_states[i],
                    None => set_up[i],
                },
        );
        if !set_up[i] {
            c.deactivate();
        }
        checks.push(c);
    }
    plat_row.end();
    col.fixed(&plat_row, ROW);

    let mut acct_row = Flex::default().row();
    acct_row.set_pad(8);
    let lbl = Frame::default()
        .with_label(tr("chat.send_as"))
        .with_align(Align::Left | Align::Inside)
        .ellipsis();
    acct_row.fixed(&lbl, 68);
    let mut who = super::fit::choice();
    who.add_choice(&format!(
        "{}|{}",
        tr("chat.who_bot_accounts"),
        tr("chat.who_broadcaster_accounts")
    ));
    who.set_value(0);
    acct_row.fixed(&who, 175);
    let fallback = CheckButton::default().with_label(tr("chat.fallback_account"));
    fallback.set_checked(true);
    acct_row.end();
    col.fixed(&acct_row, ROW);
    let update_accounts: Rc<dyn Fn()> = Rc::new({
        let (checks, row, col) = (checks.clone(), acct_row.clone(), col.clone());
        move || {
            let (mut row, mut col) = (row.clone(), col.clone());
            let only_browser = checks[3].is_checked() && !checks[..3].iter().any(CheckButton::is_checked);
            if only_browser {
                row.hide();
            } else {
                row.show();
            }
            col.fixed(&row, if only_browser { 0 } else { ROW });
            col.layout();
        }
    });
    for (i, mut check) in checks.iter().cloned().enumerate() {
        if i == 3 {
            check.set_tooltip(tr("chat.tiktok_browser_tip"));
        }
        let update = update_accounts.clone();
        check.set_callback(move |_| update());
    }
    update_accounts();

    let mut bar = Flex::default().row();
    bar.set_pad(4);
    let mut input = Input::default();
    let click = emote_field(&mut input, cmds, Rc::new(Cell::new(0)), tr("chat.send_placeholder"));
    let mut emote_btn = Button::default();
    emote_btn.set_frame(FrameType::FlatBox);
    emote_btn.set_image(icons::icon("smile", super::muted(), 18));
    emote_btn.set_tooltip(tr("chat.pick_emotes_tooltip"));
    bar.fixed(&emote_btn, 32);
    let mut send_btn = Button::default();
    send_btn.set_frame(FrameType::FlatBox);
    send_btn.set_image(icons::icon("send-horizontal", Color::Selection, 18));
    send_btn.set_tooltip(tr("chat.send_all_tooltip"));
    bar.fixed(&send_btn, 32);
    bar.end();
    col.fixed(&bar, ROW + 4);

    let mut foot = Flex::default().row();
    foot.set_pad(8);
    let status = Frame::default().with_align(Align::Left | Align::Inside).ellipsis();
    let mut close = button(&mut foot, tr("dialogs.cancel"), 80);
    foot.end();
    col.fixed(&foot, ROW);
    col.end();
    win.end();

    let done = Rc::new(Cell::new(false));
    let do_send = {
        let cmds = cmds.clone();
        let checks = checks.clone();
        let input = input.clone();
        let who = who.clone();
        let fallback = fallback.clone();
        let mut status = status.clone();
        let done = done.clone();
        move || {
            let text = input.value().trim().to_string();
            if text.is_empty() {
                return;
            }
            let targets: Vec<&'static str> = (0..checks.len())
                .filter(|&i| checks[i].is_checked())
                .map(|i| PLATFORMS[i])
                .collect();
            if targets.is_empty() {
                status.set_label(tr("chat.check_at_least_one"));
                status.set_label_color(Color::from_rgb(0xe0, 0x60, 0x50));
                status.redraw();
                return;
            }
            if targets.contains(&"tiktok") {
                if let Err(error) = crate::tiktok_bridge::validate_message(&text) {
                    super::dialogs::message(&error);
                    return;
                }
            }
            let use_bot = who.value() == 0;
            let fb = fallback.is_checked();
            let reply = reply.clone();
            tell(&cmds, move |e| {
                let connected: Vec<_> = targets.into_iter().filter(|p| e.chat_connected(p)).collect();
                let _ = e.send_chat_list(&connected, &text, use_bot, fb, reply.as_ref());
            });
            done.set(true);
        }
    };

    send_btn.set_callback({
        let mut s = do_send.clone();
        move |_| s()
    });
    input.handle({
        let mut s = do_send;
        move |i, ev| {
            if click(i, ev) {
                return true;
            }
            if ev == Event::KeyDown && app::event_key() == Key::Enter {
                s();
                return true;
            }
            false
        }
    });
    emote_btn.set_callback({
        let (cfg, cmds) = (cfg.clone(), cmds.clone());
        let who = who.clone();
        let mut input_c = input.clone();
        move |_| {
            emote_picker_popup(&cfg, &cmds, "all", who.value() == 1, None, &mut input_c);
        }
    });
    close.set_callback({
        let done = done.clone();
        move |_| done.set(true)
    });
    win.set_callback({
        let done = done.clone();
        move |_| done.set(true)
    });

    win.show();
    let _ = input.take_focus();
    while win.shown() && !done.get() {
        super::nested_wait(Some(0.1));
    }
    win.hide();
    app::delete_widget(win);
}

fn provider_icon(prov: Provider, platform: &str, size: i32) -> Option<SvgImage> {
    match prov {
        Provider::Favorites => icons::icon("star", Color::from_rgb(0xf5, 0xc5, 0x42), size),
        Provider::Platform => {
            let name = match platform {
                "twitch" => "Twitch",
                "kick" => "Kick",
                "youtube" => "YouTube",
                "tiktok" => "TikTok",
                _ => "Twitch",
            };
            img::logo(name, size)
        }
        Provider::SevenTv => icon_sized("ui:7tv", size),
        Provider::Bttv => icon_sized("ui:bttv", size),
        Provider::Ffz => icon_sized("ui:ffz", size),
        Provider::Emoji => icons::icon("smile", super::muted(), size),
    }
}

thread_local! {
    /// Emotes picked in the picker this run, by name: the chat box shows them even when no catalog has them.
    static PICKED: RefCell<HashMap<String, String>> = RefCell::default();
}

/// A piece of the chat box's text: its bytes, and its picture when it's an emote or emoji.
type Piece = (std::ops::Range<usize>, Option<Pic>);

/// `v` cut into text and emotes, the emotes as `h` px pictures (those not loaded yet stay text).
fn pieces(cmds: &Cmds, platform: &str, v: &str, h: i32) -> Vec<Piece> {
    let mut out: Vec<Piece> = Vec::new();
    let mut text_from = 0;
    let mut at = 0;
    for w in v.split(' ') {
        let start = at;
        at += w.len() + 1;
        let url = PICKED
            .with_borrow(|p| p.get(w).cloned())
            .or_else(|| emotes::third(platform, w));
        let found: Vec<(std::ops::Range<usize>, String)> = match url {
            Some(u) if !w.is_empty() => vec![(start..start + w.len(), u)],
            _ => {
                let mut o = start;
                emotes::split(w)
                    .into_iter()
                    .filter_map(|(part, url)| {
                        let r = o..o + part.len();
                        o += part.len();
                        url.map(|u| (r, u))
                    })
                    .collect()
            }
        };
        for (r, u) in found {
            if let Some(p) = img::get(cmds, &u, h, false) {
                if text_from < r.start {
                    out.push((text_from..r.start, None));
                }
                text_from = r.end;
                out.push((r, Some(p)));
            }
        }
    }
    if text_from < v.len() {
        out.push((text_from..v.len(), None));
    }
    out
}

/// The chat box shows emotes and emoji as pictures. Drawn by hand only when it has some (plain text
/// keeps FLTK's own drawing), clicking and selecting follow the pictures.
fn emote_field(
    input: &mut Input,
    cmds: &Cmds,
    platform: Rc<Cell<usize>>,
    empty: &'static str,
) -> impl Fn(&mut Input, Event) -> bool {
    // Each piece's x from the text's start, and the whole width.
    fn layout(i: &Input, ps: &[Piece]) -> (Vec<i32>, i32) {
        draw::set_font(i.text_font(), i.text_size());
        let mut x = 0;
        let xs = ps
            .iter()
            .map(|(r, p)| {
                let at = x;
                x += p
                    .as_ref()
                    .map_or_else(|| draw::width(&i.value()[r.clone()]) as i32, |p| p.w() + 2);
                at
            })
            .collect();
        (xs, x)
    }
    // Where byte `pos` is, from the text's start.
    fn x_of(i: &Input, ps: &[Piece], xs: &[i32], pos: usize) -> i32 {
        let v = i.value();
        for ((r, p), x) in ps.iter().zip(xs) {
            if pos <= r.end && pos >= r.start {
                return match p {
                    Some(p) if pos > r.start => x + p.w() + 2,
                    Some(_) => *x,
                    None => x + draw::width(&v[r.start..pos]) as i32,
                };
            }
        }
        xs.last().copied().unwrap_or(0)
    }
    let state = {
        let (cmds, platform) = (cmds.clone(), platform.clone());
        move |i: &Input| {
            let v = i.value();
            pieces(&cmds, PLATFORMS[platform.get().min(3)], &v, i.h() - 10)
        }
    };
    // Scrolled so the cursor shows: the text's start, left of the box.
    let shift = |i: &Input, cur: i32, total: i32| (cur - (i.w() - 14)).max(0).min(total.max(0));
    input.draw({
        let state = state.clone();
        move |i| {
            draw::draw_box(i.frame(), i.x(), i.y(), i.w(), i.h(), i.color());
            let v = i.value();
            if v.is_empty() && !i.has_focus() {
                draw::set_font(Font::Helvetica, i.text_size());
                draw::set_draw_color(super::muted());
                draw::draw_text2(empty, i.x() + 6, i.y(), i.w() - 12, i.h(), Align::Left | Align::Clip);
                return;
            }
            let ps = state(i);
            let (xs, total) = layout(i, &ps);
            let (pos, mark) = (i.position() as usize, i.mark() as usize);
            let cur = x_of(i, &ps, &xs, pos);
            let x0 = i.x() + 6 - shift(i, cur, total);
            draw::push_clip(i.x() + 3, i.y() + 3, i.w() - 6, i.h() - 6);
            draw::draw_rect_fill(i.x() + 3, i.y() + 3, i.w() - 6, i.h() - 6, i.color());
            if pos != mark {
                let (a, b) = (x_of(i, &ps, &xs, pos.min(mark)), x_of(i, &ps, &xs, pos.max(mark)));
                draw::draw_rect_fill(x0 + a, i.y() + 4, b - a, i.h() - 8, Color::Selection);
            }
            draw::set_font(i.text_font(), i.text_size());
            draw::set_draw_color(i.text_color());
            let base = i.y() + (i.h() + draw::height()) / 2 - draw::descent();
            for ((r, p), x) in ps.iter().zip(&xs) {
                match p {
                    Some(p) => {
                        let mut f = p.first();
                        f.draw(x0 + x + 1, i.y() + (i.h() - f.h()) / 2, f.w(), f.h());
                    }
                    None => draw::draw_text(&v[r.clone()], x0 + x, base),
                }
            }
            if i.has_focus() {
                draw::set_draw_color(i.cursor_color());
                draw::draw_line(x0 + cur, i.y() + 5, x0 + cur, i.y() + i.h() - 6);
            }
            draw::pop_clip();
        }
    });
    // Clicks and key presses
    move |i: &mut Input, ev: Event| {
        if matches!(ev, Event::Focus | Event::Unfocus) {
            i.redraw();
            return false;
        }
        if ev == Event::KeyDown {
            let key = app::event_key();
            let is_backspace = key == fltk::enums::Key::BackSpace
                || key.bits() == 0xff08
                || key.bits() == 8
                || app::event_text() == "\x08";
            let is_delete = key == fltk::enums::Key::Delete || key.bits() == 0xffff || app::event_text() == "\x7f";

            if is_backspace {
                let pos = i.position() as usize;
                let mark = i.mark() as usize;
                let v = i.value();
                let ps = state(i);
                if pos != mark {
                    let (mut start, mut end) = (pos.min(mark), pos.max(mark));
                    for (r, p) in &ps {
                        if p.is_some() && start < r.end && end > r.start {
                            start = start.min(r.start);
                            end = end.max(r.end);
                        }
                    }
                    let new_v = format!("{}{}", &v[..start], &v[end..]);
                    i.set_value(&new_v);
                    let _ = i.set_position(start as i32);
                    let _ = i.set_mark(start as i32);
                    i.redraw();
                    return true;
                } else if pos > 0 {
                    // Check if directly after an emote or after an emote's trailing space
                    if let Some((start, end)) = ps.iter().find_map(|(r, p)| {
                        if p.is_none() {
                            return None;
                        }
                        if pos > r.start && pos <= r.end {
                            let trail = usize::from(v[r.end..].starts_with(' '));
                            Some((r.start, r.end + trail))
                        } else if pos == r.end + 1 && v.as_bytes().get(r.end) == Some(&b' ') {
                            Some((r.start, pos))
                        } else {
                            None
                        }
                    }) {
                        let new_v = format!("{}{}", &v[..start], &v[end..]);
                        i.set_value(&new_v);
                        let _ = i.set_position(start as i32);
                        let _ = i.set_mark(start as i32);
                        i.redraw();
                        return true;
                    }
                }
            } else if is_delete {
                let pos = i.position() as usize;
                let mark = i.mark() as usize;
                let v = i.value();
                let ps = state(i);
                if pos != mark {
                    let (mut start, mut end) = (pos.min(mark), pos.max(mark));
                    for (r, p) in &ps {
                        if p.is_some() && start < r.end && end > r.start {
                            start = start.min(r.start);
                            end = end.max(r.end);
                        }
                    }
                    let new_v = format!("{}{}", &v[..start], &v[end..]);
                    i.set_value(&new_v);
                    let _ = i.set_position(start as i32);
                    let _ = i.set_mark(start as i32);
                    i.redraw();
                    return true;
                } else {
                    if let Some((start, end)) = ps.iter().find_map(|(r, p)| {
                        if p.is_some() && pos >= r.start && pos < r.end {
                            let trail = usize::from(v[r.end..].starts_with(' '));
                            Some((r.start, r.end + trail))
                        } else {
                            None
                        }
                    }) {
                        let new_v = format!("{}{}", &v[..start], &v[end..]);
                        i.set_value(&new_v);
                        let _ = i.set_position(start as i32);
                        let _ = i.set_mark(start as i32);
                        i.redraw();
                        return true;
                    }
                }
            }
        }
        if !matches!(ev, Event::Push | Event::Drag) || !super::left_button() {
            return false;
        }
        let ps = state(i);
        let (xs, total) = layout(i, &ps);
        let cur = x_of(i, &ps, &xs, i.position() as usize);
        let ex = app::event_x() - (i.x() + 6 - shift(i, cur, total));
        // The piece edge or character nearest the click.
        let v = i.value();
        let at = ps
            .iter()
            .flat_map(|(r, p)| match p {
                Some(_) => vec![r.start, r.end],
                None => v[r.clone()]
                    .char_indices()
                    .map(|(k, _)| r.start + k)
                    .chain([r.end])
                    .collect(),
            })
            .min_by_key(|&at| (x_of(i, &ps, &xs, at) - ex).abs())
            .unwrap_or(0) as i32;
        if ev == Event::Push {
            let _ = i.take_focus();
        }
        let shifted = app::event_state().contains(fltk::enums::Shortcut::Shift);
        let mark = if ev == Event::Push && !shifted { at } else { i.mark() };
        let _ = i.set_position(at);
        let _ = i.set_mark(mark);
        i.redraw();
        true
    }
}

fn insert_emote_word(input: &mut Input, word: &str, url: &str) {
    PICKED.with_borrow_mut(|p| p.insert(word.to_string(), url.to_string()));
    let mut v = input.value();
    if !v.is_empty() && !v.ends_with(' ') {
        v.push(' ');
    }
    v.push_str(word);
    v.push(' ');
    input.set_value(&v);
    let _ = input.set_position(v.len() as i32);
}

/// The emote and emoji picker: a tab per source (favorites, the platform's, 7TV, BTTV, FFZ, emoji), a
/// search across them, and the grid. A click puts one in the chat box and closes; Shift+click keeps it
/// open to pick several; a right click (un)favorites.
fn emote_picker_popup(
    cfg: &Cfg,
    cmds: &Cmds,
    platform: &'static str,
    as_broadcaster: bool,
    sender_name: Option<&str>,
    target_input: &mut Input,
) {
    if crate::memory_saver() || !cfg.borrow().window.chat.save_history {
        return;
    }
    emotes::load_for_config(cmds, cfg);
    let seen: Vec<(String, String)> = query(cmds, move |e| {
        let mut v: Vec<(String, String)> = Vec::new();
        for x in PLATFORMS.iter().filter(|x| platform == "all" || **x == platform) {
            if *x == "youtube" {
                continue;
            }
            for em in e.chat_emotes(x) {
                if !v.iter().any(|y| y.0 == em.0) {
                    v.push(em);
                }
            }
        }
        v
    })
    .unwrap_or_default();

    let providers = emotes::providers_for(platform);
    let default_tab = providers.iter().position(|&p| p == Provider::Platform).unwrap_or(0);
    // A popover over the chat, above its box. Where the box is in a small dialog (no room) it gets a
    // window of its own, as before.
    let Some(parent) = target_input.top_window() else {
        return;
    };
    let own = parent.h() < 520;
    let mut own_win = own.then(|| {
        let mut w = DoubleWindow::default().with_size(380, 480).with_label("Streamerz");
        w.set_xclass("streamerz");
        w.make_modal(true);
        w
    });
    let host: Box<dyn WindowExt> = match &own_win {
        Some(w) => Box::new(w.clone()),
        None => parent,
    };
    let (x, y) = if own {
        (0, 0)
    } else {
        // Above the box, or below it when there's no room above.
        let above = target_input.y() - 460.min(host.h() - 16) - 8;
        (
            target_input.x(),
            if above >= 8 {
                above
            } else {
                target_input.y() + target_input.h() + 8
            },
        )
    };
    let mut pop = super::popover::Popover::start(host, x, y, 380, if own { 480 } else { 460 });
    let (pw, ph) = pop.size();
    let done = pop.done.clone();
    let mut col = Flex::new(0, 0, pw, ph, None).column();
    col.set_margin(8);
    col.set_pad(8);

    let mut tab_bar = Flex::default().row();
    tab_bar.set_pad(4);
    let mut tab_btns: Vec<Button> = Vec::new();
    for &prov in providers {
        let mut b = Button::default();
        b.set_frame(FrameType::RFlatBox);
        b.set_image(provider_icon(prov, platform, 16));
        b.set_tooltip(prov.label(platform));
        tab_btns.push(b);
    }
    tab_bar.end();
    col.fixed(&tab_bar, 32);

    let mut search = Input::default();
    search.set_trigger(CallbackTrigger::Changed);
    super::placeholder(&mut search, tr("chat.search_emotes"));
    col.fixed(&search, ROW);

    let who = match sender_name {
        Some(n) => trf("chat.as_sender_prefix", &[("name", &n.replace('@', "@@"))]),
        None => String::new(),
    };
    let mut sub = Frame::default()
        .with_label(&format!("{who}{}", tr("chat.emote_picker_help")))
        .with_align(Align::Left | Align::Inside | Align::Clip)
        .ellipsis();
    sub.set_label_size(11);
    sub.set_label_color(super::muted());
    col.fixed(&sub, 14);

    let grid = super::grid::Grid::new(cmds);
    col.end();
    pop.finish();

    let cur_tab = Rc::new(Cell::new(default_tab));
    let collapsed: Rc<RefCell<HashSet<String>>> = Rc::default();
    let seen = Rc::new(seen);

    // The open tab lit, the others quiet.
    let style_tabs = {
        let tab_btns = tab_btns.clone();
        move |active: usize| {
            for (i, mut b) in tab_btns.iter().cloned().enumerate() {
                b.set_color(if i == active {
                    Color::color_average(Color::Foreground, Color::Background, 0.2)
                } else {
                    Color::Background
                });
                b.redraw();
            }
        }
    };

    let fill = {
        let (cfg, grid, search) = (cfg.clone(), grid.clone(), search.clone());
        let (cur_tab, collapsed, seen) = (cur_tab.clone(), collapsed.clone(), seen.clone());
        move |keep: bool| {
            let q = search.value().trim().to_lowercase();
            let favs = cfg.borrow().window.favorite_emotes.clone();
            let active = providers[cur_tab.get().min(providers.len() - 1)];
            // Searching looks in every tab of this platform.
            let secs: Vec<EmoteSection> = if q.is_empty() {
                emotes::sections(platform, active, as_broadcaster, &seen, &favs)
            } else {
                let mut names = HashSet::new();
                let mut out: Vec<EmoteSection> = providers
                    .iter()
                    .filter(|&&pv| pv != Provider::Favorites)
                    .flat_map(|&pv| {
                        emotes::sections(platform, pv, as_broadcaster, &seen, &favs)
                            .into_iter()
                            .map(move |s| (pv, s))
                    })
                    .filter_map(|(pv, s)| {
                        let emotes: Vec<(String, String)> = s
                            .emotes
                            .into_iter()
                            .filter(|(n, _)| n.to_lowercase().contains(&q) && names.insert(n.clone()))
                            .take(200)
                            .collect();
                        (!emotes.is_empty()).then(|| EmoteSection {
                            title: format!("{} · {}", pv.label(platform), s.title),
                            note: None,
                            emotes,
                        })
                    })
                    .collect();
                if out.is_empty() {
                    out.push(EmoteSection {
                        title: "Search".into(),
                        note: Some(format!("Nothing called \"{q}\".")),
                        emotes: Vec::new(),
                    });
                }
                out
            };
            let mut items = Vec::new();
            for sec in secs {
                let open = !collapsed.borrow().contains(&sec.title);
                items.push(super::grid::Item::Head {
                    title: sec.title,
                    open,
                    count: sec.emotes.len(),
                    note: sec.note,
                });
                if open {
                    for (name, url) in sec.emotes.into_iter().take(500) {
                        let fav = favs.iter().any(|(n, _)| *n == name);
                        items.push(super::grid::Item::Emote { name, url, fav });
                    }
                }
            }
            grid.set(items, keep);
        }
    };

    for (idx, mut b) in tab_btns.into_iter().enumerate() {
        let (cur_tab, style_tabs, fill, mut search) =
            (cur_tab.clone(), style_tabs.clone(), fill.clone(), search.clone());
        b.set_callback(move |_| {
            cur_tab.set(idx);
            search.set_value("");
            style_tabs(idx);
            fill(false);
        });
    }
    search.set_callback({
        let fill = fill.clone();
        move |_| fill(false)
    });
    style_tabs(default_tab);
    if let Some(w) = &mut own_win {
        w.show();
    }
    fill(false);
    let _ = search.take_focus();

    let mut pics = img::ARRIVED.load(std::sync::atomic::Ordering::Relaxed);
    let mut seen_rev = emotes::REV.load(std::sync::atomic::Ordering::Relaxed);
    let mut picked = false;
    while !done.get() && own_win.as_ref().is_none_or(|w| w.shown()) {
        super::nested_wait(Some(0.25));
        let asked: Vec<super::grid::Asked> = grid.asked.borrow_mut().drain(..).collect();
        let mut refill = false;
        for a in asked {
            match a {
                super::grid::Asked::Pick(name, url, several) => {
                    insert_emote_word(target_input, &name, &url);
                    target_input.redraw();
                    picked = true;
                    if !several {
                        done.set(true);
                    }
                }
                super::grid::Asked::Favorite(name, url) => {
                    cfg.send_modify(|c| {
                        let list = &mut c.window.favorite_emotes;
                        match list.iter().position(|(n, _)| *n == name) {
                            Some(pos) => drop(list.remove(pos)),
                            None => list.push((name, url)),
                        }
                    });
                    crate::save_later(&cfg.borrow());
                    refill = true;
                }
                super::grid::Asked::Fold(title) => {
                    let mut c = collapsed.borrow_mut();
                    if !c.remove(&title) {
                        c.insert(title);
                    }
                    refill = true;
                }
            }
        }
        if img::ARRIVED.load(std::sync::atomic::Ordering::Relaxed) != pics {
            pics = img::ARRIVED.load(std::sync::atomic::Ordering::Relaxed);
            grid.area.clone().redraw();
        }
        let rev = emotes::REV.load(std::sync::atomic::Ordering::Relaxed);
        if rev != seen_rev || refill {
            seen_rev = rev;
            fill(true);
        }
    }
    pop.close();
    if let Some(mut w) = own_win {
        w.hide();
        app::delete_widget(w);
    }
    if picked {
        let _ = target_input.take_focus();
    }
}

/// The platforms shown: the connected ones, and ones whose chat is in the log (so its lines stay when
/// it drops), as indexes in PLATFORMS.
fn shown_platforms(cmds: &Cmds) -> Vec<usize> {
    query(cmds, |e| {
        let hidden = e.config().window.chat_hidden.clone();
        (0..4)
            .filter(|&i| !hidden.iter().any(|h| h == PLATFORMS[i]))
            .filter(|&i| e.chat_connected(PLATFORMS[i]) || e.chat_has(PLATFORMS[i]))
            .collect()
    })
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiktok_offers_replies_without_exposing_moderation() {
        let _g = crate::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        crate::roles::set("tiktok", crate::roles::Role::Viewer, "42");
        let line = Line {
            platform: "tiktok".into(),
            user_id: "43".into(),
            user: "Other".into(),
            ..Default::default()
        };
        let actions = acts(&line, true);
        assert!(actions.iter().any(|a| matches!(a.0, Act::Reply)));
        assert!(!actions.iter().any(|a| matches!(a.0, Act::Mod(_))));
        assert!(!acts(&line, false).iter().any(|a| matches!(a.0, Act::Reply)));
        crate::roles::clear("tiktok");
        assert!(!acts(&line, true).iter().any(|a| matches!(a.0, Act::Reply)));
    }

    #[test]
    fn control_characters_never_reach_the_chat() {
        assert_eq!(flat("a\r\nb\rc\td"), "a  b c d");
        assert_eq!(flat("plain"), "plain");
    }

    #[test]
    fn viewer_counts_are_compact_without_rounding_up() {
        assert_eq!(compact_viewers(999), "999");
        assert_eq!(compact_viewers(1_234), "1.2k");
        assert_eq!(compact_viewers(10_000), "10k");
        assert_eq!(compact_viewers(1_999_999), "1.9m");
        assert_eq!(compact_viewers(u64::MAX), "18446744073b");
    }

    #[test]
    fn a_tag_of_one_of_our_names_is_a_mention() {
        let mut c = Config::default();
        c.twitch.broadcaster = Some(crate::twitch::TwitchAccount {
            login: "pedrozamecki".into(),
            display_name: "PedroZamecki".into(),
            ..Default::default()
        });
        ME.set(my_names(&c));
        assert!(mentions_me("@PedroZamecki"));
        assert!(mentions_me("@pedrozamecki,"));
        assert!(mentions_me("@PedroZamecki!"));
        assert!(!mentions_me("PedroZamecki"));
        assert!(!mentions_me("@someoneelse"));
        assert!(!mentions_me("@"));
        ME.set(Vec::new());
    }

    #[test]
    fn reply_preview_wraps_by_word_and_marks_our_name() {
        ME.set(vec!["target".into()]);
        let reply = crate::engine::chatlog::Reply {
            user: "Target".into(),
            text: "a long message that needs to wrap beside the reply icon".into(),
        };
        let mut items = Vec::new();
        reply_preview(&reply, Color::Foreground, &mut items);
        assert!(items.iter().any(|it| matches!(it, It::Mention(t) if t == "@Target:")));
        assert!(items.iter().any(|it| matches!(it, It::Space)));
        assert!(items.iter().filter(|it| matches!(it, It::Text(..))).count() > 2);
        ME.set(Vec::new());
    }

    #[test]
    fn card_hit_test_matches_its_visible_edges() {
        assert!(card_contains(10, 20, 300, 8, 70, 18, 28));
        assert!(card_contains(10, 20, 300, 8, 70, 301, 97));
        assert!(card_contains(10, 20, 300, 8, 70, 307, 97));
        assert!(!card_contains(10, 20, 300, 8, 70, 308, 97));
        assert!(!card_contains(10, 20, 300, 8, 70, 18, 98));
    }

    #[test]
    fn a_pins_time_left_reads_as_a_clock() {
        assert_eq!(left(125_000), "2:05");
        assert_eq!(left(1), "0:01");
        assert_eq!(left(3_729_000), "1:02:09");
    }

    #[test]
    fn emote_backspace_removes_whole_token() {
        let text = "look at this test testEmote";
        let emote_range = 18..27;
        let pos = 27;
        assert!(pos > emote_range.start && pos <= emote_range.end);
        let new_text = format!("{}{}", &text[..emote_range.start], &text[emote_range.end..]);
        assert_eq!(new_text, "look at this test ");
    }
}
