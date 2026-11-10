//! Browser logins and other long operations a person can cancel: a task on the engine's thread with a
//! waiting prompt any frontend (or MCP) can cancel, and a completion that runs only on success.

use crate::{engine::Engine, i18n::trf};
use std::{future::Future, rc::Rc};

/// Run `work`; while it runs a prompt says to finish in the browser and cancelling it stops the task
/// (so its port is free again). `done` runs with the result; a failure becomes a notice.
pub fn spawn<T: 'static>(
    engine: &Rc<Engine>,
    what: &str,
    who: &str,
    work: impl Future<Output = Result<T, String>> + 'static,
    done: impl FnOnce(&Rc<Engine>, T) + 'static,
) {
    let (title, text) = (
        trf("accounts.browser_login_title", &[("what", what), ("who", who)]),
        trf("accounts.browser_login_msg", &[("what", what), ("who", who)]),
    );
    let (engine, what) = (engine.clone(), what.to_owned());
    let failed_title = title.clone();
    let handle = tokio::task::spawn_local(async move {
        match work.await {
            Ok(value) => done(&engine, value),
            Err(error) => {
                crate::error(trf(
                    "accounts.browser_login_failed",
                    &[("what", &what), ("error", &error)],
                ));
                super::prompts::notice(
                    failed_title,
                    trf("accounts.browser_login_failed", &[("what", &what), ("error", &error)]),
                );
            }
        }
        crate::host::prompts_changed();
        crate::host::changed();
    });
    super::prompts::wait(title, text, handle.abort_handle());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_waiting_login_can_be_cancelled_and_a_failure_is_shown() {
        let _lock = crate::TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        crate::engine::tests::run(async {
            let engine = crate::engine::tests::engine(serde_json::json!({"actions": []}));
            let before = super::super::prompts::list().len();
            spawn(
                &engine,
                "Test",
                "me",
                std::future::pending::<Result<(), String>>(),
                |_, ()| {},
            );
            let waiting = super::super::prompts::list();
            assert_eq!(waiting.len(), before + 1);
            let id = waiting.last().unwrap().id;
            super::super::prompts::answer(id, serde_json::Value::Null).unwrap();
            tokio::task::yield_now().await;
            assert_eq!(super::super::prompts::list().len(), before);
            spawn(
                &engine,
                "Test",
                "me",
                async { Err::<(), _>("no browser".to_string()) },
                |_, ()| {},
            );
            tokio::task::yield_now().await;
            tokio::task::yield_now().await;
            let shown = super::super::prompts::list();
            let notice = shown
                .iter()
                .rev()
                .find(|prompt| matches!(prompt.content, super::super::prompts::Content::Notice { .. }));
            assert!(notice.is_some(), "a failed login is shown");
            super::super::prompts::answer(notice.unwrap().id, serde_json::Value::Null).unwrap();
        });
    }
}
