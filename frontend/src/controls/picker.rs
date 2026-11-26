//! Pick one leaf of a searchable tree, with its description beside it (triggers, sub-actions, templates).
//! The tree is on the left, with search, Expand all and Collapse all; the selection's icon, path and
//! description are on the right (under the tree when the window is narrow). Cancel and OK go in the
//! dialog's footer.
use super::form::Form;
use super::slot::Slot;
use super::tree::{Node, Options, PaneEvent, TreePane};
use crate::i18n::tr;
use crate::theme::{CONTROL_HEIGHT, FONT_BODY, FONT_CAPTION, LINE_BODY, LINE_CAPTION, SPACE_LG, SPACE_MD, SPACE_SM};
use gpui_kit::component::{
    button::{Button, ButtonVariants},
    ActiveTheme, Disableable, Icon,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use serde_json::Value;

/// The id of the leaf chosen.
pub struct Picked(pub String);
impl EventEmitter<Picked> for Picker {}
pub struct Chosen(pub String);
impl EventEmitter<Chosen> for Picker {}

/// Under this window width the description goes under the tree.
const NARROW: f32 = 700.;

pub struct Picker {
    pane: Entity<TreePane>,
    chosen: Option<Node>,
    form: Option<Entity<Form>>,
    authoring: bool,
    /// What the panel says while nothing is picked.
    hint: String,
    buttons_view: Entity<Slot<Self>>,
    _subscription: Subscription,
}
impl Picker {
    pub fn new(model: Value, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let hint = model["hint"].as_str().unwrap_or_default().to_owned();
        let authoring = matches!(model["kind"].as_str(), Some("trigger" | "sub" | "template"));
        let fold_buttons = super::tree::has_folders(&model["nodes"]);
        let pane = cx.new(|cx| {
            TreePane::new(
                "picker",
                model,
                true,
                Options {
                    closed: true,
                    compact: true,
                    fold_buttons,
                    ..Options::default()
                },
                window,
                cx,
            )
        });
        let subscription = cx.subscribe_in(&pane, window, |picker, _, event: &PaneEvent, _window, cx| match event {
            PaneEvent::Selected(node) => {
                picker.chosen = node.clone().filter(|node| node.kind == "item");
                picker.form = None;
                if picker.authoring {
                    cx.emit(Chosen(
                        picker.chosen.as_ref().map(|node| node.id.clone()).unwrap_or_default(),
                    ));
                }
                cx.notify();
            }
            PaneEvent::Activate(node) if node.kind == "item" && !picker.authoring => cx.emit(Picked(node.id.clone())),
            PaneEvent::Activate(node) if node.kind == "item" => {
                if let Some(form) = &picker.form {
                    form.update(cx, |form, cx| form.submit(false, cx));
                }
            }
            _ => {}
        });
        Self {
            pane,
            chosen: None,
            form: None,
            authoring,
            hint,
            buttons_view: Slot::new(Self::buttons, cx),
            _subscription: subscription,
        }
    }

    /// The width the dialog takes in this window.
    pub fn width(window: &Window) -> f32 {
        super::dialog::fit_width(780., window)
    }

    /// The buttons as their own view, for the dialog's footer.
    pub fn buttons_view(&self) -> Entity<Slot<Self>> {
        self.buttons_view.clone()
    }

    pub fn set_form(&mut self, form: Entity<Form>, cx: &mut Context<Self>) {
        self.form = Some(form);
        cx.notify();
    }

    /// The picker's Cancel and OK, or its attached form's footer, drawn in the dialog's footer.
    fn buttons(&mut self, _: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        if let Some(form) = &self.form {
            return form.read(cx).buttons_view().into_any_element();
        }
        let right = div()
            .flex()
            .gap(px(SPACE_SM))
            .child(
                Button::new("cancel-picker")
                    .h(px(CONTROL_HEIGHT))
                    .label(tr("dialogs.cancel"))
                    .on_click(|_, window, cx| crate::controls::dialog::dismiss(window, cx)),
            )
            .child(
                Button::new("pick")
                    .h(px(CONTROL_HEIGHT))
                    .primary()
                    .label(tr("btn.add"))
                    .disabled(self.chosen.is_none() || self.authoring)
                    .on_click(cx.listener(|picker, _, _, cx| picker.pick(cx))),
            );
        super::dialog::footer(None, right, cx).into_any_element()
    }

    fn pick(&mut self, cx: &mut Context<Self>) {
        if self.authoring {
            return;
        }
        if let Some(node) = &self.chosen {
            cx.emit(Picked(node.id.clone()));
        }
    }

    /// The selection's icon, name, path and description, or a hint while nothing is picked.
    fn detail(&self, cx: &App) -> AnyElement {
        let theme = cx.theme();
        let Some(node) = &self.chosen else {
            return div()
                .size_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(SPACE_SM))
                .p(px(SPACE_LG))
                .text_center()
                .text_size(px(FONT_BODY))
                .line_height(px(LINE_BODY))
                .text_color(theme.muted_foreground)
                .child(Icon::empty().path("icons/info.svg").size_5())
                .child(self.hint.clone())
                .into_any_element();
        };
        let detail = &node.detail;
        let text = |key: &str| detail[key].as_str().unwrap_or_default().to_owned();
        let badge = div()
            .size(px(36.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(8.))
            .bg(theme.secondary)
            .child(match detail["logo"].as_str() {
                Some(logo) => img(logo.to_owned()).id("picker-logo").size_5().into_any_element(),
                None => Icon::empty()
                    .path(format!("icons/{}.svg", detail["icon"].as_str().unwrap_or("circle")))
                    .size_5()
                    .into_any_element(),
            });
        let path = text("path");
        div()
            .id("picker-description")
            .size_full()
            .overflow_y_scroll()
            .p(px(SPACE_MD))
            .flex()
            .flex_col()
            .gap(px(SPACE_MD))
            .child(
                div().flex().items_center().gap(px(SPACE_MD)).child(badge).child(
                    div()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .font_weight(FontWeight::BOLD)
                                .text_color(crate::theme::strong(cx))
                                .child(text("title")),
                        )
                        .when(!path.is_empty(), |column| {
                            column.child(
                                div()
                                    .text_size(px(FONT_CAPTION))
                                    .line_height(px(LINE_CAPTION))
                                    .text_color(theme.muted_foreground)
                                    .child(path),
                            )
                        }),
                ),
            )
            .child(
                div()
                    .text_size(px(FONT_BODY))
                    .line_height(px(LINE_BODY))
                    .text_color(theme.foreground)
                    .child(super::markdown::inline("picker-text", text("text"))),
            )
            .when_some(detail["values"].as_str(), |panel, values| {
                panel.child(super::notice::info(values, cx))
            })
            // Why it can't run yet (not logged in, more permissions needed) and what to do.
            .when_some(detail["unready"].as_str(), |panel, why| {
                panel.child(super::notice::info(why, cx))
            })
            .when_some(self.form.clone(), |panel, form| panel.child(form))
            .into_any_element()
    }
}
impl Render for Picker {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let narrow = f32::from(window.viewport_size().width) < NARROW;
        let height = super::dialog::fit_height(180., 220., 650., window);
        let panel = div()
            .rounded(cx.theme().radius)
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().table)
            .child(self.detail(cx));
        let layout = if narrow {
            div()
                .flex()
                .flex_col()
                .gap(px(SPACE_SM))
                .h(px(height))
                .child(div().h(px(150.)).flex_shrink_0().child(self.pane.clone()))
                .child(panel.flex_1().min_h_0())
        } else {
            div()
                .flex()
                .gap(px(SPACE_MD))
                .h(px(height))
                .child(div().w(px(280.)).flex_shrink_0().h_full().child(self.pane.clone()))
                // Lined up with the tree, under the search row.
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .pt(px(CONTROL_HEIGHT + SPACE_SM))
                        .child(panel.size_full()),
                )
        };
        div().id("picker").test_support().role(Role::Group).child(layout)
    }
}

