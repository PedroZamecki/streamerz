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
use super::fit::Ellipsis;
use super::img::{self, Pic};
use super::rows::Rows as InfoRows;
use super::{button, icons, query, tell, ROW};
use crate::engine::chatlog::{Line, Moderate, PLATFORMS};
use crate::i18n::{tr, trf};
use crate::Config;
use fltk::app;
use fltk::button::{Button, CheckButton};
use fltk::draw;
use fltk::enums::{Align, CallbackTrigger, Color, Event, Font, FrameType, Key};
use fltk::frame::Frame;
use fltk::group::{Flex, Wizard};
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

/// Pinned, and not run out yet.
fn pinned(l: &Line) -> bool {
    l.pinned.is_some_and(|until| until > now_ms())
}

/// One thing on a line.
enum It {
    Text(String, Font, Color),
    /// Text struck through: a deleted message's.
    Struck(String),
    Pic(Pic),
    Svg(SvgImage),
    /// A word's space: where the line may wrap.
    Space,
    /// A few pixels between pictures (no wrap).
    Gap(i32),
    /// What follows starts a new row (after a reply's header).
    Break,
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
            It::Pic(p) => (p.w(), p.h().max(1)),
            It::Svg(s) => (s.w(), s.h()),
            It::Space => {
                draw::set_font(Font::Helvetica, TEXT);
                (draw::width(" ") as i32, 0)
            }
            It::Gap(w) => (*w, 0),
            It::Break => (0, 0),
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
    if let Some(r) = &l.reply {
        if let Some(s) = icons::icon("corner-up-left", gray, 14) {
            v.push(It::Svg(s));
            v.push(It::Gap(4));
        }
        let mut said: String = r.text.chars().take(60).collect();
        if said.len() < r.text.len() {
            said.push('…');
        }
        v.push(It::Text(
            format!("Replying to @{}: {said}", r.user),
            Font::Helvetica,
            gray,
        ));
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
    if l.deleted {
        for w in l.text.split(' ') {
            v.push(It::Space);
            v.push(It::Struck(w.to_string()));
        }
        return v;
    }
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
        if matches!(it, It::Break) {
            out.push((Vec::new(), LINE));
            x = 0;
            continue;
        }
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
    /// The line under the pointer, which shows its actions, and the action under it.
    hover: Option<usize>,
    act: Option<usize>,
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

/// The actions `l` offers (with their icon and tooltip): moderation only where the platform has it,
/// and not on the broadcaster.
fn acts(l: &Line) -> Vec<(Act, &'static str, &'static str)> {
    if l.note || l.deleted {
        return Vec::new();
    }
    let mut v = vec![
        (Act::Reply, "corner-up-left", tr("chat.act_reply")),
        (Act::Copy, "copy", tr("chat.act_copy")),
    ];
    let p = l.platform.as_str();
    if p == "twitch" && !l.msg_id.is_empty() {
        v.push(if pinned(l) {
            (Act::Mod(Moderate::Unpin), "pin-off", tr("chat.act_unpin"))
        } else {
            (Act::Mod(Moderate::Pin(None)), "pin", tr("chat.act_pin"))
        });
    }
    if matches!(p, "twitch" | "kick") && !l.msg_id.is_empty() {
        v.push((Act::Mod(Moderate::Delete), "trash-2", tr("chat.act_delete_message")));
    }
    if matches!(p, "twitch" | "kick" | "youtube") && !l.broadcaster && !l.user_id.is_empty() {
        v.push((Act::Mod(Moderate::Timeout(0)), "clock", tr("chat.act_timeout")));
        v.push((Act::Mod(Moderate::Ban), "ban", tr("chat.act_ban")));
    }
    v
}

/// What Reply does: the chat box under the pane answers `l`.
type OnReply = Rc<RefCell<Option<Rc<dyn Fn(&Line)>>>>;
/// Answer a line in a chat box (None: stop answering).
type SetReply = Rc<dyn Fn(Option<&Line>)>;

/// One chat view: the lines, drawn, with a scrollbar.
#[derive(Clone)]
struct Pane {
    area: Frame,
    bar: Slider,
    st: Rc<RefCell<State>>,
    cmds: Cmds,
    on_reply: OnReply,
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
                act: None,
            })),
            cmds: cmds.clone(),
            on_reply: Rc::default(),
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
                        It::Struck(t) => {
                            draw::set_font(Font::Helvetica, TEXT);
                            draw::set_draw_color(Color::from_rgb(0x88, 0x88, 0x88));
                            draw::draw_text2(t, x, y, 0, rh, Align::Left);
                            let w = draw::width(t) as i32;
                            // Through the space before it too: one line across the message.
                            let sp = draw::width(" ") as i32;
                            draw::draw_line(x - sp, y + rh / 2, x + w, y + rh / 2);
                        }
                        It::Space | It::Gap(_) | It::Break => {}
                    }
                }
                y += rh;
            }
        }
        if let Some((l, bx, by)) = st.hover.and_then(|i| self.bar_at(f, &st, i)) {
            let a = acts(l);
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
        draw::pop_clip();
        // Animated emotes on screen: one redraw ~20 times a second, only while this is drawn.
        if moving && !st.ticking {
            st.ticking = true;
            let p = self.clone();
            app::add_timeout3(0.05, move |_| {
                p.st.borrow_mut().ticking = false;
                if !p.area.was_deleted() {
                    p.area.clone().redraw();
                }
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

    /// Line `i` and where its action bar goes: at the right of its first row, kept inside the view.
    fn bar_at<'a>(&self, f: &Frame, st: &'a State, i: usize) -> Option<(&'a Line, i32, i32)> {
        let l = st.lines.get(i)?;
        let n = acts(l).len() as i32;
        if n == 0 {
            return None;
        }
        let y = f.y() + 4 - st.top + st.laid.iter().take(i).map(|l| l.1).sum::<i32>();
        let first = st.laid.get(i)?.0.first().map_or(LINE, |r| r.1);
        let by = (y + (first - ACT) / 2).clamp(f.y() + 4, f.y() + f.h() - ACT - 4);
        Some((l, f.x() + f.w() - n * ACT - 8, by))
    }

    /// The hovered line's action under the pointer: its index in `acts`.
    fn act_index(&self, f: &Frame, st: &State) -> Option<usize> {
        let (l, bx, by) = self.bar_at(f, st, st.hover?)?;
        let (ex, ey) = (app::event_x(), app::event_y());
        let k = (ex - bx).div_euclid(ACT);
        ((by..by + ACT).contains(&ey) && (0..acts(l).len() as i32).contains(&k)).then_some(k as usize)
    }

    /// The action under the pointer, on the hovered line: the line and its `acts` entry.
    fn act_at(&self, f: &Frame) -> Option<(Line, (Act, &'static str, &'static str))> {
        let st = self.st.borrow();
        let k = self.act_index(f, &st)?;
        let l = st.lines.get(st.hover?)?;
        Some((l.clone(), acts(l)[k]))
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
                let menu = fltk::menu::MenuItem::new(&labels);
                if let Some(item) = menu.popup(app::event_x(), app::event_y()) {
                    let idx = item.value() as usize;
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
                let menu = fltk::menu::MenuItem::new(&labels);
                if let Some(item) = menu.popup(app::event_x(), app::event_y()) {
                    let idx = item.value() as usize;
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
            Event::Push if app::event_mouse_button() == app::MouseButton::Right => {
                let l = {
                    let st = self.st.borrow();
                    self.line_at(f, &st).and_then(|i| st.lines.get(i).cloned())
                };
                let Some(l) = l.filter(|l| !l.note) else { return true };
                use fltk::menu::{MenuButton, MenuFlag};
                let mut m = MenuButton::new(app::event_x(), app::event_y(), 0, 0, None);
                for (act, icon, name) in acts(&l) {
                    let (p, l) = (self.clone(), l.clone());
                    let i = m.add(name, fltk::enums::Shortcut::None, MenuFlag::Normal, move |_| {
                        p.run(&l, act)
                    });
                    if let Some(mut item) = m.at(i) {
                        item.add_image(icons::before_label(icon, super::muted(), 14), true);
                    }
                }
                if !l.user_id.is_empty() {
                    let (cmds, l) = (self.cmds.clone(), l.clone());
                    m.add(
                        tr("chat.user_info"),
                        fltk::enums::Shortcut::None,
                        MenuFlag::Normal,
                        move |_| card(&cmds, &l),
                    );
                }
                if m.size() > 0 {
                    m.popup();
                }
                MenuButton::delete(m);
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
    /// The chat box and what it answers (hidden in the all-platforms view).
    bar: Flex,
    set_reply: SetReply,
}

impl ChatColumn {
    fn new(cfg: &Cfg, cmds: &Cmds, fixed_plat: Option<usize>, go_accounts: Rc<dyn Fn()>) -> Self {
        let mut col = Flex::default().column();
        col.set_pad(4);
        let (_pane_group, pane) = Pane::new(cmds);

        // "Replying to @name: text" with an X, while answering one.
        let mut replying = Flex::default().row();
        replying.set_pad(6);
        let mut r_icon = Frame::default();
        r_icon.set_image(icons::icon("corner-up-left", super::muted(), 14));
        replying.fixed(&r_icon, 18);
        let mut r_text = Frame::default().with_align(Align::Left | Align::Inside | Align::Clip);
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
        col.end();

        let init_idx = fixed_plat.unwrap_or_else(|| {
            let saved = cfg.borrow().window.chat_all_platform.clone();
            PLATFORMS.iter().position(|p| *p == saved).unwrap_or(0)
        });
        let target_plat = Rc::new(Cell::new(init_idx));
        let reply: Rc<RefCell<Option<Line>>> = Rc::default();
        let click = emote_field(&mut input, cmds, target_plat.clone(), "Send a message");
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
            pane,
            acct_btn: acct_btn.clone(),
            input: input.clone(),
            target_plat: target_plat.clone(),
            bar: bar.clone(),
            set_reply: set_reply.clone(),
        };

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
                let want_broadcaster = cfg.borrow().window.chat_as_broadcaster.iter().any(|s| s == plat);
                let use_bot = !want_broadcaster;
                let to = reply.borrow_mut().take();
                tell(&cmds, move |e| {
                    let _ = e.send_chat_list(&[plat], &text, use_bot, true, to.as_ref());
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
        me.input.clone().handle({
            let s = do_send;
            move |i, ev| {
                if click(i, ev) {
                    return true;
                }
                if ev == Event::KeyDown && app::event_key() == Key::Enter {
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

    fn refresh_sender(&self, cfg: &Cfg, cmds: &Cmds) {
        let p_idx = self.target_plat.get().min(3);
        let plat_name = NAMES[p_idx];
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
        let tip = match &sender_name {
            Some(name) => trf(
                "chat.sending_as_tip",
                &[
                    ("platform", plat_name),
                    ("name", &name.replace('@', "@@")),
                    ("role", role_str),
                ],
            ),
            None => trf("chat.nobody_logged_in_tip", &[("platform", plat_name)]),
        };
        btn.set_tooltip(&tip);
        let mut inp = self.input.clone();
        let placeholder_tip = match &sender_name {
            Some(name) => trf(
                "chat.message_as_tip",
                &[
                    ("platform", plat_name),
                    ("name", &name.replace('@', "@@")),
                    ("role", role_str),
                ],
            ),
            None => trf("chat.message_plat_tip", &[("platform", plat_name)]),
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
        } else if let Some(i) = self.shown().and_then(|s| PLATFORMS.iter().position(|p| *p == s)) {
            self.main.target_plat.set(i);
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
        if together {
            (self.main.set_reply)(None);
            bar.hide();
            self.main.col.fixed(&bar, 0);
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
    top.fixed(&view, 136);
    let mut hint = Frame::default()
        .with_label(tr("chat.hint"))
        .with_align(Align::Right | Align::Inside | Align::Clip)
        .ellipsis();
    hint.set_label_size(12);
    hint.set_label_color(super::muted());

    // Discrete "Message all" button with broadcast icon
    let mut msg_all = Button::default()
        .with_label(tr("chat.message_all_btn"))
        .with_align(Align::Inside | Align::ImageNextToText);
    msg_all.set_image(icons::icon("megaphone", Color::Foreground, 15));
    msg_all.set_tooltip(tr("chat.message_all_tooltip"));
    top.fixed(&msg_all, 114);

    let mut opts = fltk::menu::MenuButton::default().with_label(tr("chat.options"));
    top.fixed(&opts, 84);
    let mut clear = button(&mut top, tr("btn.clear"), 64);
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
        move |_| message_all_popup(&cfg, &cmds, None)
    });

    {
        let w = &cfg.borrow().window;
        SHOW.set((!w.chat_no_time, !w.chat_no_badges));
    }
    let (time, badges) = SHOW.get();
    for (i, (label, on)) in [(tr("chat.opt_time"), time), (tr("chat.opt_badges"), badges)]
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
    let h = 236 + i32::from(crate::secrets::is_locked()) * 34;
    let mut win = Window::default()
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
        let mut pick_btn = button(&mut row, tr("btn.select"), 96);
        pick_btn.set_frame(FrameType::RFlatBox);
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
            let (h, st_txt) = if conn {
                (super::Health::Up, "Connected")
            } else if standby {
                (super::Health::Standby, "Standby")
            } else {
                (super::Health::Off, "Off")
            };
            h.show(&mut head_title, &format!("{pname}: {st_txt}"));
            head_title.set_label_font(Font::HelveticaBold);

            let want_broadcaster = cfg.borrow().window.chat_as_broadcaster.iter().any(|s| s == slug);
            let slots = platform_accounts(&cmds, &cfg, slug, 32);
            let (eff_bot, _, _) = effective_sender_role(&cfg, &cmds, p_idx);

            for (bot_i, ((mut pic_f, mut info_f, mut btn), slot)) in
                slot_uis.iter().cloned().zip(slots.iter()).enumerate()
            {
                let is_bot = bot_i == 1;
                let role = if is_bot {
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
                        info_f.set_label(&trf("chat.role_not_logged_in", &[("role", role), ("suffix", suffix)]));
                        btn.set_label(tr("chat.btn_not_logged_in"));
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
fn message_all_popup(cfg: &Cfg, cmds: &Cmds, reply: Option<Line>) {
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
    for i in 0..4 {
        let suffix = if conn_states[i] { "" } else { tr("chat.offline_suffix") };
        let mut c = CheckButton::default().with_label(&format!("{}{suffix}", NAMES[i]));
        let any_conn = conn_states.iter().any(|&x| x);
        c.set_checked(match &reply {
            Some(l) => l.platform == PLATFORMS[i],
            None if any_conn => conn_states[i],
            None => set_up[i],
        });
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

    let mut bar = Flex::default().row();
    bar.set_pad(4);
    let mut input = Input::default();
    let click = emote_field(&mut input, cmds, Rc::new(Cell::new(0)), "Send a message");
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
    let status = Frame::default().with_align(Align::Left | Align::Inside);
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
            let targets: Vec<&'static str> = (0..4)
                .filter(|&i| checks[i].is_checked())
                .map(|i| PLATFORMS[i])
                .collect();
            if targets.is_empty() {
                status.set_label(tr("chat.check_at_least_one"));
                status.set_label_color(Color::from_rgb(0xe0, 0x60, 0x50));
                status.redraw();
                return;
            }
            let use_bot = who.value() == 0;
            let fb = fallback.is_checked();
            let reply = reply.clone();
            tell(&cmds, move |e| {
                let _ = e.send_chat_list(&targets, &text, use_bot, fb, reply.as_ref());
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
        let _ = app::wait_for(0.1);
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
            let ps = pieces(&cmds, PLATFORMS[platform.get().min(3)], &v, i.h() - 10);
            ps.iter().any(|p| p.1.is_some()).then_some(ps)
        }
    };
    // Scrolled so the cursor shows: the text's start, left of the box.
    let shift = |i: &Input, cur: i32, total: i32| (cur - (i.w() - 14)).max(0).min(total.max(0));
    input.draw({
        let state = state.clone();
        move |i| {
            let Some(ps) = state(i) else {
                if i.value().is_empty() && !i.has_focus() {
                    draw::set_font(Font::Helvetica, i.text_size());
                    draw::set_draw_color(super::muted());
                    draw::draw_text2(empty, i.x() + 6, i.y(), i.w() - 12, i.h(), Align::Left | Align::Clip);
                }
                return;
            };
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
            let v = i.value();
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
    // Clicks land between the pieces as drawn.
    move |i: &mut Input, ev: Event| {
        if !matches!(ev, Event::Push | Event::Drag) || app::event_mouse_button() != app::MouseButton::Left {
            return false;
        }
        let Some(ps) = state(i) else { return false };
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
    let default_tab = providers.iter().position(|&p| p == Provider::Platform).unwrap_or(0);
    let title_plat = match platform {
        "twitch" => "Twitch",
        "kick" => "Kick",
        "youtube" => "YouTube",
        "tiktok" => "TikTok",
        _ => tr("chat.all_platforms"),
    };
    let mut win = Window::default()
        .with_size(380, 480)
        .with_label(&trf("chat.emotes_window_title", &[("platform", title_plat)]));
    win.set_xclass("streamerz");
    win.make_modal(true);
    let mut col = Flex::new(8, 8, 364, 464, None).column();
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
    win.end();
    win.resizable(&grid.area);

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
    let done = Rc::new(Cell::new(false));
    win.set_callback({
        let done = done.clone();
        move |_| done.set(true)
    });

    style_tabs(default_tab);
    win.show();
    col.layout();
    fill(false);
    let _ = search.take_focus();

    let mut pics = img::ARRIVED.load(std::sync::atomic::Ordering::Relaxed);
    let mut seen_rev = emotes::REV.load(std::sync::atomic::Ordering::Relaxed);
    let mut picked = false;
    while win.shown() && !done.get() {
        let _ = app::wait_for(0.25);
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
                    let _ = crate::save(&cfg.borrow());
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
    win.hide();
    app::delete_widget(win);
    if picked {
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
    let mut win = Window::default().with_size(460, 480).with_label(&trf(
        "chat.user_on_platform",
        &[("user", &l.user), ("platform", platform)],
    ));
    win.set_xclass("streamerz");
    let mut col = Flex::new(10, 10, 440, 460, None).column();
    col.set_pad(8);
    let mut head = Flex::default().row();
    head.set_pad(8);
    let pic = Frame::default();
    head.fixed(&pic, 64);
    let mut name = Frame::default()
        .with_label(&format!("  {}", l.user.replace('@', "@@")))
        .with_align(Align::Left | Align::Inside | Align::ImageNextToText)
        .ellipsis();
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
    let mut close = button(&mut bottom, tr("dialogs.close"), 80);
    bottom.end();
    col.fixed(&bottom, ROW);
    col.end();
    win.end();
    win.resizable(&*rows);

    let add = |rows: &mut InfoRows, k: &str, v: &str| {
        rows.add(&format!("{k}\t{}", v.replace(['\n', '\t'], " ")));
    };
    let roles: Vec<&str> = [
        (l.broadcaster, tr("chat.card_broadcaster")),
        (l.moderator, tr("chat.card_moderator")),
        (l.vip, tr("chat.card_vip")),
        (l.subscriber, tr("chat.card_subscriber")),
    ]
    .iter()
    .filter(|r| r.0)
    .map(|r| r.1)
    .collect();
    add(
        &mut rows,
        tr("chat.card_roles"),
        &if roles.is_empty() {
            tr("chat.card_viewer").into()
        } else {
            roles.join(", ")
        },
    );
    add(&mut rows, tr("chat.card_messages_session"), &seen.messages.to_string());
    add(&mut rows, tr("chat.card_first_message"), &seen.first);
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
        add(
            &mut rows,
            &trf("chat.card_platform_says", &[("platform", platform)]),
            tr("chat.card_looking_up"),
        );
    }
    rows.set_callback(|b| {
        if app::event_clicks() {
            if let Some(t) = b.selected_text() {
                app::copy(t.split_once('\t').map_or(t.as_str(), |x| x.1));
                crate::say(tr("chat.copied").into());
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
