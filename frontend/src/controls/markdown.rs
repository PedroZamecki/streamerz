//! Shared Markdown rendering styled with Streamerz's theme and typography tokens.
use crate::theme::{badge_bg, FONT_BODY, FONT_PAGE_TITLE, FONT_SECTION_TITLE, LINE_BODY};
use gpui_kit::component::{
    highlighter::HighlightTheme,
    text::{TextView, TextViewStyle},
    ActiveTheme,
};
use gpui_kit::*;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

fn keyed_id(prefix: &str, text: &str) -> ElementId {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    SharedString::from(format!("{prefix}:{:016x}", hasher.finish())).into()
}

fn doc_style(cx: &App) -> TextViewStyle {
    let dark = cx.theme().is_dark();
    let mut style = TextViewStyle::default()
        .paragraph_gap(rems(0.6))
        .heading_font_size(|level, _| match level {
            1 => px(FONT_PAGE_TITLE),
            2 => px(FONT_SECTION_TITLE),
            _ => px(FONT_BODY),
        })
        .inline_code(HighlightStyle {
            background_color: Some(badge_bg(cx)),
            ..Default::default()
        });
    style.is_dark = dark;
    if dark {
        style.highlight_theme = HighlightTheme::default_dark().clone();
    }
    style
}

/// Full Markdown block for Help and rich notices, using Streamerz's heading scale and inline code tone.
pub fn view(id: &str, text: impl Into<SharedString>, cx: &App) -> TextView {
    let text: SharedString = text.into();
    let element_id = keyed_id(id, text.as_ref());
    TextView::markdown(element_id, text)
        .w_full()
        .min_w_0()
        .text_size(px(FONT_BODY))
        .line_height(px(LINE_BODY))
        .selectable(true)
        .style(doc_style(cx))
}

/// Compact inline-capable Markdown block that inherits its parent's text colour, size, and line height.
pub fn inline(id: &str, text: impl Into<SharedString>) -> TextView {
    let text: SharedString = text.into();
    let element_id = keyed_id(id, text.as_ref());
    TextView::markdown(element_id, text).w_full().min_w_0().selectable(true)
}
