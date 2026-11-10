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
pub mod navigation;
pub mod pages;
pub mod parent;
pub mod polls;
pub mod prompts;
pub mod protocol;
pub mod read;
#[cfg(windows)]
mod renderer_job;
pub mod requests;
pub mod sections;
pub mod settings;
pub mod status;
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
