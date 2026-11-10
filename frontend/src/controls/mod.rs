//! Reusable controls. Pages choose content and actions, never a separate widget style.
pub mod actions;
pub mod body;
pub mod card;
pub mod chat;
pub mod dialog;
pub mod emotes;
pub mod form;
pub mod health;
pub mod list;
pub mod log;
pub mod notice;
pub mod picker;
pub mod prompt;
pub mod rows;
pub mod sections;
pub mod tabs;
pub mod tree;

/// A dialog's title as 1.6.0 sets it: 13px, bold, and a hair above the toolkit's own place, so it lines up
/// with the close button.
pub fn dialog_title(text: impl Into<gpui_kit::SharedString>) -> impl gpui_kit::IntoElement {
    use gpui_kit::{px, FontWeight, ParentElement, Styled};
    gpui_kit::div()
        .text_size(px(13.))
        .font_weight(FontWeight::BOLD)
        .child(text.into())
}
