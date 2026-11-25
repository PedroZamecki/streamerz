//! Pending core interactions survive a renderer restart and can be answered through MCP.
use serde::Serialize;
use serde_json::Value;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Mutex,
};
use tokio::sync::oneshot;

#[derive(Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Content {
    Input {
        title: String,
        prompt: String,
        text: String,
    },
    Confirm {
        text: String,
        no: String,
        yes: String,
    },
    Choice {
        title: String,
        options: Vec<Choice>,
    },
    /// Something is happening elsewhere (a browser login). Answering cancels it.
    Wait {
        title: String,
        text: String,
        /// A page to open again, and a code to copy, when the wait needs them.
        #[serde(skip_serializing_if = "Option::is_none")]
        link: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        copy: Option<String>,
    },
    /// Something to read. Any answer closes it.
    Notice {
        title: String,
        text: String,
        #[serde(skip_serializing_if = "std::ops::Not::not")]
        markdown: bool,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        meta: Vec<NoticeMeta>,
    },
}
#[derive(Clone, PartialEq, Serialize)]
pub struct NoticeMeta {
    pub key: String,
    pub value: String,
}
#[derive(Clone, PartialEq, Serialize)]
pub struct Choice {
    pub id: String,
    pub label: String,
}
#[derive(Clone, Serialize)]
pub struct Prompt {
    pub id: u64,
    #[serde(flatten)]
    pub content: Content,
}
enum Reply {
    Text(oneshot::Sender<Option<String>>),
    Confirm(oneshot::Sender<bool>),
    Abort(tokio::task::AbortHandle),
    /// Nobody waits for the answer.
    Notice,
}
impl Reply {
    fn closed(&self) -> bool {
        match self {
            Self::Text(sender) => sender.is_closed(),
            Self::Confirm(sender) => sender.is_closed(),
            Self::Abort(handle) => handle.is_finished(),
            Self::Notice => false,
        }
    }
}
#[derive(Default)]
struct Broker {
    entries: Vec<(Prompt, Reply)>,
}
impl Broker {
    fn insert(&mut self, content: Content, reply: Reply) {
        static SERIAL: AtomicU64 = AtomicU64::new(1);
        self.entries.push((
            Prompt {
                id: SERIAL.fetch_add(1, Ordering::Relaxed),
                content,
            },
            reply,
        ));
    }
    /// A notice to read, unless the same one is already waiting (a button pressed again, a login
    /// that fails the same way twice): one OK closes it.
    fn notice(&mut self, content: Content) {
        if !self.entries.iter().any(|(prompt, _)| prompt.content == content) {
            self.insert(content, Reply::Notice);
        }
    }
    fn list(&mut self) -> Vec<Prompt> {
        self.entries.retain(|(_, reply)| !reply.closed());
        self.entries.iter().map(|(prompt, _)| prompt.clone()).collect()
    }
    fn answer(&mut self, id: u64, value: Value) -> Result<(), String> {
        let index = self
            .entries
            .iter()
            .position(|(prompt, _)| prompt.id == id)
            .ok_or("prompt is no longer waiting")?;
        let (prompt, reply) = &self.entries[index];
        match &prompt.content {
            Content::Input { .. } if value.is_null() || value.is_string() => {}
            Content::Confirm { .. } if value.is_boolean() => {}
            Content::Wait { .. } => {}
            // A notice is only acknowledged, whatever comes with the OK.
            Content::Notice { .. } => {}
            Content::Choice { options, .. }
                if value.is_null()
                    || value
                        .as_str()
                        .is_some_and(|id| options.iter().any(|option| option.id == id)) => {}
            _ => return Err("invalid prompt answer".into()),
        }
        if reply.closed() {
            self.entries.remove(index);
            return Err("prompt is no longer waiting".into());
        }
        let (_, reply) = self.entries.remove(index);
        match reply {
            Reply::Text(sender) => {
                let _ = sender.send(value.as_str().map(str::to_owned));
            }
            Reply::Confirm(sender) => {
                let _ = sender.send(value.as_bool().unwrap_or(false));
            }
            Reply::Abort(handle) => handle.abort(),
            Reply::Notice => {}
        }
        Ok(())
    }
}
static BROKER: Mutex<Broker> = Mutex::new(Broker { entries: Vec::new() });

