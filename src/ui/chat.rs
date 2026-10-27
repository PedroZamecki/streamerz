//! The Chat tab:
//! - Every set-up platform's chat together, side by side, or individually
//! - Individual per-pane input bars: `[Account Avatar] [Textbox] [Emotes Icon] [Send Icon]`
//! - Account selector popup on avatar click (switches Broadcaster vs Bot per platform)
//! - Discrete `"Message all"` button & popup in the top bar
//! - Individual 7TV-style emote & emoji picker per platform and account, with search, provider tabs,
//!   collapsible sections, and right-click `★` favorites
//! - Platform-authentic badges & username colors, plus pre-join history before `"Welcome to the chat room!"`

use super::accounts::{platform_accounts, unlock_logins_interactive};
use super::emotes::{self, EmoteSection, Provider};
use super::img::{self, Pic};
use super::rows::Rows as InfoRows;
use super::{button, query, tell, ROW};
use crate::engine::chatlog::{Line, Moderate, PLATFORMS};
use crate::Config;
use fltk::app;
use fltk::button::{Button, CheckButton};
use fltk::draw;
use fltk::enums::{Align, CallbackTrigger, Color, Event, Font, FrameType, Key};
use fltk::frame::Frame;
use fltk::group::{Flex, Scroll, Wizard};
use fltk::image::SvgImage;
use fltk::input::Input;
use fltk::menu::Choice;
use fltk::prelude::*;
use fltk::valuator::Slider;
use fltk::window::Window;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{mpsc::UnboundedSender, watch};

type Cfg = Arc<watch::Sender<Config>>;
type Cmds = UnboundedSender<crate::engine::UiCmd>;

const NAMES: [&str; 4] = ["Twitch", "YouTube", "Kick", "TikTok"];
/// Text size, emote height, and a line's least height.
const TEXT: i32 = 14;
const EMOTE: i32 = 28;
const LINE: i32 = 22;
/// Lines kept per view (the engine keeps as many).
const KEEP: usize = 500;
/// Badge height, and the gap after each badge.
const BADGE: i32 = 18;
const GAP: i32 = 3;

