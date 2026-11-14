//! Core prompts are ordinary in-window dialogs; the core owns their lifetime and answers.
use crate::{i18n::tr, send};
use gpui_kit::component::{
    button::{Button, ButtonVariants},
    input::{Input, InputState},
    Disableable,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use serde_json::{json, Value};

pub struct Prompt {
    model: Value,
    input: Entity<InputState>,
    selected: Option<String>,
    pub pending: bool,
    pub error: String,
    buttons_view: Entity<PromptButtons>,
}
impl Prompt {
    pub fn new(model: Value, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx
            .new(|cx| InputState::new(window, cx).default_value(model["text"].as_str().unwrap_or_default().to_owned()));
        Self {
            model,
            input,
            selected: None,
            pending: false,
            error: String::new(),
            buttons_view: {
                let weak = cx.entity().downgrade();
                cx.new(|_| PromptButtons(weak))
            },
        }
    }
    pub fn answer(&mut self, value: Value, cx: &mut Context<Self>) {
        if self.pending {
            return;
        }
        let id = self.model["id"].as_u64().unwrap_or_default();
        send(json!({"request":"StreamerzAnswerPrompt", "promptId":id, "value":value, "id":format!("prompt:{id}")}));
        self.pending = true;
        self.error.clear();
        cx.notify();
    }
    pub fn cancel(&mut self, cx: &mut Context<Self>) {
        self.answer(
            if self.model["kind"] == "confirm" {
                json!(false)
            } else {
                Value::Null
            },
            cx,
        );
    }
}
impl Render for Prompt {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let waiting = self.model["kind"] == "wait";
        let notice = self.model["kind"] == "notice";
        let confirm = self.model["kind"] == "confirm";
        let choice = self.model["kind"] == "choice";
        let mut body = div().id("prompt-form").test_support().flex().flex_col().gap_3();
        if !self.error.is_empty() {
            body = body.child(super::notice::error(&self.error, cx));
        }
        if confirm || waiting || notice {
            body = body.child(self.model["text"].as_str().unwrap_or_default().to_owned());
            // A device login also offers its page and its code.
            if waiting && (self.model["link"].is_string() || self.model["copy"].is_string()) {
                let (link, code) = (
                    self.model["link"].as_str().map(str::to_owned),
                    self.model["copy"].as_str().map(str::to_owned),
                );
                body = body.child(
                    div()
                        .flex()
                        .gap_2()
                        .when_some(link, |row, link| {
                            row.child(
                                Button::new("prompt-open-link")
                                    .label(tr("btn.open_browser"))
                                    .on_click(move |_, _, cx| cx.open_url(&link)),
                            )
                        })
                        .when_some(code, |row, code| {
                            row.child(Button::new("prompt-copy-code").label(tr("btn.copy_code")).on_click(
                                move |_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(code.clone())),
                            ))
                        }),
                );
            }
        } else if choice {
            for (index, option) in self.model["options"].as_array().into_iter().flatten().enumerate() {
                let id = option["id"].as_str().unwrap_or_default().to_owned();
                let selected = self.selected.as_ref() == Some(&id);
                body = body.child(
                    Button::new(("prompt-choice", index))
                        .label(option["label"].as_str().unwrap_or_default().to_owned())
                        .disabled(self.pending)
                        .when(selected, |button| button.primary())
                        .on_click(cx.listener(move |prompt, _, _, cx| {
                            prompt.selected = Some(id.clone());
                            cx.notify();
                        })),
                );
            }
        } else {
            let label = self.model["prompt"].as_str().unwrap_or_default().to_owned();
            body = body
                .child(label.clone())
                .child(Input::new(&self.input).aria_label(label).disabled(self.pending));
        }
        body
    }
}

impl Prompt {
    /// The prompt's buttons, for its dialog's footer.
    pub fn buttons(&self, cx: &mut Context<Self>) -> Div {
        let waiting = self.model["kind"] == "wait";
        let notice = self.model["kind"] == "notice";
        let confirm = self.model["kind"] == "confirm";
        let choice = self.model["kind"] == "choice";
        let no = if confirm {
            self.model["no"].as_str().unwrap_or_default()
        } else {
            tr("dialogs.cancel")
        };
        let yes = if confirm {
            self.model["yes"].as_str().unwrap_or_default()
        } else {
            tr("dialogs.ok")
        };
        // A confirm with no yes is only acknowledged: its one button (OK) is the main one.
        let only_no = confirm && yes.is_empty();
        let right = div()
            .flex()
            .gap_2()
            .when(!notice, |row| {
                row.child(
                    Button::new("cancel-prompt")
                        .label(no.to_owned())
                        .when(only_no, |button| button.primary())
                        .disabled(self.pending)
                        .on_click(cx.listener(|prompt, _, _, cx| prompt.cancel(cx))),
                )
            })
            .when(!waiting && !only_no, |row| {
                row.child(
                    Button::new("answer-prompt")
                        .primary()
                        .label(yes.to_owned())
                        .disabled(self.pending || choice && self.selected.is_none())
                        .on_click(cx.listener(move |prompt, _, _, cx| {
                            // A notice is only acknowledged: it has no box to read.
                            let value = if confirm {
                                json!(true)
                            } else if choice {
                                json!(prompt.selected)
                            } else if notice {
                                Value::Null
                            } else {
                                json!(prompt.input.read(cx).value().to_string())
                            };
                            prompt.answer(value, cx);
                        })),
                )
            });
        super::dialog::footer(None, right, cx)
    }

    /// The buttons as their own view, for a dialog's footer slot.
    pub fn buttons_view(&self) -> Entity<PromptButtons> {
        self.buttons_view.clone()
    }
}

/// A prompt's buttons, drawn in its dialog's footer.
pub struct PromptButtons(WeakEntity<Prompt>);
impl Render for PromptButtons {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        match self.0.upgrade() {
            Some(prompt) => prompt.update(cx, |prompt, cx| prompt.buttons(cx)).into_any_element(),
            None => div().into_any_element(),
        }
    }
}