pub fn input(title: String, prompt: String, text: String) -> oneshot::Receiver<Option<String>> {
    let (sender, receiver) = oneshot::channel();
    if let Ok(mut broker) = BROKER.lock() {
        broker.insert(Content::Input { title, prompt, text }, Reply::Text(sender));
    }
    receiver
}
pub fn confirm(text: String, no: &str, yes: &str) -> oneshot::Receiver<bool> {
    let (sender, receiver) = oneshot::channel();
    if let Ok(mut broker) = BROKER.lock() {
        broker.insert(
            Content::Confirm {
                text,
                no: no.into(),
                yes: yes.into(),
            },
            Reply::Confirm(sender),
        );
    }
    receiver
}
pub fn youtube(streams: Vec<(String, String)>) -> oneshot::Receiver<Option<String>> {
    let (sender, receiver) = oneshot::channel();
    let options = streams
        .into_iter()
        .map(|(id, title)| Choice {
            label: format!("{title} ({id})"),
            id,
        })
        .collect();
    if let Ok(mut broker) = BROKER.lock() {
        broker.insert(
            Content::Choice {
                title: crate::i18n::tr("youtube.choose_stream_title").into(),
                options,
            },
            Reply::Text(sender),
        );
    }
    receiver
}
/// A cancellable wait for `handle`'s task; it goes away with the task.
pub fn wait(title: String, text: String, handle: tokio::task::AbortHandle) {
    wait_with(title, text, handle, None, None);
}
/// A wait that also offers a page to open and a code to copy (a device login).
pub fn wait_with(
    title: String,
    text: String,
    handle: tokio::task::AbortHandle,
    link: Option<String>,
    copy: Option<String>,
) {
    if let Ok(mut broker) = BROKER.lock() {
        broker.insert(
            Content::Wait {
                title,
                text,
                link,
                copy,
            },
            Reply::Abort(handle),
        );
    }
    crate::host::prompts_changed();
}
/// One of `options`, by its id; None when cancelled.
pub fn choice(title: String, options: Vec<Choice>) -> oneshot::Receiver<Option<String>> {
    let (sender, receiver) = oneshot::channel();
    if let Ok(mut broker) = BROKER.lock() {
        broker.insert(Content::Choice { title, options }, Reply::Text(sender));
    }
    crate::host::prompts_changed();
    receiver
}
/// Text to read, closed with OK.
pub fn notice(title: String, text: String) {
    if let Ok(mut broker) = BROKER.lock() {
        broker.notice(Content::Notice {
            title,
            text,
            markdown: false,
            meta: Vec::new(),
        });
    }
    crate::host::prompts_changed();
}
/// Markdown text to read, with optional metadata tags, closed with OK.
pub fn markdown_notice(title: String, meta: Vec<NoticeMeta>, text: String) {
    if let Ok(mut broker) = BROKER.lock() {
        broker.notice(Content::Notice {
            title,
            text,
            markdown: true,
            meta,
        });
    }
    crate::host::prompts_changed();
}
pub fn list() -> Vec<Prompt> {
    BROKER.lock().map(|mut broker| broker.list()).unwrap_or_default()
}
pub fn answer(id: u64, value: Value) -> Result<(), String> {
    BROKER
        .lock()
        .map_err(|_| "prompts are unavailable")?
        .answer(id, value)?;
    crate::host::prompts_changed();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn reads_keep_prompts_until_a_valid_single_answer_and_prune_cancelled_callers() {
        let mut broker = Broker::default();
        let (sender, mut receiver) = oneshot::channel();
        broker.insert(
            Content::Input {
                title: "title".into(),
                prompt: "question".into(),
                text: "default".into(),
            },
            Reply::Text(sender),
        );
        let id = broker.list()[0].id;
        assert_eq!(broker.list()[0].id, id);
        assert!(broker.answer(id, true.into()).is_err());
        assert_eq!(broker.list().len(), 1);
        broker.answer(id, "answer".into()).unwrap();
        assert_eq!(receiver.try_recv().unwrap(), Some("answer".into()));
        assert!(broker.answer(id, Value::Null).is_err());
        let (sender, receiver) = oneshot::channel();
        broker.insert(
            Content::Confirm {
                text: "question".into(),
                no: "No".into(),
                yes: "Yes".into(),
            },
            Reply::Confirm(sender),
        );
        drop(receiver);
        assert!(broker.list().is_empty());
    }
    #[test]
    fn a_notice_closes_with_its_ok_whatever_comes_with_it() {
        // A login's "Successfully logged in" once answered "invalid prompt answer" to its OK.
        for answer in [Value::Null, json!(""), json!(true)] {
            let mut broker = Broker::default();
            broker.insert(
                Content::Notice {
                    title: "Twitch Login".into(),
                    text: "Logged in".into(),
                    markdown: false,
                    meta: Vec::new(),
                },
                Reply::Notice,
            );
            let id = broker.list()[0].id;
            assert_eq!(broker.answer(id, answer), Ok(()));
            assert!(broker.list().is_empty());
        }
    }

    #[test]
    fn choices_preserve_ids_and_cancellation_differs_from_an_empty_answer() {
        let mut broker = Broker::default();
        let (sender, mut receiver) = oneshot::channel();
        broker.insert(
            Content::Choice {
                title: "Pick".into(),
                options: vec![Choice {
                    id: "stream".into(),
                    label: "Live".into(),
                }],
            },
            Reply::Text(sender),
        );
        let id = broker.list()[0].id;
        assert!(broker.answer(id, "unknown".into()).is_err());
        broker.answer(id, Value::Null).unwrap();
        assert_eq!(receiver.try_recv().unwrap(), None);
        let (sender, mut receiver) = oneshot::channel();
        broker.insert(
            Content::Input {
                title: "title".into(),
                prompt: "question".into(),
                text: "default".into(),
            },
            Reply::Text(sender),
        );
        let id = broker.list()[0].id;
        broker.answer(id, "".into()).unwrap();
        assert_eq!(receiver.try_recv().unwrap(), Some(String::new()));
    }

    #[test]
    fn markdown_notice_serializes_markdown_flag_and_meta_while_plain_notice_omits_them() {
        let mut broker = Broker::default();
        broker.insert(
            Content::Notice {
                title: "Plain".into(),
                text: "keep_underscores_literal".into(),
                markdown: false,
                meta: Vec::new(),
            },
            Reply::Notice,
        );
        broker.insert(
            Content::Notice {
                title: "Toast Activation".into(),
                text: "### Fields\n\nNone.".into(),
                markdown: true,
                meta: vec![
                    NoticeMeta {
                        key: "Kind".into(),
                        value: "trigger".into(),
                    },
                    NoticeMeta {
                        key: "Code".into(),
                        value: "708".into(),
                    },
                ],
            },
            Reply::Notice,
        );
        let listed = serde_json::to_value(broker.list()).unwrap();
        let plain = &listed[0];
        assert_eq!(plain["title"], "Plain");
        assert!(plain.get("markdown").is_none());
        assert!(plain.get("meta").is_none());

        let md = &listed[1];
        assert_eq!(md["title"], "Toast Activation");
        assert_eq!(md["markdown"], true);
        assert_eq!(md["meta"][0]["key"], "Kind");
        assert_eq!(md["meta"][0]["value"], "trigger");
        assert_eq!(md["meta"][1]["key"], "Code");
        assert_eq!(md["meta"][1]["value"], "708");
    }

    fn plain(title: &str, text: &str) -> Content {
        Content::Notice {
            title: title.into(),
            text: text.into(),
            markdown: false,
            meta: Vec::new(),
        }
    }

    #[test]
    fn the_same_notice_twice_waits_once_and_closes_with_one_ok() {
        let mut broker = Broker::default();
        broker.notice(plain("Unlock logins", "The keyring stayed locked."));
        broker.notice(plain("Unlock logins", "The keyring stayed locked."));
        assert_eq!(broker.list().len(), 1);
        // Another text is another notice.
        broker.notice(plain("Unlock logins", "No keyring was found."));
        let listed = broker.list();
        assert_eq!(listed.len(), 2);
        broker.answer(listed[0].id, Value::Null).unwrap();
        assert_eq!(broker.list().len(), 1);
        // Once read, it can be said again.
        broker.notice(plain("Unlock logins", "The keyring stayed locked."));
        assert_eq!(broker.list().len(), 2);
    }

    #[test]
    fn a_prompt_is_answered_once_and_a_wait_goes_with_its_task() {
        crate::engine::tests::run(async {
            let mut broker = Broker::default();
            let (sender, mut receiver) = oneshot::channel();
            broker.insert(
                Content::Confirm {
                    text: "Sure?".into(),
                    no: "No".into(),
                    yes: "Yes".into(),
                },
                Reply::Confirm(sender),
            );
            let id = broker.list()[0].id;
            assert!(
                broker.answer(id, json!("yes")).is_err(),
                "a confirm takes true or false"
            );
            broker.answer(id, json!(true)).unwrap();
            assert_eq!(receiver.try_recv(), Ok(true));
            assert_eq!(
                broker.answer(id, json!(true)),
                Err("prompt is no longer waiting".into())
            );
            // Answering a wait cancels its task; a task that ends takes its wait away.
            let task = tokio::task::spawn_local(std::future::pending::<()>());
            broker.insert(plain_wait("Logging in"), Reply::Abort(task.abort_handle()));
            let id = broker.list()[0].id;
            broker.answer(id, Value::Null).unwrap();
            assert!(task.await.unwrap_err().is_cancelled());
            let done = tokio::task::spawn_local(async {});
            broker.insert(plain_wait("Logging in"), Reply::Abort(done.abort_handle()));
            done.await.unwrap();
            assert!(broker.list().is_empty());
            // An id never given is not waiting.
            assert!(broker.answer(u64::MAX, Value::Null).is_err());
        });
    }

    fn plain_wait(title: &str) -> Content {
        Content::Wait {
            title: title.into(),
            text: String::new(),
            link: None,
            copy: None,
        }
    }
}