#[cfg(all(test, feature = "test-support"))]
mod tests {
    use super::{Chosen, Picked, Picker};
    use crate::controls::form::Form;
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{
        div, px, size, AppContext, Bounds, Entity, IntoElement, ParentElement, Render, Styled, TestAppContext, Window,
        WindowBounds, WindowOptions,
    };
    use serde_json::json;

    struct PickerHost(Entity<Picker>);
    impl Render for PickerHost {
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

    fn open_picker(
        model: serde_json::Value,
        width: f32,
        cx: &mut TestAppContext,
    ) -> (gpui_kit::WindowHandle<gpui_kit::base::Root>, Entity<Picker>) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::theme::apply(true, cx);
        });
        let slot: std::rc::Rc<std::cell::RefCell<Option<Entity<Picker>>>> =
            std::rc::Rc::new(std::cell::RefCell::new(None));
        let s = slot.clone();
        let window = cx.update(|cx| {
            let bounds = Bounds {
                origin: gpui_kit::Point::default(),
                size: size(px(width), px(620.)),
            };
            let (window, _) = gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                cx,
                move |window, cx| {
                    let picker = cx.new(|cx| Picker::new(model, window, cx));
                    *s.borrow_mut() = Some(picker.clone());
                    cx.new(|_| PickerHost(picker))
                },
            )
            .expect("open test window");
            window.downcast::<gpui_kit::base::Root>().expect("root window")
        });
        let picker = slot.borrow().clone().expect("picker created");
        (window, picker)
    }

    fn sample_model(kind: &str) -> serde_json::Value {
        json!({
            "kind": kind,
            "hint": "Select an item",
            "nodes": [
                {
                    "id": "cat-twitch",
                    "kind": "category",
                    "label": "Twitch",
                    "children": [
                        {
                            "id": "101",
                            "kind": "item",
                            "label": "Follow",
                            "detail": {
                                "title": "Follow",
                                "path": "Twitch > Channel",
                                "text": "Fires on a new follower."
                            }
                        }
                    ]
                }
            ]
        })
    }

    #[gpui_kit::test]
    fn authoring_picker_emits_chosen_and_delegates_footer_to_form(cx: &mut TestAppContext) {
        let (window, picker) = open_picker(sample_model("trigger"), 900., cx);
        let chosen = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
        let chosen_cb = chosen.clone();
        let picked = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
        let picked_cb = picked.clone();

        cx.update_window(window.into(), |_, window, cx| {
            cx.subscribe(&picker, move |_, event: &Chosen, _| {
                chosen_cb.borrow_mut().push(event.0.clone());
            })
            .detach();
            cx.subscribe(&picker, move |_, event: &Picked, _| {
                picked_cb.borrow_mut().push(event.0.clone());
            })
            .detach();
            window.render_frame(cx);

            // Selecting a category keeps chosen empty.
            let pane = picker.read(cx).pane.clone();
            pane.update(cx, |pane, cx| pane.select("cat-twitch", cx));
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(window.into(), |_, window, cx| {
            assert!(picker.read(cx).chosen.is_none());
            // Selecting a leaf item emits Chosen and populates detail.
            let pane = picker.read(cx).pane.clone();
            pane.update(cx, |pane, cx| pane.select("101", cx));
            window.render_frame(cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(window.into(), |_, window, cx| {
            assert_eq!(picker.read(cx).chosen.as_ref().map(|n| n.id.as_str()), Some("101"));
            assert_eq!(chosen.borrow().last().map(String::as_str), Some("101"));
            // Direct pick() is a no-op while authoring.
            picker.update(cx, |picker, cx| picker.pick(cx));
            assert!(picked.borrow().is_empty());

            // Attaching a form delegates footer rendering without panicking.
            let form = cx.new(|cx| {
                Form::new(
                    json!({"title": "Follow", "fields": []}),
                    "actions:1:save".into(),
                    window,
                    cx,
                )
            });
            picker.update(cx, |picker, cx| picker.set_form(form, cx));
            window.render_frame(cx);
            assert!(picker.read(cx).form.is_some());
        })
        .unwrap();
    }

    #[gpui_kit::test]
    fn non_authoring_picker_emits_picked_and_renders_narrow_layout(cx: &mut TestAppContext) {
        let (window, picker) = open_picker(sample_model("sound"), 540., cx);
        let picked = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
        let picked_cb = picked.clone();

        cx.update_window(window.into(), |_, window, cx| {
            cx.subscribe(&picker, move |_, event: &Picked, _| {
                picked_cb.borrow_mut().push(event.0.clone());
            })
            .detach();
            window.render_frame(cx);
            let pane = picker.read(cx).pane.clone();
            pane.update(cx, |pane, cx| pane.select("101", cx));
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(window.into(), |_, window, cx| {
            window.render_frame(cx);
            window.click("pick", cx);
        })
        .unwrap();
        cx.run_until_parked();
        assert_eq!(*picked.borrow(), vec!["101"]);
    }
}
