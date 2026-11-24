//! Application adapters shared by the frontend and automation clients.

pub mod accounts;
pub mod actions;
pub mod chat;
pub mod clipboard;
pub mod connections;
pub mod frontend;
pub mod images;
pub mod log;
pub mod login;
pub mod moderation;
pub mod moderation_activity;
pub mod moderation_chat;
pub mod moderation_settings;
pub mod moderation_suspicious;
pub mod navigation;
pub mod outputs;
pub mod overlay_media;
pub mod overlay_test;
pub mod overlays;
pub mod pages;
pub mod parent;
pub mod polls;
pub mod preflight;
pub mod prompts;
pub mod protocol;
pub mod read;
#[cfg(windows)]
mod renderer_job;
pub mod requests;
pub mod requirements;
pub mod rewards;
pub mod sections;
pub mod setting_fields;
pub mod settings;
pub mod status;
pub mod stream;
pub mod timers;
pub mod undo;
pub mod view;

/// Submit an engine operation to its existing single-thread runtime.
pub fn submit(
    commands: &tokio::sync::mpsc::UnboundedSender<crate::engine::UiCmd>,
    operation: impl FnOnce(&std::rc::Rc<crate::engine::Engine>) + Send + 'static,
) {
    let _ = commands.send(Box::new(operation));
}

#[cfg(test)]
mod boundary;
