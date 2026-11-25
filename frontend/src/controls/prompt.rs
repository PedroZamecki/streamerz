//! Core prompts are ordinary in-window dialogs; the core owns their lifetime and answers.
use crate::theme::{CONTROL_HEIGHT, FONT_MICRO, LINE_MICRO, SPACE_SM, SPACE_XS};
use crate::{i18n::tr, send};
use gpui_kit::component::{
    button::{Button, ButtonVariants},
    input::{Input, InputState},
    ActiveTheme, Disableable, Sizable,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use serde_json::{json, Value};

fn meta_pill(text: String, cx: &App) -> Div {
    div()
        .px(px(6.))
        .py(px(2.))
        .rounded(px(4.))
        .bg(crate::theme::badge_bg(cx))
        .text_size(px(FONT_MICRO))
        .line_height(px(LINE_MICRO))
        .text_color(crate::theme::nav_foreground(cx))
        .child(text)
}

fn meta_header(meta: &[Value], cx: &App) -> impl IntoElement {
    let mut pills = Vec::new();
    let mut summary = Vec::new();
    let mut needs: Option<String> = None;
    for item in meta {
        let key = item["key"].as_str().unwrap_or_default();
        let value = item["value"].as_str().unwrap_or_default().trim();
        if value.is_empty() {
            continue;
        }
        match key {
            "Kind" => {
                let label = match value {
                    "trigger" => "Trigger".to_owned(),
                    "sub-action" => "Sub-action".to_owned(),
                    other => other.to_owned(),
                };
                summary.push(label.clone());
                pills.push(meta_pill(label, cx));
            }
            "Category" => {
                let label = value.replace('/', " > ");
                summary.push(label.clone());
                pills.push(meta_pill(label, cx));
            }
            "Type" => {
                summary.push(value.to_owned());
                pills.push(meta_pill(value.to_owned(), cx));
            }
            "Code" => {
                let label = format!("#{value}");
                summary.push(label.clone());
                pills.push(meta_pill(label, cx));
            }
            "Needs" => {
                if !value.eq_ignore_ascii_case("nothing") {
                    needs = Some(value.to_owned());
                }
            }
            other => {
                let label = format!("{other}: {value}");
                summary.push(label.clone());
                pills.push(meta_pill(label, cx));
            }
        }
    }
    if let Some(needs) = &needs {
        summary.push(format!("Needs: {needs}"));
    }
    div()
        .id("prompt-meta")
        .test_support()
        .role(Role::Group)
        .when(!summary.is_empty(), |el| el.aria_label(summary.join(", ")))
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .gap(px(SPACE_SM))
        .pb(px(SPACE_SM))
        .border_b_1()
        .border_color(cx.theme().border)
        .when(!pills.is_empty(), |header| {
            header.child(div().flex().flex_wrap().gap(px(SPACE_XS)).children(pills))
        })
        .when_some(needs, |header, needs| {
            header.child(super::notice::info(&format!("Needs: {needs}"), cx))
        })
}

pub struct Prompt {
    model: Value,
    input: Option<Entity<InputState>>,
    selected: Option<String>,
    pub pending: bool,
    pub error: Option<crate::error::StreamerzError>,
    buttons_view: Entity<PromptButtons>,
}
impl Prompt {
    pub fn new(model: Value, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let needs_input = !matches!(model["kind"].as_str(), Some("wait" | "notice" | "confirm" | "choice"));
        let input = needs_input.then(|| {
            cx.new(|cx| {
                InputState::new(window, cx).default_value(model["text"].as_str().unwrap_or_default().to_owned())
            })
        });
        Self {
            model,
            input,
            selected: None,
            pending: false,
            error: None,
            buttons_view: {
                let entity = cx.entity();
                let weak = entity.downgrade();
                cx.new(|cx| {
                    let sub = cx.observe(&entity, |_, _, cx| cx.notify());
                    PromptButtons {
                        prompt: weak,
                        _subscription: sub,
                    }
                })
            },
        }
    }
    pub fn answer(&mut self, value: Value, cx: &mut Context<Self>) {
        if self.pending {
            return;
        }
        let Some(id) = self.model["id"].as_u64() else {
            return;
        };
        send(json!({"request":"StreamerzAnswerPrompt", "promptId":id, "value":value, "id":format!("prompt:{id}")}));
        self.pending = true;
        self.error = None;
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let waiting = self.model["kind"] == "wait";
        let notice = self.model["kind"] == "notice";
        let confirm = self.model["kind"] == "confirm";
        let choice = self.model["kind"] == "choice";
        let mut body = div().w_full().min_w_0().flex().flex_col().gap(px(SPACE_SM));
        if let Some(err) = &self.error {
            body = body.child(super::notice::error(err, cx));
        }
        if confirm || waiting || notice {
            let text = self.model["text"].as_str().unwrap_or_default().to_owned();
            if notice && self.model["markdown"] == true {
                if let Some(meta) = self.model["meta"].as_array().filter(|m| !m.is_empty()) {
                    body = body.child(meta_header(meta, cx));
                }
                body = body.child(
                    div()
                        .id("prompt-markdown")
                        .test_support()
                        .role(Role::Document)
                        .when(!text.trim().is_empty(), |el| el.aria_label(text.clone()))
                        .w_full()
                        .min_w_0()
                        .child(super::markdown::view("prompt-markdown", text, cx)),
                );
            } else {
                body = body.child(text);
            }
            // A device login also offers its page and its code.
            if waiting && (self.model["link"].is_string() || self.model["copy"].is_string()) {
                let (link, code) = (
                    self.model["link"].as_str().map(str::to_owned),
                    self.model["copy"].as_str().map(str::to_owned),
                );
                body = body.child(
                    div()
                        .flex()
                        .gap(px(SPACE_SM))
                        .when_some(link, |row, link| {
                            row.child(
                                Button::new("prompt-open-link")
                                    .h(px(CONTROL_HEIGHT))
                                    .label(tr("btn.open_browser"))
                                    .on_click(move |_, _, cx| cx.open_url(&link)),
                            )
                        })
                        .when_some(code, |row, code| {
                            row.child(
                                Button::new("prompt-copy-code")
                                    .h(px(CONTROL_HEIGHT))
                                    .label(tr("btn.copy_code"))
                                    .on_click(move |_, _, cx| {
                                        cx.write_to_clipboard(ClipboardItem::new_string(code.clone()))
                                    }),
                            )
                        }),
                );
            }
        } else if choice {
            for (index, option) in self.model["options"].as_array().into_iter().flatten().enumerate() {
                let id = option["id"].as_str().unwrap_or_default().to_owned();
                let selected = self.selected.as_ref() == Some(&id);
                body = body.child(
                    Button::new(("prompt-choice", index))
                        .h(px(CONTROL_HEIGHT))
                        .label(option["label"].as_str().unwrap_or_default().to_owned())
                        .disabled(self.pending)
                        .when(selected, |button| button.primary())
                        .on_click(cx.listener(move |prompt, _, _, cx| {
                            prompt.selected = Some(id.clone());
                            cx.notify();
                        })),
                );
            }
        } else if let Some(input) = &self.input {
            let label = self.model["prompt"].as_str().unwrap_or_default().to_owned();
            body = body.child(label.clone()).child(
                Input::new(input)
                    .small()
                    .min_h(px(CONTROL_HEIGHT))
                    .aria_label(label)
                    .disabled(self.pending),
            );
        }
        div()
            .id("prompt-form")
            .test_support()
            .w_full()
            .min_w_0()
            .max_h(px(super::dialog::fit_height(180., 160., 620., window)))
            .overflow_y_scroll()
            .child(body)
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
            .gap(px(SPACE_SM))
            .when(!notice, |row| {
                row.child(
                    Button::new("cancel-prompt")
                        .h(px(CONTROL_HEIGHT))
                        .label(no.to_owned())
                        .when(only_no, |button| button.primary())
                        .disabled(self.pending)
                        .on_click(cx.listener(|prompt, _, _, cx| prompt.cancel(cx))),
                )
            })
            .when(!waiting && !only_no, |row| {
                row.child(
                    Button::new("answer-prompt")
                        .h(px(CONTROL_HEIGHT))
                        .primary()
                        .label(yes.to_owned())
                        .disabled(self.pending || choice && self.selected.is_none())
                        .on_click(cx.listener(move |prompt, _, _, cx| {
                            // A notice is only acknowledged: it has no box to read.
                            let value = if confirm {
                                json!(true)
                            } else if choice {
                                let Some(selected) = prompt.selected.clone() else {
                                    return;
                                };
                                json!(selected)
                            } else if notice {
                                Value::Null
                            } else {
                                json!(prompt
                                    .input
                                    .as_ref()
                                    .map(|input| input.read(cx).value().to_string())
                                    .unwrap_or_default())
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
pub struct PromptButtons {
    prompt: WeakEntity<Prompt>,
    _subscription: Subscription,
}
impl Render for PromptButtons {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        match self.prompt.upgrade() {
            Some(prompt) => prompt.update(cx, |prompt, cx| prompt.buttons(cx)).into_any_element(),
            None => div().into_any_element(),
        }
    }
}

#[cfg(all(test, feature = "test-support"))]
mod tests {
    use super::Prompt;
    use gpui_kit::test::{TestQueryExt, TestWindowExt};
    use gpui_kit::{
        div, px, size, AppContext, Bounds, Entity, IntoElement, ParentElement, Render, Styled, TestAppContext, Window,
        WindowBounds, WindowOptions,
    };
    use serde_json::json;

    struct PromptHost(Entity<Prompt>);
    impl Render for PromptHost {
        fn render(&mut self, _: &mut Window, cx: &mut gpui_kit::Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .p_4()
                .flex()
                .flex_col()
                .child(self.0.clone())
                .child(self.0.read(cx).buttons_view())
        }
    }

    fn open_prompt(
        model: serde_json::Value,
        cx: &mut TestAppContext,
    ) -> (gpui_kit::WindowHandle<gpui_kit::base::Root>, Entity<Prompt>) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::theme::apply(true, cx);
        });
        let slot: std::rc::Rc<std::cell::RefCell<Option<Entity<Prompt>>>> =
            std::rc::Rc::new(std::cell::RefCell::new(None));
        let s = slot.clone();
        let window = cx.update(|cx| {
            let bounds = Bounds {
                origin: gpui_kit::Point::default(),
                size: size(px(540.), px(480.)),
            };
            let (window, _) = gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                cx,
                move |window, cx| {
                    let prompt = cx.new(|cx| Prompt::new(model, window, cx));
                    *s.borrow_mut() = Some(prompt.clone());
                    cx.new(|_| PromptHost(prompt))
                },
            )
            .expect("open test window");
            window.downcast::<gpui_kit::base::Root>().expect("root window")
        });
        let prompt = slot.borrow().clone().expect("prompt created");
        (window, prompt)
    }

    #[gpui_kit::test]
    fn choice_prompt_skips_input_allocation_and_updates_footer_when_selected(cx: &mut TestAppContext) {
        let model = json!({
            "id": 7,
            "kind": "choice",
            "title": "Choose broadcast",
            "options": [
                {"id": "b1", "label": "Main Stream"},
                {"id": "b2", "label": "Test Stream"}
            ]
        });
        let (window, prompt) = open_prompt(model, cx);
        cx.update_window(window.into(), |_, window, cx| {
            window.render_frame(cx);
            assert!(prompt.read(cx).input.is_none());
            assert!(!prompt.read(cx).pending);
            // OK starts disabled until an option is picked.
            window.click("answer-prompt", cx);
            assert!(!prompt.read(cx).pending);

            window.click(("prompt-choice", 1usize), cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(window.into(), |_, window, cx| {
            window.render_frame(cx);
            assert_eq!(prompt.read(cx).selected.as_deref(), Some("b2"));
            window.click("answer-prompt", cx);
            assert!(prompt.read(cx).pending);
            // A second answer while pending is ignored.
            prompt.update(cx, |prompt, cx| prompt.cancel(cx));
            assert!(prompt.read(cx).pending);
        })
        .unwrap();
    }

    #[gpui_kit::test]
    fn prompt_without_id_refuses_to_send_and_notice_markdown_renders_meta(cx: &mut TestAppContext) {
        let missing_id = json!({
            "kind": "confirm",
            "text": "Proceed?",
            "yes": "Yes",
            "no": "No"
        });
        let (window, prompt) = open_prompt(missing_id, cx);
        cx.update_window(window.into(), |_, window, cx| {
            window.render_frame(cx);
            assert!(prompt.read(cx).input.is_none());
            prompt.update(cx, |prompt, cx| prompt.answer(json!(true), cx));
            assert!(!prompt.read(cx).pending);
        })
        .unwrap();

        let notice = json!({
            "id": 42,
            "kind": "notice",
            "title": "Help",
            "text": "Details here.",
            "markdown": true,
            "meta": [
                {"key": "Kind", "value": "trigger"},
                {"key": "Category", "value": "Twitch/Channel"},
                {"key": "Code", "value": "101"},
                {"key": "Needs", "value": "Twitch login"}
            ]
        });
        let (window, prompt) = open_prompt(notice, cx);
        cx.update_window(window.into(), |_, window, cx| {
            window.render_frame(cx);
            assert!(prompt.read(cx).input.is_none());
            let meta = window.find_by_label("Trigger, Twitch > Channel, #101, Needs: Twitch login");
            assert_eq!(
                meta.label(),
                Some("Trigger, Twitch > Channel, #101, Needs: Twitch login")
            );
        })
        .unwrap();
    }

    #[gpui_kit::test]
    fn input_prompt_allocates_input_and_answers_once(cx: &mut TestAppContext) {
        let model = json!({
            "id": 9,
            "kind": "input",
            "title": "Rename",
            "prompt": "New name",
            "text": "Initial"
        });
        let (window, prompt) = open_prompt(model, cx);
        cx.update_window(window.into(), |_, window, cx| {
            window.render_frame(cx);
            assert!(prompt.read(cx).input.is_some());
            window.click("answer-prompt", cx);
            assert!(prompt.read(cx).pending);
        })
        .unwrap();
    }
}
