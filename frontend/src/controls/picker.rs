//! Pick one leaf of a searchable tree, with its description beside it (triggers, sub-actions, templates).
use super::tree::{Node, Options, PaneEvent, TreePane};
use crate::i18n::tr;
use gpui_kit::component::{
    button::{Button, ButtonVariants},
    ActiveTheme, Disableable, WindowExt,
};
use gpui_kit::*;
use serde_json::Value;

/// The id of the leaf chosen.
pub struct Picked(pub String);
impl EventEmitter<Picked> for Picker {}

pub struct Picker {
    pane: Entity<TreePane>,
    chosen: Option<Node>,
    _subscription: Subscription,
}
impl Picker {
    pub fn new(model: Value, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let pane = cx.new(|cx| {
            TreePane::new(
                "picker",
                model,
                true,
                Options {
                    closed: true,
                    compact: true,
                    ..Options::default()
                },
                window,
                cx,
            )
        });
        let subscription = cx.subscribe(&pane, |picker, _, event: &PaneEvent, cx| match event {
            PaneEvent::Selected(node) => {
                picker.chosen = node.clone().filter(|node| node.kind == "item");
                cx.notify();
            }
            PaneEvent::Activate(node) if node.kind == "item" => cx.emit(Picked(node.id.clone())),
            _ => {}
        });
        Self {
            pane,
            chosen: None,
            _subscription: subscription,
        }
    }
}
impl Render for Picker {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let height = (f32::from(window.viewport_size().height) - 220.).clamp(240., 520.);
        let (title, text) = self
            .chosen
            .as_ref()
            .map(|node| {
                (
                    node.detail["title"].as_str().unwrap_or_default().to_owned(),
                    node.detail["text"].as_str().unwrap_or_default().to_owned(),
                )
            })
            .unwrap_or_default();
        let picked = self.chosen.as_ref().map(|node| node.id.clone());
        div()
            .id("picker")
            .test_support()
            .role(Role::Group)
            .flex()
            .flex_col()
            .gap_2()
            .child(div().h(px(height)).child(self.pane.clone()))
            .child(
                div()
                    .id("picker-description")
                    .h(px(64.))
                    .flex_shrink_0()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().font_weight(FontWeight::BOLD).child(title))
                    .child(div().text_color(cx.theme().muted_foreground).child(text)),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("cancel-picker")
                            .label(tr("dialogs.cancel"))
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    .child(
                        Button::new("pick")
                            .primary()
                            .label(tr("dialogs.ok"))
                            .disabled(picked.is_none())
                            .on_click(cx.listener(move |picker, _, _, cx| {
                                if let Some(node) = &picker.chosen {
                                    cx.emit(Picked(node.id.clone()));
                                }
                            })),
                    ),
            )
    }
}