/// Platform-specific vector badges (each platform has its own visual language; never cross-applied)
/// plus crisp UI icons for buttons and provider tabs.
const ICONS: &[(&str, &str)] = &[
    // Twitch fallback SVGs (used only while Twitch Helix badge PNGs are still downloading)
    (
        "twitch:broadcaster",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 18 18"><rect width="18" height="18" rx="3" fill="#E91916"/><path d="M3.5 6h7.5v6H3.5zM11 8.2l3.5-2.2v6l-3.5-2.2z" fill="#fff"/></svg>"##,
    ),
    (
        "twitch:moderator",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 18 18"><rect width="18" height="18" rx="3" fill="#00AD03"/><path d="M13.5 3.5 7 10l1 1 6.5-6.5zM5 10.5l2.5 2.5-1 1-1-1-1.5 1.5-1-1L4.5 12l-1-1z" fill="#fff"/></svg>"##,
    ),
    (
        "twitch:vip",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 18 18"><rect width="18" height="18" rx="3" fill="#E005B9"/><path d="M9 3.5 14 8l-5 6.5L4 8z" fill="#fff"/></svg>"##,
    ),
    (
        "twitch:subscriber",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 18 18"><rect width="18" height="18" rx="3" fill="#8205B4"/><path d="m9 3.2 1.7 3.6 3.9.4-2.9 2.7.8 3.9L9 11.8l-3.5 2 .8-3.9-2.9-2.7 3.9-.4z" fill="#fff"/></svg>"##,
    ),
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
        "ui:user",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20"><circle cx="10" cy="10" r="9" fill="#3A3D45"/><circle cx="10" cy="7.5" r="3.2" fill="#D8DCE6"/><path d="M4.5 16.2c1-2.8 3.2-4.2 5.5-4.2s4.5 1.4 5.5 4.2" fill="#D8DCE6"/></svg>"##,
    ),
    (
        "ui:emotes",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20"><circle cx="10" cy="10" r="8" fill="none" stroke="#DCDFE6" stroke-width="1.7"/><circle cx="7.2" cy="8.2" r="1.2" fill="#DCDFE6"/><circle cx="12.8" cy="8.2" r="1.2" fill="#DCDFE6"/><path d="M6.5 11.8c.9 1.9 2.2 2.7 3.5 2.7s2.6-.8 3.5-2.7" fill="none" stroke="#DCDFE6" stroke-width="1.7" stroke-linecap="round"/></svg>"##,
    ),
    (
        "ui:send",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20"><path d="M2.5 3.5 17.8 10 2.5 16.5 5.2 10z" fill="#5CA8FF"/><path d="M5.2 10h6.8" stroke="#1E1E20" stroke-width="1.4" stroke-linecap="round"/></svg>"##,
    ),
    (
        "ui:broadcast",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20"><path d="M3 5.5h11a1.5 1.5 0 0 1 1.5 1.5v5a1.5 1.5 0 0 1-1.5 1.5H7l-3.2 2.5V13.5H3A1.5 1.5 0 0 1 1.5 12V7A1.5 1.5 0 0 1 3 5.5z" fill="none" stroke="#C8CCD4" stroke-width="1.5"/><path d="M17.2 7.2h1.3v5.5h-4.5" fill="none" stroke="#5CA8FF" stroke-width="1.5" stroke-linecap="round"/></svg>"##,
    ),
    (
        "ui:star",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20"><path d="m10 2.3 2.3 4.7 5.2.8-3.8 3.6.9 5.2L10 14.1l-4.6 2.5.9-5.2-3.8-3.6 5.2-.8z" fill="#F5C542"/></svg>"##,
    ),
    // Message actions, shown on hover.
    (
        "ui:reply",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20"><path d="M8 5 3 10l5 5M3.5 10H12a5 5 0 0 1 5 5v1" fill="none" stroke="#C8CCD4" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/></svg>"##,
    ),
    (
        "ui:copy",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20"><rect x="7" y="7" width="10" height="10" rx="2" fill="none" stroke="#C8CCD4" stroke-width="1.7"/><path d="M13 4.5V4a1.5 1.5 0 0 0-1.5-1.5H4A1.5 1.5 0 0 0 2.5 4v7.5A1.5 1.5 0 0 0 4 13h.5" fill="none" stroke="#C8CCD4" stroke-width="1.7"/></svg>"##,
    ),
    (
        "ui:pin",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20"><path d="M7 2.5h6M8 2.5v5L5.5 11h9L12 7.5v-5M10 11v6.5" fill="none" stroke="#C8CCD4" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"/></svg>"##,
    ),
    (
        "ui:delete",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20"><path d="M3 5.5h14M8 5.5V3.5h4v2M5 5.5l.8 11h8.4l.8-11M8.3 9v5M11.7 9v5" fill="none" stroke="#C8CCD4" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"/></svg>"##,
    ),
    (
        "ui:timeout",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20"><circle cx="10" cy="10" r="7.2" fill="none" stroke="#C8CCD4" stroke-width="1.7"/><path d="M10 5.8V10l3 2" fill="none" stroke="#C8CCD4" stroke-width="1.7" stroke-linecap="round"/></svg>"##,
    ),
    (
        "ui:ban",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20"><circle cx="10" cy="10" r="7.2" fill="none" stroke="#F06A6A" stroke-width="1.8"/><path d="m5 15 10-10" stroke="#F06A6A" stroke-width="1.8"/></svg>"##,
    ),
    (
        "ui:search",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20"><circle cx="8.8" cy="8.8" r="5.5" fill="none" stroke="#9AA0AA" stroke-width="1.8"/><path d="m13 13 4.2 4.2" stroke="#9AA0AA" stroke-width="1.8" stroke-linecap="round"/></svg>"##,
    ),
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
fn icon_sized(key: &str, size: i32) -> Option<SvgImage> {
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

/// Map a badge key on `platform` to its platform-specific vector SVG fallback when no image URL is set.
fn badge_fallback_svg(platform: &str, raw_key: &str) -> Option<SvgImage> {
    if raw_key.contains(':') {
        if let Some(s) = svg(raw_key) {
            return Some(s);
        }
    }
    let base = raw_key.split('/').next().unwrap_or(raw_key).to_lowercase();
    let mapped = match (platform, base.as_str()) {
        ("twitch", "broadcaster") => "twitch:broadcaster",
        ("twitch", "moderator" | "lead_moderator") => "twitch:moderator",
        ("twitch", "vip") => "twitch:vip",
        ("twitch", "subscriber" | "founder") => "twitch:subscriber",
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

/// One thing on a line.
enum It {
    Text(String, Font, Color),
    Pic(Pic),
    Svg(SvgImage),
    /// A word's space: where the line may wrap.
    Space,
    /// A few pixels between pictures (no wrap).
    Gap(i32),
}

impl It {
    fn size(&self) -> (i32, i32) {
        match self {
            It::Text(t, f, _) => {
                draw::set_font(*f, TEXT);
                (draw::width(t) as i32, TEXT + 4)
            }
            It::Pic(p) => (p.w(), p.h().max(1)),
            It::Svg(s) => (s.w(), s.h()),
            It::Space => {
                draw::set_font(Font::Helvetica, TEXT);
                (draw::width(" ") as i32, 0)
            }
            It::Gap(w) => (*w, 0),
        }
    }
}

thread_local! {
    /// What lines show: (the time, the badges). Chat's options; all panes follow.
    static SHOW: Cell<(bool, bool)> = const { Cell::new((true, true)) };
}

/// A line's things, in order.
fn items(cmds: &Cmds, l: &Line, tag: bool) -> Vec<It> {
    let gray = Color::from_rgb(0x88, 0x88, 0x88);
    let welcome_col = Color::from_rgb(0x9e, 0xa3, 0xb0);
    let (time, badges) = SHOW.get();
    let mut v = Vec::new();
    // "Welcome to the chat room!" is rendered cleanly without a timestamp, matching official chats.
    if l.welcome {
        if tag {
            let p = PLATFORMS.iter().position(|x| *x == l.platform).unwrap_or(0);
            if let Some(s) = svg(NAMES[p]) {
                v.push(It::Svg(s));
                v.push(It::Space);
            }
        }
        v.push(It::Text(l.text.clone(), Font::Helvetica, welcome_col));
        return v;
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
    if badges {
        let before = v.len();
        if l.badges.is_empty() {
            // Role fallback using strictly this platform's own badge design.
            let mods = l.moderator && !l.broadcaster;
            for (role, on) in [
                ("broadcaster", l.broadcaster),
                ("moderator", mods),
                ("vip", l.vip),
                ("subscriber", l.subscriber),
            ] {
                if on {
                    if let Some(s) = badge_fallback_svg(&l.platform, role) {
                        v.push(It::Svg(s));
                        v.push(It::Gap(GAP));
                    }
                }
            }
        } else {
            for (name, url) in &l.badges {
                if !url.is_empty() {
                    if let Some(p) = img::get(cmds, url, BADGE, false) {
                        v.push(It::Pic(p));
                        v.push(It::Gap(GAP));
                        continue;
                    }
                }
                if let Some(s) = badge_fallback_svg(&l.platform, name) {
                    v.push(It::Svg(s));
                    v.push(It::Gap(GAP));
                }
            }
        }
        if v.len() > before {
            v.pop();
            v.push(It::Space);
        }
    }
    v.push(It::Text(format!("{}:", l.user), Font::HelveticaBold, name_color(l)));
    let fg = Color::Foreground;
    let pic = |url: &str| img::get(cmds, url, EMOTE, false);
    for w in l.text.split(' ') {
        v.push(It::Space);
        let known = l.emotes.iter().find(|e| e.0 == w).map(|e| e.1.clone());
        if let Some(p) = known.or_else(|| emotes::third(&l.platform, w)).and_then(|u| pic(&u)) {
            v.push(It::Pic(p));
            continue;
        }
        for (part, url) in emotes::split(w) {
            match url.and_then(|u| pic(&u)) {
                Some(p) => v.push(It::Pic(p)),
                None => v.push(It::Text(part.to_string(), Font::Helvetica, fg)),
            }
        }
    }
    v
}

/// A line laid out: its rows (items with their x, and the row's height).
type Rows = Vec<(Vec<(It, i32)>, i32)>;

/// Lay a line out in `w` px.
fn rows(items: Vec<It>, w: i32) -> Rows {
    let mut out: Rows = vec![(Vec::new(), LINE)];
    let mut x = 0;
    for it in items {
        let (iw, ih) = it.size();
        if matches!(it, It::Space | It::Gap(_)) {
            x += iw;
            continue;
        }
        let row = out.last_mut().expect("a row");
        if x + iw > w && !row.0.is_empty() {
            out.push((Vec::new(), LINE));
            x = 0;
        }
        let row = out.last_mut().expect("a row");
        row.1 = row.1.max(ih + 2);
        row.0.push((it, x));
        x += iw;
    }
    out
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
    /// An animation frame is due.
    ticking: bool,
    /// The line under the pointer, which shows its actions.
    hover: Option<usize>,
}

/// A message's hover actions.
#[derive(Clone, Copy)]
enum Act {
    Reply,
    Copy,
    Mod(Moderate),
}

const ACT: i32 = 24;

/// The actions `l` offers: moderation only where the platform has it, and not on the broadcaster.
fn acts(l: &Line) -> Vec<(Act, &'static str, &'static str)> {
    if l.note {
        return Vec::new();
    }
    let mut v = vec![(Act::Reply, "ui:reply", "Reply"), (Act::Copy, "ui:copy", "Copy")];
    let p = l.platform.as_str();
    if p == "twitch" && !l.msg_id.is_empty() {
        v.push((Act::Mod(Moderate::Pin), "ui:pin", "Pin"));
    }
    if matches!(p, "twitch" | "kick") && !l.msg_id.is_empty() {
        v.push((Act::Mod(Moderate::Delete), "ui:delete", "Delete message"));
    }
    if matches!(p, "twitch" | "kick" | "youtube") && !l.broadcaster && !l.user_id.is_empty() {
        v.push((Act::Mod(Moderate::Timeout(0)), "ui:timeout", "Timeout"));
        v.push((Act::Mod(Moderate::Ban), "ui:ban", "Ban"));
    }
    v
}

/// One chat view: the lines, drawn, with a scrollbar.
#[derive(Clone)]
struct Pane {
    area: Frame,
    bar: Slider,
    st: Rc<RefCell<State>>,
    cmds: Cmds,
    /// Where Reply writes the mention.
    input: Rc<RefCell<Option<Input>>>,
}

impl Pane {
    fn new(cmds: &Cmds) -> (Flex, Pane) {
        let mut group = Flex::default().row();
        group.set_pad(0);
        let mut area = Frame::default();
        area.set_frame(FrameType::DownBox);
        area.set_color(Color::Background2);
        // A slider has no arrows: drawn over as a thin rounded bar, like the lists'.
        let mut bar = Slider::default();
        bar.draw(|b| {
            let (x, y, w, h) = (b.x(), b.y(), b.w(), b.h());
            draw::draw_rect_fill(x, y, w, h, Color::Background);
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
                ticking: false,
                hover: None,
            })),
            cmds: cmds.clone(),
            input: Rc::default(),
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
        st.laid.iter().map(|l| l.1).sum::<i32>() + 8
    }

    fn lay(&self, l: &Line, tag: bool, w: i32) -> (Rows, i32) {
        let r = rows(items(&self.cmds, l, tag), w);
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
        self.measure(st);
        let (total, h) = (self.total(st), self.area.h());
        let max = (total - h).max(0);
        if st.follow {
            st.top = max;
        }
        st.top = st.top.clamp(0, max);
        let mut bar = self.bar.clone();
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
        let mut st = self.st.borrow_mut();
        self.settle(&mut st);
        draw::draw_box(f.frame(), f.x(), f.y(), f.w(), f.h(), f.color());
        draw::push_clip(f.x() + 2, f.y() + 2, f.w() - 4, f.h() - 4);
        let (ms, mut moving) = (now_ms(), false);
        let mut y = f.y() + 4 - st.top;
        for (i, (laid, h)) in st.laid.iter().enumerate() {
            if y + h < f.y() {
                y += h;
                continue;
            }
            if y > f.y() + f.h() {
                break;
            }
            if st.hover == Some(i) {
                draw::draw_rect_fill(f.x() + 2, y, f.w() - 4, *h, Color::from_rgb(0x2c, 0x2e, 0x34));
            }
            for (row, rh) in laid {
                let rh = *rh;
                for (it, x) in row {
                    let x = f.x() + 6 + x;
                    match it {
                        It::Text(t, font, color) => {
                            draw::set_font(*font, TEXT);
                            draw::set_draw_color(*color);
                            draw::draw_text2(t, x, y, 0, rh, Align::Left);
                        }
                        It::Pic(p) => {
                            moving |= p.animated();
                            let mut i = p.at(ms).clone();
                            i.draw(x, y + (rh - i.h()) / 2, i.w(), i.h());
                        }
                        It::Svg(s) => {
                            let (w, h) = (s.w(), s.h());
                            s.clone().draw(x, y + (rh - h) / 2, w, h);
                        }
                        It::Space | It::Gap(_) => {}
                    }
                }
                y += rh;
            }
        }
        if let Some((l, bx, by)) = st.hover.and_then(|i| self.bar_at(f, &st, i)) {
            let a = acts(l);
            let w = a.len() as i32 * ACT + 4;
            draw::draw_box(
                FrameType::RFlatBox,
                bx - 2,
                by - 2,
                w,
                ACT + 4,
                Color::from_rgb(0x3a, 0x3d, 0x45),
            );
            for (k, (_, icon, _)) in a.iter().enumerate() {
                if let Some(mut s) = icon_sized(icon, 16) {
                    s.draw(bx + k as i32 * ACT + 4, by + 4, 16, 16);
                }
            }
        }
        draw::pop_clip();
        // Animated emotes on screen: one redraw ~20 times a second, only while this is drawn.
        if moving && !st.ticking {
            st.ticking = true;
            let p = self.clone();
            app::add_timeout3(0.05, move |_| {
                p.st.borrow_mut().ticking = false;
                p.area.clone().redraw();
            });
        }
    }

    /// The line under the pointer's y.
    fn line_at(&self, f: &Frame, st: &State) -> Option<usize> {
        let mut y = f.y() + 4 - st.top;
        for (i, (_, h)) in st.laid.iter().enumerate() {
            if (y..y + h).contains(&app::event_y()) {
                return Some(i);
            }
            y += h;
        }
        None
    }

    /// Line `i` and where its action bar goes: at its top right, kept inside the view.
    fn bar_at<'a>(&self, f: &Frame, st: &'a State, i: usize) -> Option<(&'a Line, i32, i32)> {
        let l = st.lines.get(i)?;
        let n = acts(l).len() as i32;
        if n == 0 {
            return None;
        }
        let y = f.y() + 4 - st.top + st.laid.iter().take(i).map(|l| l.1).sum::<i32>();
        let by = (y - ACT / 2).clamp(f.y() + 4, f.y() + f.h() - ACT - 4);
        Some((l, f.x() + f.w() - n * ACT - 10, by))
    }

    /// The action under the pointer, on the hovered line: the line and its `acts` entry.
    fn act_at(&self, f: &Frame) -> Option<(Line, (Act, &'static str, &'static str))> {
        let st = self.st.borrow();
        let (l, bx, by) = self.bar_at(f, &st, st.hover?)?;
        let (ex, ey) = (app::event_x(), app::event_y());
        if !(by..by + ACT).contains(&ey) || ex < bx {
            return None;
        }
        let a = *acts(l).get(((ex - bx) / ACT) as usize)?;
        Some((l.clone(), a))
    }

    fn run(&self, l: &Line, a: Act) {
        match a {
            Act::Reply => {
                if let Some(i) = self.input.borrow_mut().as_mut() {
                    let v = format!("@{} {}", l.user, i.value().trim_start());
                    i.set_value(&v);
                    let _ = i.take_focus();
                    let _ = i.set_position(v.len() as i32);
                }
            }
            Act::Copy => app::copy(&l.text),
            Act::Mod(Moderate::Timeout(_)) => {
                let menu = fltk::menu::MenuItem::new(&["1 minute", "10 minutes", "1 hour", "1 day", "1 week"]);
                if let Some(item) = menu.popup(app::event_x(), app::event_y()) {
                    let secs = match item.label().unwrap_or_default().as_str() {
                        "1 minute" => 60,
                        "10 minutes" => 600,
                        "1 hour" => 3600,
                        "1 day" => 86_400,
                        _ => 604_800,
                    };
                    let l = l.clone();
                    tell(&self.cmds, move |e| e.moderate(&l, Moderate::Timeout(secs)));
                }
            }
            Act::Mod(Moderate::Ban) => {
                let q = format!("Ban {} from {}?", l.user, l.platform).replace('@', "@@");
                if fltk::dialog::choice2_default(&q, "Ban", "Cancel", "") == Some(0) {
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
        let mut y = f.y() + 4 - st.top;
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

    fn handle(&self, f: &mut Frame, ev: Event) -> bool {
        match ev {
            Event::Enter => true,
            Event::Move => {
                let hover = {
                    let st = self.st.borrow();
                    self.line_at(f, &st)
                };
                if self.st.borrow().hover != hover {
                    self.st.borrow_mut().hover = hover;
                    f.redraw();
                }
                let act = self.act_at(f);
                f.set_tooltip(act.as_ref().map_or("", |a| a.1 .2));
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
                if self.st.borrow_mut().hover.take().is_some() {
                    f.redraw();
                }
                if let Some(mut w) = f.window() {
                    w.set_cursor(fltk::enums::Cursor::Default);
                }
                true
            }
            Event::Push if app::event_mouse_button() == app::MouseButton::Left => {
                if let Some((l, a)) = self.act_at(f) {
                    self.run(&l, a.0);
                } else if let Some(l) = self.name_at(f) {
                    card(&self.cmds, &l);
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

    fn add(&self, l: &Line) {
        let mut st = self.st.borrow_mut();
        if st.lines.len() == KEEP {
            st.lines.pop_front();
            st.laid.pop_front();
            st.hover = st.hover.and_then(|h| h.checked_sub(1));
        }
        st.lines.push_back(l.clone());
        if st.width > 0 && st.laid.len() + 1 == st.lines.len() {
            let laid = self.lay(l, st.tag, st.width);
            st.laid.push_back(laid);
        }
        drop(st);
        self.area.clone().redraw();
    }

    fn replace_all(&self, lines: impl Iterator<Item = Line>) {
        let mut st = self.st.borrow_mut();
        st.lines.clear();
        st.hover = None;
        for l in lines {
            if st.lines.len() == KEEP {
                st.lines.pop_front();
            }
            st.lines.push_back(l);
        }
        st.laid.clear();
        st.width = -1;
        drop(st);
        self.area.clone().redraw();
    }

    fn clear(&self) {
        let mut st = self.st.borrow_mut();
        st.lines.clear();
        st.laid.clear();
        st.hover = None;
        st.top = 0;
        st.follow = true;
        drop(st);
        self.area.clone().redraw();
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
    pane: Pane,
    acct_btn: Button,
    input: Input,
    target_plat: Rc<Cell<usize>>,
    allow_switch_plat: Rc<Cell<bool>>,
}

impl ChatColumn {
    fn new(cfg: &Cfg, cmds: &Cmds, fixed_plat: Option<usize>, go_accounts: Rc<dyn Fn()>) -> Self {
        let mut col = Flex::default().column();
        col.set_pad(4);
        let (_pane_group, pane) = Pane::new(cmds);

        let mut bar = Flex::default().row();
        bar.set_pad(4);

        let mut acct_btn = Button::default();
        acct_btn.set_frame(FrameType::FlatBox);
        acct_btn.set_image(icon_sized("ui:user", 20));
        bar.fixed(&acct_btn, 32);

        let mut input = Input::default();
        super::placeholder(&mut input, "Send a message");
        pane.input.replace(Some(input.clone()));

        let mut emote_btn = Button::default();
        emote_btn.set_frame(FrameType::FlatBox);
        emote_btn.set_image(icon_sized("ui:emotes", 18));
        emote_btn.set_tooltip("Emotes & emoji for this platform and account");
        bar.fixed(&emote_btn, 32);

        let mut send_btn = Button::default();
        send_btn.set_frame(FrameType::FlatBox);
        send_btn.set_image(icon_sized("ui:send", 18));
        send_btn.set_tooltip("Send message (Enter)");
        bar.fixed(&send_btn, 32);

        bar.end();
        col.fixed(&bar, ROW + 4);
        col.end();

        let init_idx = fixed_plat.unwrap_or_else(|| {
            let saved = cfg.borrow().window.chat_all_platform.clone();
            PLATFORMS.iter().position(|p| *p == saved).unwrap_or(0)
        });
        let target_plat = Rc::new(Cell::new(init_idx));
        let allow_switch_plat = Rc::new(Cell::new(fixed_plat.is_none()));

        let me = Self {
            col,
            pane,
            acct_btn: acct_btn.clone(),
            input: input.clone(),
            target_plat: target_plat.clone(),
            allow_switch_plat: allow_switch_plat.clone(),
        };

        let do_send = {
            let (cfg, cmds, input, target_plat) = (cfg.clone(), cmds.clone(), input.clone(), target_plat.clone());
            move || {
                let text = input.value().trim().to_string();
                if text.is_empty() {
                    return;
                }
                let p_idx = target_plat.get().min(3);
                let plat = PLATFORMS[p_idx];
                let want_broadcaster = cfg.borrow().window.chat_as_broadcaster.iter().any(|s| s == plat);
                let use_bot = !want_broadcaster;
                tell(&cmds, move |e| {
                    let _ = e.send_chat_list(&[plat], &text, use_bot, true);
                });
                input.clone().set_value("");
            }
        };

        send_btn.set_callback({
            let s = do_send.clone();
            move |_| s()
        });
        me.input.clone().handle({
            let s = do_send;
            move |_, ev| {
                if ev == Event::KeyDown && app::event_key() == Key::Enter {
                    s();
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

    fn refresh_sender(&self, cfg: &Cfg, cmds: &Cmds) {
        let p_idx = self.target_plat.get().min(3);
        let plat_name = NAMES[p_idx];
        let (use_bot, sender_name, pic) = effective_sender_role(cfg, cmds, p_idx);
        let mut btn = self.acct_btn.clone();
        match pic {
            Some(img) => btn.set_image(Some(img)),
            None => btn.set_image(icon_sized("ui:user", 20)),
        }
        let role_str = if use_bot { "Bot" } else { "Broadcaster" };
        let tip = match &sender_name {
            Some(name) => format!(
                "{plat_name} — sending as {} ({role_str}). Click to info / switch account",
                name.replace('@', "@@")
            ),
            None => format!("{plat_name} — no account logged in. Click for account options"),
        };
        btn.set_tooltip(&tip);
        let mut inp = self.input.clone();
        let placeholder_tip = match &sender_name {
            Some(name) => format!("Message {plat_name} as {} ({role_str})", name.replace('@', "@@")),
            None => format!("Message {plat_name}"),
        };
        inp.set_tooltip(&placeholder_tip);
        btn.redraw();
    }
}

pub struct Chat {
    pub tab: Flex,
    cfg: Cfg,
    cmds: Cmds,
    view: Choice,
    /// The set-up platforms (indexes in PLATFORMS), as the view list shows them.
    plats: Rc<RefCell<Vec<usize>>>,
    /// All together / single platform column, plus one individual column per platform for side by side.
    main: ChatColumn,
    side: Vec<ChatColumn>,
    client_gen: u64,
    seen: u64,
}

impl Chat {
    /// Sync new lines (or full history resync when pre-join history / badges updated) and refresh avatars.
    pub fn refresh(&mut self) {
        emotes::load_for_config(&self.cmds, &self.cfg);
        let plats = shown_platforms(&self.cmds);
        if *self.plats.borrow() != plats {
            *self.plats.borrow_mut() = plats;
            self.lists();
            self.rebuild(true);
            return;
        }
        self.refresh_senders();
        let (cgen, seen) = (self.client_gen, self.seen);
        let Some((new_gen, full, lines)) = query(&self.cmds, move |e| e.chat_sync(cgen, seen)) else {
            return;
        };
        self.client_gen = new_gen;
        let v = self.view.value();
        if full {
            self.seen = lines.last().map_or(0, |l| l.seq);
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
    }

    fn refresh_senders(&self) {
        if self.view.value() == 1 {
            for col in &self.side {
                col.refresh_sender(&self.cfg, &self.cmds);
            }
        } else {
            if let Some(shown_slug) = self.shown() {
                if let Some(idx) = PLATFORMS.iter().position(|p| *p == shown_slug) {
                    self.main.target_plat.set(idx);
                    self.main.allow_switch_plat.set(false);
                }
            } else {
                let saved = self.cfg.borrow().window.chat_all_platform.clone();
                let plats = self.plats.borrow();
                let idx = PLATFORMS
                    .iter()
                    .position(|p| *p == saved)
                    .filter(|i| plats.is_empty() || plats.contains(i))
                    .or_else(|| plats.first().copied())
                    .unwrap_or(0);
                self.main.target_plat.set(idx);
                self.main.allow_switch_plat.set(true);
            }
            self.main.refresh_sender(&self.cfg, &self.cmds);
        }
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
            &["All together", "Side by side"]
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
    let l = Frame::default().with_label("Show:");
    top.fixed(&l, 46);
    let mut view = Choice::default();
    top.fixed(&view, 136);
    let mut hint = Frame::default()
        .with_label("Click a name for its info, hover a line for actions")
        .with_align(Align::Right | Align::Inside | Align::Clip);
    hint.set_label_size(12);
    hint.set_label_color(super::muted());

    // Discrete "Message all" button with broadcast icon
    let mut msg_all = Button::default()
        .with_label(" Message all")
        .with_align(Align::Inside | Align::ImageNextToText);
    msg_all.set_image(icon_sized("ui:broadcast", 15));
    msg_all.set_tooltip("Open a popup to send a message to multiple platforms at once");
    top.fixed(&msg_all, 114);

    let mut opts = fltk::menu::MenuButton::default().with_label("Options");
    top.fixed(&opts, 84);
    let mut clear = button(&mut top, "Clear", 64);
    top.end();
    col.fixed(&top, ROW);

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
    for (idx, name) in NAMES.into_iter().enumerate() {
        let mut p = ChatColumn::new(cfg, cmds, Some(idx), go_accounts.clone());
        p.col.set_label(&format!("  {name}"));
        p.col.set_image(img::logo(name, 16));
        p.col.set_align(Align::Top | Align::ImageNextToText);
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
        plats: Rc::default(),
        main,
        side,
        client_gen: 0,
        seen: 0,
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
    panes.set_margins(0, if view.value() == 1 { 20 } else { 0 }, 0, 0);

    view.set_callback({
        let me = me.clone();
        let mut panes = panes.clone();
        let cfg = cfg.clone();
        move |v| {
            let view = v.choice().unwrap_or_default();
            cfg.send_if_modified(|c| {
                c.window.chat_view = view;
                false
            });
            panes.set_margins(0, if v.value() == 1 { 20 } else { 0 }, 0, 0);
            me.borrow_mut().rebuild(true);
        }
    });

    msg_all.set_callback({
        let (cfg, cmds) = (cfg.clone(), cmds.clone());
        move |_| message_all_popup(&cfg, &cmds)
    });

    {
        let w = &cfg.borrow().window;
        SHOW.set((!w.chat_no_time, !w.chat_no_badges));
    }
    let (time, badges) = SHOW.get();
    for (i, (label, on)) in [("Time", time), ("Badges", badges)].into_iter().enumerate() {
        let flag = if on {
            fltk::menu::MenuFlag::Toggle | fltk::menu::MenuFlag::Value
        } else {
            fltk::menu::MenuFlag::Toggle
        };
        let (me, cfg) = (me.clone(), cfg.clone());
        opts.add(label, fltk::enums::Shortcut::None, flag, move |m| {
            let on = m.at(i as i32).is_some_and(|x| x.value());
            let (mut time, mut badges) = SHOW.get();
            if i == 0 {
                time = on;
            } else {
                badges = on;
            }
            SHOW.set((time, badges));
            cfg.send_if_modified(|c| {
                (c.window.chat_no_time, c.window.chat_no_badges) = (!time, !badges);
                false
            });
            me.borrow().images();
        });
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
    let switch_plat = col_ui.allow_switch_plat.get();
    let cur_p = col_ui.target_plat.get().min(3);
    let h = 236 + i32::from(switch_plat) * 38 + i32::from(crate::secrets::is_locked()) * 34;
    let mut win = Window::default().with_size(430, h).with_label("Sending Account");
    win.set_xclass("streamerz");
    win.make_modal(true);

    let mut root = Flex::new(10, 10, 410, h - 20, None).column();
    root.set_pad(8);

    if crate::secrets::is_locked() {
        let mut lrow = Flex::default().row();
        lrow.set_pad(8);
        let mut lbl = Frame::default()
            .with_label("🔒 Keyring locked — unlock saved logins?")
            .with_align(Align::Left | Align::Inside);
        lbl.set_label_color(Color::from_rgb(0xe0, 0xa8, 0x30));
        let mut ubtn = button(&mut lrow, "Unlock", 80);
        lrow.end();
        root.fixed(&lrow, ROW);
        let cmds_c = cmds.clone();
        let mut win_c = win.clone();
        ubtn.set_callback(move |_| {
            unlock_logins_interactive(&cmds_c);
            win_c.hide();
        });
    }

    let mut plat_choice_opt = None;
    if switch_plat {
        let mut prow = Flex::default().row();
        prow.set_pad(8);
        let lbl = Frame::default()
            .with_label("Send to platform:")
            .with_align(Align::Left | Align::Inside);
        prow.fixed(&lbl, 130);
        let mut ch = Choice::default();
        ch.add_choice(&NAMES.join("|"));
        ch.set_value(cur_p as i32);
        prow.end();
        root.fixed(&prow, ROW);
        plat_choice_opt = Some(ch);
    }

    let mut head = Flex::default().row();
    head.set_pad(8);
    let head_logo = Frame::default();
    head.fixed(&head_logo, 24);
    let mut head_title = Frame::default().with_align(Align::Left | Align::Inside | Align::Clip);
    head_title.set_label_font(Font::HelveticaBold);
    head.end();
    root.fixed(&head, ROW);

    // Two account cards: [0] Broadcaster, [1] Bot
    let mut slot_uis = Vec::new();
    for _ in 0..2 {
        let mut row = Flex::default().row();
        row.set_pad(8);
        let pic = Frame::default();
        row.fixed(&pic, 36);
        let info = Frame::default().with_align(Align::Left | Align::Inside | Align::Wrap);
        let mut pick_btn = button(&mut row, "Select", 96);
        pick_btn.set_frame(FrameType::RFlatBox);
        row.end();
        root.fixed(&row, 48);
        slot_uis.push((pic, info, pick_btn));
    }

    let note = Frame::default()
        .with_label("Emote picker automatically switches to the selected account's unlocked emotes.")
        .with_align(Align::Left | Align::Inside | Align::Wrap);
    root.fixed(&note, 28);

    let mut bottom = Flex::default().row();
    bottom.set_pad(8);
    let mut manage = button(&mut bottom, "Manage in Accounts…", 160);
    Frame::default();
    let mut close = button(&mut bottom, "Close", 80);
    bottom.end();
    root.fixed(&bottom, ROW);
    root.end();
    win.end();

    let done = Rc::new(Cell::new(false));
    let mut populate = {
        let (cfg, cmds) = (cfg.clone(), cmds.clone());
        let mut head_logo = head_logo.clone();
        let mut head_title = head_title.clone();
        let slot_uis: Vec<_> = slot_uis.clone();
        let target_plat = col_ui.target_plat.clone();
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
            head_logo.set_image(img::logo(pname, 18));
            let st_txt = if conn {
                "● Connected"
            } else if standby {
                "◐ Standby (offline)"
            } else {
                "○ Not connected"
            };
            head_title.set_label(&format!("{pname}  —  {st_txt}"));

            let want_broadcaster = cfg.borrow().window.chat_as_broadcaster.iter().any(|s| s == slug);
            let slots = platform_accounts(&cmds, &cfg, slug, 32);
            let (eff_bot, _, _) = effective_sender_role(&cfg, &cmds, p_idx);

            for (bot_i, ((mut pic_f, mut info_f, mut btn), slot)) in
                slot_uis.iter().cloned().zip(slots.iter()).enumerate()
            {
                let is_bot = bot_i == 1;
                let role = if is_bot { "Bot" } else { "Broadcaster" };
                let is_active = eff_bot == is_bot && slot.is_some();
                let is_pref = want_broadcaster == !is_bot;
                match slot {
                    Some(s) => {
                        match &s.pic {
                            Some(im) => pic_f.set_image(Some(im.clone())),
                            None => pic_f.set_image(icon_sized("ui:user", 28)),
                        }
                        let label = match s.note {
                            Some(n) => format!("{role}: {} ({n})", s.name.replace('@', "@@")),
                            None => format!("{role}: {}", s.name.replace('@', "@@")),
                        };
                        info_f.set_label(&label);
                        btn.activate();
                        if is_active {
                            btn.set_label("✓ Active");
                            btn.set_color(Color::from_rgb(0x2e, 0x6b, 0x47));
                        } else {
                            btn.set_label("Use account");
                            btn.set_color(Color::Background);
                        }
                    }
                    None => {
                        pic_f.set_image(icon_sized("ui:user", 28));
                        let suffix = if is_pref { " (preferred, falls back)" } else { "" };
                        info_f.set_label(&format!("{role}: not logged in{suffix}"));
                        btn.set_label("Not logged in");
                        btn.set_color(Color::Background);
                        btn.deactivate();
                    }
                }
                pic_f.redraw();
                info_f.redraw();
                btn.redraw();
            }
        }
    };

    if let Some(mut ch) = plat_choice_opt {
        let cfg = cfg.clone();
        let target_plat = col_ui.target_plat.clone();
        let mut populate = populate.clone();
        ch.set_callback(move |c| {
            let idx = c.value().clamp(0, 3) as usize;
            target_plat.set(idx);
            cfg.send_if_modified(|cfg| {
                cfg.window.chat_all_platform = PLATFORMS[idx].to_string();
                false
            });
            populate();
        });
    }

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
            let _ = crate::save(&cfg.borrow());
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
        let _ = app::wait_for(0.1);
    }
    win.hide();
    app::delete_widget(win);
}

/// Popup opened by the discrete `"Message all"` button in the top bar:
/// lets the user pick target platforms, bot/broadcaster + fallback behavior, emotes, and send.
fn message_all_popup(cfg: &Cfg, cmds: &Cmds) {
    let conn_states: [bool; 4] = query(cmds, |e| {
        [
            e.twitch_connected(),
            e.youtube_connected(),
            e.kick_connected(),
            e.tiktok_connected(),
        ]
    })
    .unwrap_or_default();
    let set_up = {
        let c = cfg.borrow();
        [
            c.twitch.broadcaster.is_some() || c.twitch.bot.is_some(),
            c.youtube.broadcaster.is_some() || c.youtube.bot.is_some() || !c.youtube.channel_id.is_empty(),
            c.kick.broadcaster.is_some() || c.kick.bot.is_some() || !c.kick.channel.is_empty(),
            c.tiktok.broadcaster.is_some() || c.tiktok.bot.is_some() || !c.tiktok.channel.is_empty(),
        ]
    };

    let mut win = Window::default()
        .with_size(520, 210)
        .with_label("Message All Platforms");
    win.set_xclass("streamerz");
    win.make_modal(true);
    let mut col = Flex::new(12, 12, 496, 186, None).column();
    col.set_pad(8);

    let title = Frame::default()
        .with_label("Send one message across selected platforms:")
        .with_align(Align::Left | Align::Inside);
    col.fixed(&title, 22);

    let mut plat_row = Flex::default().row();
    plat_row.set_pad(8);
    let mut checks = Vec::new();
    for i in 0..4 {
        let suffix = if conn_states[i] { "" } else { " (offline)" };
        let mut c = CheckButton::default().with_label(&format!("{}{suffix}", NAMES[i]));
        let any_conn = conn_states.iter().any(|&x| x);
        c.set_checked(if any_conn { conn_states[i] } else { set_up[i] });
        if !set_up[i] && !conn_states[i] {
            c.deactivate();
        }
        checks.push(c);
    }
    plat_row.end();
    col.fixed(&plat_row, ROW);

    let mut acct_row = Flex::default().row();
    acct_row.set_pad(8);
    let lbl = Frame::default()
        .with_label("Send as:")
        .with_align(Align::Left | Align::Inside);
    acct_row.fixed(&lbl, 68);
    let mut who = Choice::default();
    who.add_choice("Bot accounts|Broadcaster accounts");
    who.set_value(0);
    acct_row.fixed(&who, 175);
    let fallback = CheckButton::default().with_label("Fall back to other account if missing");
    fallback.set_checked(true);
    acct_row.end();
    col.fixed(&acct_row, ROW);

    let mut bar = Flex::default().row();
    bar.set_pad(4);
    let mut input = Input::default();
    super::placeholder(&mut input, "Send a message to every platform");
    let mut emote_btn = Button::default();
    emote_btn.set_frame(FrameType::FlatBox);
    emote_btn.set_image(icon_sized("ui:emotes", 18));
    emote_btn.set_tooltip("Pick emotes & emoji");
    bar.fixed(&emote_btn, 32);
    let mut send_btn = Button::default();
    send_btn.set_frame(FrameType::FlatBox);
    send_btn.set_image(icon_sized("ui:send", 18));
    send_btn.set_tooltip("Send to all checked platforms (Enter)");
    bar.fixed(&send_btn, 32);
    bar.end();
    col.fixed(&bar, ROW + 4);

    let mut foot = Flex::default().row();
    foot.set_pad(8);
    let status = Frame::default().with_align(Align::Left | Align::Inside);
    let mut close = button(&mut foot, "Cancel", 80);
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
            let targets: Vec<&'static str> = (0..4)
                .filter(|&i| checks[i].is_checked())
                .map(|i| PLATFORMS[i])
                .collect();
            if targets.is_empty() {
                status.set_label("Check at least one platform above.");
                status.set_label_color(Color::from_rgb(0xe0, 0x60, 0x50));
                status.redraw();
                return;
            }
            let use_bot = who.value() == 0;
            let fb = fallback.is_checked();
            tell(&cmds, move |e| {
                let _ = e.send_chat_list(&targets, &text, use_bot, fb);
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
        move |_, ev| {
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
        let _ = app::wait_for(0.1);
    }
    win.hide();
    app::delete_widget(win);
}

fn provider_icon(prov: Provider, platform: &str, size: i32) -> Option<SvgImage> {
    match prov {
        Provider::Favorites => icon_sized("ui:star", size),
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
        Provider::Emoji => icon_sized("ui:emotes", size),
    }
}

fn insert_emote_word(input: &mut Input, word: &str) {
    let mut v = input.value();
    if !v.is_empty() && !v.ends_with(' ') {
        v.push(' ');
    }
    v.push_str(word);
    v.push(' ');
    input.set_value(&v);
    let _ = input.set_position(v.len() as i32);
}

/// 7TV-inspired individual Emote & Emoji picker popup:
/// - Top provider icon tabs (`★ Favorites`, platform native, `7TV`, `BTTV`, `FFZ`, `Emoji`)
/// - Instant search across all providers on this platform & account
/// - Collapsible section headers + empty-state guidance notes
/// - Left-click inserts (Shift+click inserts multiple without closing); right-click toggles `★` favorite
fn emote_picker_popup(
    cfg: &Cfg,
    cmds: &Cmds,
    platform: &'static str,
    as_broadcaster: bool,
    sender_name: Option<&str>,
    target_input: &mut Input,
) {
    emotes::load_for_config(cmds, cfg);
    let seen: Vec<(String, String)> = query(cmds, move |e| {
        let mut v: Vec<(String, String)> = Vec::new();
        for x in PLATFORMS.iter().filter(|x| platform == "all" || **x == platform) {
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
    // Start on Platform native tab (or 7TV on "all", or Favorites if non-empty and preferred)
    let default_tab = providers.iter().position(|&p| p == Provider::Platform).unwrap_or(0);

    let title_plat = match platform {
        "twitch" => "Twitch",
        "kick" => "Kick",
        "youtube" => "YouTube",
        "tiktok" => "TikTok",
        _ => "All Platforms",
    };
    let mut win = Window::default()
        .with_size(400, 490)
        .with_label(&format!("{title_plat} Emotes"));
    win.set_xclass("streamerz");
    win.make_modal(true);

    let mut col = Flex::new(8, 8, 384, 474, None).column();
    col.set_pad(6);

    // 1. Top Provider Icon Tab Bar
    let mut tab_bar = Flex::default().row();
    tab_bar.set_pad(4);
    let mut tab_btns: Vec<(Provider, Button)> = Vec::new();
    for &prov in providers {
        let mut b = Button::default();
        b.set_frame(FrameType::RFlatBox);
        b.set_image(provider_icon(prov, platform, 16));
        b.set_tooltip(prov.label(platform));
        tab_btns.push((prov, b));
    }
    tab_bar.end();
    col.fixed(&tab_bar, 34);

    // 2. Search Bar
    let mut search_row = Flex::default().row();
    search_row.set_pad(4);
    let mut s_icon = Frame::default();
    s_icon.set_image(icon_sized("ui:search", 15));
    search_row.fixed(&s_icon, 24);
    let mut search = Input::default();
    search.set_trigger(CallbackTrigger::Changed);
    search.set_tooltip("Search emotes & emoji across tabs");
    super::placeholder(&mut search, "Search emotes…");
    search_row.end();
    col.fixed(&search_row, ROW);

    // 3. Account & Favorites hint strip
    let role_txt = if as_broadcaster { "Broadcaster" } else { "Bot" };
    let acct_txt = match sender_name {
        Some(n) => format!(
            "{} ({role_txt}) · Right-click to ★ favorite · Shift+click for multiple",
            n.replace('@', "@@")
        ),
        None => "Right-click any emote to ★ favorite · Shift+click for multiple".to_string(),
    };
    let mut sub = Frame::default()
        .with_label(&acct_txt)
        .with_align(Align::Left | Align::Inside | Align::Clip);
    sub.set_label_size(11);
    sub.set_label_color(Color::from_rgb(0x98, 0x9c, 0xa6));
    col.fixed(&sub, 18);

    // 4. Scrollable Collapsible Sections + Grid
    let mut scroll = Scroll::default();
    scroll.set_type(fltk::group::ScrollType::Vertical);
    scroll.end();
    col.end();
    win.end();

    let cur_tab = Rc::new(Cell::new(default_tab));
    let collapsed: Rc<RefCell<HashSet<String>>> = Rc::default();
    let done = Rc::new(Cell::new(false));
    let picked_words: Rc<RefCell<Vec<String>>> = Rc::default();
    let seen = Rc::new(seen);

    // Same size icons, the active one lit: relabeling and resizing them left pieces of old labels.
    let mut style_tabs = {
        let mut tab_bar = tab_bar.clone();
        let tab_btns = tab_btns.clone();
        move |active_idx: usize| {
            for (i, (_, mut b)) in tab_btns.iter().cloned().enumerate() {
                b.set_color(if i == active_idx {
                    Color::from_rgb(0x4a, 0x4f, 0x5a)
                } else {
                    Color::from_rgb(0x26, 0x28, 0x2d)
                });
            }
            tab_bar.redraw();
        }
    };

    // Set by the buttons inside the scroll; the loop below refills. Refilling from their own
    // callback would delete the button running it (that crashed).
    let rebuild_slot: Rc<Cell<bool>> = Rc::default();

    let fill = {
        let (cfg, cmds) = (cfg.clone(), cmds.clone());
        let scroll = scroll.clone();
        let search = search.clone();
        let cur_tab = cur_tab.clone();
        let collapsed = collapsed.clone();
        let seen = seen.clone();
        let done = done.clone();
        let picked_words = picked_words.clone();
        let rebuild_slot = rebuild_slot.clone();
        move || {
            let q = search.value().trim().to_lowercase();
            let favs = cfg.borrow().window.favorite_emotes.clone();
            let active_prov = providers[cur_tab.get().min(providers.len() - 1)];

            // When searching, search across all providers on this platform; otherwise show active provider.
            let secs: Vec<(Provider, EmoteSection)> = if !q.is_empty() {
                let mut all_secs = Vec::new();
                let mut seen_names = HashSet::new();
                for &pv in providers {
                    if pv == Provider::Favorites {
                        continue;
                    }
                    for s in emotes::sections(platform, pv, as_broadcaster, &seen, &favs) {
                        let filtered: Vec<(String, String)> = s
                            .emotes
                            .into_iter()
                            .filter(|(n, _)| n.to_lowercase().contains(&q) && seen_names.insert(n.clone()))
                            .take(200)
                            .collect();
                        if !filtered.is_empty() {
                            all_secs.push((
                                pv,
                                EmoteSection {
                                    title: format!("{} · {}", pv.label(platform), s.title),
                                    note: None,
                                    emotes: filtered,
                                },
                            ));
                        }
                    }
                }
                if all_secs.is_empty() {
                    all_secs.push((
                        active_prov,
                        EmoteSection {
                            title: "Search Results".into(),
                            note: Some(format!("No emotes matching \"{q}\".")),
                            emotes: Vec::new(),
                        },
                    ));
                }
                all_secs
            } else {
                emotes::sections(platform, active_prov, as_broadcaster, &seen, &favs)
                    .into_iter()
                    .map(|s| (active_prov, s))
                    .collect()
            };

            let mut scroll = scroll.clone();
            scroll.clear();
            scroll.begin();
            let avail_w = (scroll.w() - 18).max(180);
            let cell = 46;
            let per = (avail_w / cell).max(1);
            let mut y = scroll.y() + 2;
            let x0 = scroll.x() + 2;

            for (pv, sec) in secs {
                let is_closed = collapsed.borrow().contains(&sec.title);
                let arrow = if is_closed { "▸" } else { "▾" };
                let hdr_txt = if sec.emotes.is_empty() {
                    format!(" {arrow}  {}", sec.title.replace('@', "@@").replace('&', "&&"))
                } else {
                    format!(
                        " {arrow}  {} ({})",
                        sec.title.replace('@', "@@").replace('&', "&&"),
                        sec.emotes.len()
                    )
                };
                let mut hdr = Button::new(x0, y, avail_w, 26, None).with_label(&hdr_txt);
                hdr.set_align(Align::Left | Align::Inside);
                hdr.set_frame(FrameType::FlatBox);
                hdr.set_color(Color::from_rgb(0x28, 0x2a, 0x30));
                hdr.set_label_font(Font::HelveticaBold);
                hdr.set_label_size(13);
                {
                    let collapsed = collapsed.clone();
                    let title = sec.title.clone();
                    let rebuild_slot = rebuild_slot.clone();
                    hdr.set_callback(move |_| {
                        let mut c = collapsed.borrow_mut();
                        if !c.remove(&title) {
                            c.insert(title.clone());
                        }
                        drop(c);
                        rebuild_slot.set(true);
                    });
                }
                y += 30;

                if is_closed {
                    continue;
                }

                if let Some(note_txt) = &sec.note {
                    let mut nf = Frame::new(x0 + 6, y, avail_w - 12, 42, None)
                        .with_label(&note_txt.replace('@', "@@"))
                        .with_align(Align::Center | Align::Inside | Align::Wrap);
                    nf.set_label_size(12);
                    nf.set_label_color(Color::from_rgb(0xa4, 0xa8, 0xb3));
                    y += 46;
                }

                // Cap huge single-section rendering when unfiltered only if extreme (> 500)
                let slice = &sec.emotes[..sec.emotes.len().min(500)];
                for (i, (name, url)) in slice.iter().enumerate() {
                    let cx = x0 + (i as i32 % per) * cell;
                    let cy = y + (i as i32 / per) * cell;
                    let mut b = Button::new(cx, cy, cell - 4, cell - 4, None);
                    let is_fav = favs.iter().any(|(n, _)| n == name);
                    b.set_frame(if is_fav {
                        FrameType::BorderBox
                    } else {
                        FrameType::FlatBox
                    });
                    b.set_color(if is_fav {
                        Color::from_rgb(0x3a, 0x35, 0x22)
                    } else {
                        Color::from_rgb(0x23, 0x24, 0x28)
                    });
                    let tip = if is_fav {
                        format!("★ {} (right-click to unfavorite)", name.replace('@', "@@"))
                    } else {
                        format!("{} (right-click to ★ favorite)", name.replace('@', "@@"))
                    };
                    b.set_tooltip(&tip);
                    // Looked up when drawn: only the visible ones load, and an arrival only redraws.
                    b.draw({
                        let (cmds, url, name) = (cmds.clone(), url.clone(), name.clone());
                        move |b| match img::get(&cmds, &url, EMOTE, false) {
                            Some(p) => {
                                let mut i = p.first();
                                i.draw(b.x() + (b.w() - i.w()) / 2, b.y() + (b.h() - i.h()) / 2, i.w(), i.h());
                            }
                            None => {
                                draw::set_font(Font::Helvetica, 10);
                                draw::set_draw_color(Color::from_rgb(0xa4, 0xa8, 0xb3));
                                draw::draw_text2(&name, b.x(), b.y(), b.w(), b.h(), Align::Center | Align::Clip);
                            }
                        }
                    });

                    let _ = pv;
                    let (cfg_c, name_c, url_c) = (cfg.clone(), name.clone(), url.clone());
                    let (picked_words, done, rebuild_slot) = (picked_words.clone(), done.clone(), rebuild_slot.clone());
                    b.handle(move |_, ev| {
                        if ev == Event::Push {
                            if app::event_mouse_button() == app::MouseButton::Right {
                                cfg_c.send_modify(|c| {
                                    let list = &mut c.window.favorite_emotes;
                                    if let Some(pos) = list.iter().position(|(n, _)| *n == name_c) {
                                        list.remove(pos);
                                    } else {
                                        list.push((name_c.clone(), url_c.clone()));
                                    }
                                });
                                let _ = crate::save(&cfg_c.borrow());
                                rebuild_slot.set(true);
                                return true;
                            }
                            if app::event_mouse_button() == app::MouseButton::Left {
                                picked_words.borrow_mut().push(name_c.clone());
                                if !app::event_state().contains(fltk::enums::Shortcut::Shift) {
                                    done.set(true);
                                }
                                return true;
                            }
                        }
                        false
                    });
                }
                if !slice.is_empty() {
                    let rows_n = (slice.len() as i32 + per - 1) / per;
                    y += rows_n * cell + 6;
                }
            }

            scroll.end();
            scroll.redraw();
        }
    };

    for (idx, (_, mut b)) in tab_btns.into_iter().enumerate() {
        let cur_tab = cur_tab.clone();
        let mut style_tabs = style_tabs.clone();
        let fill = fill.clone();
        let mut scroll = scroll.clone();
        let mut search = search.clone();
        b.set_callback(move |_| {
            cur_tab.set(idx);
            if !search.value().is_empty() {
                search.set_value("");
            }
            style_tabs(idx);
            fill();
            scroll.scroll_to(0, 0);
        });
    }

    search.set_callback({
        let fill = fill.clone();
        let mut scroll = scroll.clone();
        move |_| {
            fill();
            scroll.scroll_to(0, 0);
        }
    });

    win.set_callback({
        let done = done.clone();
        move |_| done.set(true)
    });

    style_tabs(default_tab);
    win.show();
    col.layout();
    fill();
    let _ = search.take_focus();

    let mut pics = img::ARRIVED.load(std::sync::atomic::Ordering::Relaxed);
    let mut seen_rev = emotes::REV.load(std::sync::atomic::Ordering::Relaxed);
    let mut flushed = 0usize;
    while win.shown() && !done.get() {
        let _ = app::wait_for(0.25);
        // Flush any Shift+clicked emotes live into the input box!
        {
            let words = picked_words.borrow();
            while flushed < words.len() {
                insert_emote_word(target_input, &words[flushed]);
                flushed += 1;
            }
        }
        if img::ARRIVED.load(std::sync::atomic::Ordering::Relaxed) != pics {
            pics = img::ARRIVED.load(std::sync::atomic::Ordering::Relaxed);
            scroll.redraw();
        }
        let rev = emotes::REV.load(std::sync::atomic::Ordering::Relaxed);
        if rev != seen_rev || rebuild_slot.replace(false) {
            seen_rev = rev;
            let y = scroll.yposition();
            fill();
            scroll.scroll_to(0, y);
        }
    }
    win.hide();
    app::delete_widget(win);

    let words = picked_words.take();
    while flushed < words.len() {
        insert_emote_word(target_input, &words[flushed]);
        flushed += 1;
    }
    if !words.is_empty() {
        let _ = target_input.take_focus();
    }
}

/// The platforms shown: the connected ones, and ones whose chat is in the log (so its lines stay when
/// it drops), as indexes in PLATFORMS.
fn shown_platforms(cmds: &Cmds) -> Vec<usize> {
    query(cmds, |e| {
        (0..4)
            .filter(|&i| e.chat_connected(PLATFORMS[i]) || e.chat_has(PLATFORMS[i]))
            .collect()
    })
    .unwrap_or_default()
}

/// Label and value rows.
type Facts = Vec<(String, String)>;

/// A chatter's card: what chat said about them this session, then what their platform says (asked
/// when it opens). Double click a row to copy its value.
fn card(cmds: &Cmds, l: &Line) {
    let (p, id) = (l.platform.clone(), l.user_id.clone());
    let seen = query(cmds, move |e| e.chatter(&p, &id)).flatten().unwrap_or_default();
    let platform = NAMES[PLATFORMS.iter().position(|x| *x == l.platform).unwrap_or(0)];
    let mut win = Window::default()
        .with_size(460, 480)
        .with_label(&format!("{} on {platform}", l.user));
    win.set_xclass("streamerz");
    let mut col = Flex::new(10, 10, 440, 460, None).column();
    col.set_pad(8);
    let mut head = Flex::default().row();
    head.set_pad(8);
    let pic = Frame::default();
    head.fixed(&pic, 64);
    let mut name = Frame::default()
        .with_label(&format!("  {}", l.user.replace('@', "@@")))
        .with_align(Align::Left | Align::Inside | Align::ImageNextToText);
    name.set_label_font(Font::HelveticaBold);
    name.set_label_size(20);
    name.set_label_color(name_color(l));
    name.set_image(img::logo(platform, 20));
    head.end();
    col.fixed(&head, 64);
    let mut rows = InfoRows::new("");
    rows.set_column_widths(&[150]);
    let mut bottom = Flex::default().row();
    bottom.set_pad(8);
    Frame::default();
    let mut open = button(&mut bottom, "Open profile", 110);
    let mut close = button(&mut bottom, "Close", 80);
    bottom.end();
    col.fixed(&bottom, ROW);
    col.end();
    win.end();
    win.resizable(&*rows);

    let add = |rows: &mut InfoRows, k: &str, v: &str| {
        rows.add(&format!("{k}\t{}", v.replace(['\n', '\t'], " ")));
    };
    let roles: Vec<&str> = [
        (l.broadcaster, "broadcaster"),
        (l.moderator, "moderator"),
        (l.vip, "VIP"),
        (l.subscriber, "subscriber"),
    ]
    .iter()
    .filter(|r| r.0)
    .map(|r| r.1)
    .collect();
    add(
        &mut rows,
        "Roles",
        &if roles.is_empty() {
            "viewer".into()
        } else {
            roles.join(", ")
        },
    );
    add(&mut rows, "Messages this session", &seen.messages.to_string());
    add(&mut rows, "First message at", &seen.first);
    if !seen.args.contains_key("userId") {
        add(&mut rows, "userId", &l.user_id);
    }
    let s = |v: &serde_json::Value| v.as_str().map_or_else(|| v.to_string(), str::to_string);
    // As the actions get them (%userName%...); "user" is the name above.
    for (k, v) in seen.args.iter().filter(|a| a.0 != "user") {
        add(&mut rows, k, &s(v));
    }
    let asks = matches!(l.platform.as_str(), "twitch" | "kick");
    if asks {
        add(&mut rows, &format!("{platform} says"), "looking up...");
    }
    rows.set_callback(|b| {
        if app::event_clicks() {
            if let Some(t) = b.selected_text() {
                app::copy(t.split_once('\t').map_or(t.as_str(), |x| x.1));
                crate::say("Copied".into());
            }
        }
    });
    let login = seen.args.get("userName").map(s).unwrap_or_else(|| l.user.clone());
    let url: Rc<RefCell<String>> = Rc::new(RefCell::new(match l.platform.as_str() {
        "twitch" => format!("https://twitch.tv/{}", login.to_lowercase()),
        "youtube" => format!("https://youtube.com/channel/{}", l.user_id),
        "kick" => format!("https://kick.com/{}", login.to_lowercase()),
        _ => format!("https://tiktok.com/@{login}"),
    }));
    open.set_callback({
        let url = url.clone();
        move |_| crate::oauth::open_browser(&url.borrow())
    });
    close.set_callback({
        let mut win = win.clone();
        move |_| win.hide()
    });
    win.show();

    let picture = seen
        .args
        .iter()
        .find(|(k, v)| k.contains("rofile") && s(v).starts_with("http"))
        .map(|(_, v)| s(v))
        .unwrap_or_default();
    let picture = Rc::new(RefCell::new(picture));
    let answer: Arc<std::sync::Mutex<Option<Facts>>> = Arc::default();
    if asks {
        let (p, id, answer) = (l.platform.clone(), l.user_id.clone(), answer.clone());
        tell(cmds, move |e| {
            let e = e.clone();
            tokio::task::spawn_local(async move {
                let r = e.chatter_lookup(&p, &id).await;
                if let Ok(mut a) = answer.lock() {
                    *a = Some(r);
                }
            });
        });
    }
    let (cmds, tries) = (cmds.clone(), Rc::new(Cell::new(0)));
    let mut asked = asks;
    let mut pic = pic;
    app::add_timeout3(0.0, move |h| {
        if !win.shown() {
            return;
        }
        tries.set(tries.get() + 1);
        if asked {
            if let Some(found) = answer.lock().ok().and_then(|mut a| a.take()) {
                asked = false;
                rows.remove(rows.size());
                if found.is_empty() {
                    add(&mut rows, &format!("{platform} says"), "nothing (not connected?)");
                }
                for (k, v) in found {
                    match k.as_str() {
                        "Picture" => *picture.borrow_mut() = v,
                        "Channel" => *url.borrow_mut() = format!("https://kick.com/{v}"),
                        _ => add(&mut rows, &k, &v),
                    }
                }
                rows.redraw();
            }
        }
        let mut waiting = asked;
        if pic.image().is_none() && !picture.borrow().is_empty() {
            match img::get(&cmds, &picture.borrow(), 64, true) {
                Some(p) => {
                    pic.set_image(Some(p.first()));
                    pic.redraw();
                }
                None => waiting = true,
            }
        }
        if waiting && tries.get() < 75 {
            app::repeat_timeout3(0.2, h);
        }
    });
}
