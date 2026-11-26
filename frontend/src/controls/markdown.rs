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

/// Whether a link in shown text may open: only web pages and mail. The text can come from an imported
/// action or a plugin, and a `file:` path or another program's own scheme must not start from a click.
fn opens(url: &str) -> bool {
    let scheme = url
        .trim_start()
        .split_once(':')
        .map(|(scheme, _)| scheme.to_ascii_lowercase());
    matches!(scheme.as_deref(), Some("http" | "https" | "mailto"))
}

/// Opens a clicked link the way the toolkit does (a left or middle click, a key, a tap), when [`opens`].
fn open_link(url: &SharedString, event: &ClickEvent, _: &mut Window, cx: &mut App) {
    let pressed = match event {
        ClickEvent::Mouse(click) => matches!(click.up.button, MouseButton::Left | MouseButton::Middle),
        ClickEvent::Keyboard(_) => true,
        ClickEvent::Touch(tap) => !tap.long_press,
    };
    if pressed && opens(url) {
        cx.open_url(url);
    }
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
        .on_link_click(open_link)
        .style(doc_style(cx))
}

/// Compact inline-capable Markdown block that inherits its parent's text colour, size, and line height.
pub fn inline(id: &str, text: impl Into<SharedString>) -> TextView {
    let text: SharedString = text.into();
    let element_id = keyed_id(id, text.as_ref());
    TextView::markdown(element_id, text)
        .w_full()
        .min_w_0()
        .selectable(true)
        .on_link_click(open_link)
}

#[cfg(test)]
mod tests {
    #[test]
    fn only_web_and_mail_links_open() {
        for url in [
            "https://docs.streamer.bot",
            "HTTP://example.com/a?b=c",
            " mailto:me@example.com",
        ] {
            assert!(super::opens(url), "{url}");
        }
        for url in [
            "file:///etc/passwd",
            "ms-msdt:/id PCWDiagnostic",
            "javascript:alert(1)",
            "steam://run/1",
            "C:\\Windows\\notepad.exe",
            "/home/user/script.sh",
            "relative/page",
            "",
        ] {
            assert!(!super::opens(url), "{url}");
        }
    }
}
