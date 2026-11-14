//! Pick one leaf of a searchable tree, with its description beside it (triggers, sub-actions, templates).
//! The tree is on the left, with search, Expand all and Collapse all; the selection's icon, path and
//! description are on the right (under the tree when the window is narrow). Cancel and OK go in the
//! dialog's footer.
use super::tree::{Node, Options, PaneEvent, TreePane};
use crate::i18n::tr;
use gpui_kit::component::{
    button::{Button, ButtonVariants},
    ActiveTheme, Disableable, Icon, WindowExt,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use serde_json::Value;

/// The id of the leaf chosen.
pub struct Picked(pub String);
impl EventEmitter<Picked> for Picker {}

/// Under this window width the description goes under the tree.
const NARROW: f32 = 700.;

pub struct Picker {
    pane: Entity<TreePane>,
    chosen: Option<Node>,
    /// What the panel says while nothing is picked.
    hint: String,
    buttons_view: Entity<PickerButtons>,
    _subscription: Subscription,
}
impl Picker {
    pub fn new(model: Value, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let hint = model["hint"].as_str().unwrap_or_default().to_owned();
        let pane = cx.new(|cx| {
            TreePane::new(
                "picker",
                model,
                true,
                Options {
                    closed: true,
                    compact: true,
                    fold_buttons: true,
                    ..Options::default()
                },
                window,
                cx,
            )
        });
        let subscription = cx.subscribe(&pane, |picker, _, event: &PaneEvent, cx| match event {
            PaneEvent::Selected(node) => {
                picker.chosen = node.clone().filter(|node| node.kind == "item");
                picker.buttons_view.update(cx, |_, cx| cx.notify());
                cx.notify();
            }
            PaneEvent::Activate(node) if node.kind == "item" => cx.emit(Picked(node.id.clone())),
            _ => {}
        });
        let weak = cx.entity().downgrade();
        Self {
            pane,
            chosen: None,
            hint,
            buttons_view: cx.new(|_| PickerButtons(weak)),
            _subscription: subscription,
        }
    }

    /// The width the dialog takes in this window.
    pub fn width(window: &Window) -> f32 {
        (f32::from(window.viewport_size().width) - 32.).clamp(320., 780.)
    }

    /// The buttons as their own view, for the dialog's footer.
    pub fn buttons_view(&self) -> Entity<PickerButtons> {
        self.buttons_view.clone()
    }

    fn pick(&mut self, cx: &mut Context<Self>) {
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
                .gap_2()
                .p_4()
                .text_center()
                .text_size(px(13.))
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
            .p_3()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div().flex().items_center().gap_3().child(badge).child(
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
                            column.child(div().text_size(px(12.)).text_color(theme.muted_foreground).child(path))
                        }),
                ),
            )
            .child(
                div()
                    .text_size(px(13.))
                    .line_height(px(19.))
                    .text_color(theme.foreground)
                    .child(text("text")),
            )
            .when_some(detail["values"].as_str(), |panel, values| {
                panel.child(super::notice::info(values, cx))
            })
            .into_any_element()
    }
}
impl Render for Picker {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let narrow = f32::from(window.viewport_size().width) < NARROW;
        let height = (f32::from(window.viewport_size().height) - 240.).clamp(220., 480.);
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
                .gap_2()
                .child(div().h(px((height - 120.).max(180.))).child(self.pane.clone()))
                .child(panel.h(px(120.)).flex_shrink_0())
        } else {
            div()
                .flex()
                .gap_3()
                .h(px(height))
                .child(div().flex_1().min_w_0().h_full().child(self.pane.clone()))
                // Lined up with the tree, under the search row.
                .child(div().w(px(280.)).flex_shrink_0().pt(px(30.)).child(panel.size_full()))
        };
        div().id("picker").test_support().role(Role::Group).pt_2().child(layout)
    }
}

/// The picker's Cancel and OK, drawn in the dialog's footer.
pub struct PickerButtons(WeakEntity<Picker>);
impl Render for PickerButtons {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(picker) = self.0.upgrade() else {
            return div().into_any_element();
        };
        let picked = picker.read(cx).chosen.is_some();
        let right = div()
            .flex()
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
                    .disabled(!picked)
                    .on_click(move |_, _, cx| picker.update(cx, |picker, cx| picker.pick(cx))),
            );
        super::dialog::footer(None, right, cx).into_any_element()
    }
}
