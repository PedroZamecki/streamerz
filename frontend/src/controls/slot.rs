//! A small view that draws one part of an owner entity elsewhere in the layout (a dialog's footer,
//! a page's header actions, or a list's bottom button rows) and repaints whenever its owner does.
use gpui_kit::*;

/// Draws a slice of `T` in another slot of the window, staying in sync with `T`'s notifications.
pub struct Slot<T: 'static> {
    owner: WeakEntity<T>,
    draw: fn(&mut T, &mut Window, &mut Context<T>) -> AnyElement,
    _subscription: Subscription,
}

impl<T: 'static> Slot<T> {
    /// Creates a slot view observing `cx.entity()` and drawing it with `draw`.
    pub fn new(draw: fn(&mut T, &mut Window, &mut Context<T>) -> AnyElement, cx: &mut Context<T>) -> Entity<Self> {
        let entity = cx.entity();
        let owner = entity.downgrade();
        cx.new(|cx| {
            let sub = cx.observe(&entity, |_, _, cx| cx.notify());
            Self {
                owner,
                draw,
                _subscription: sub,
            }
        })
    }
}

impl<T: 'static> Render for Slot<T> {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        match self.owner.upgrade() {
            Some(owner) => owner.update(cx, |owner, cx| (self.draw)(owner, window, cx)),
            None => div().into_any_element(),
        }
    }
}

#[cfg(all(test, feature = "test-support"))]
mod tests {
    use super::Slot;
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{
        div, px, size, AppContext, Bounds, Context, Entity, InteractiveElement, IntoElement, ParentElement, Render,
        Role, StatefulInteractiveElement, Styled, TestAppContext, TestSupportExt, Window, WindowBounds, WindowOptions,
    };

    struct Demo {
        label: String,
        slot: Entity<Slot<Self>>,
    }

    impl Demo {
        fn new(label: &str, cx: &mut Context<Self>) -> Self {
            Self {
                label: label.to_owned(),
                slot: Slot::new(
                    |demo, _, _| {
                        div()
                            .id("demo-slot")
                            .test_support()
                            .role(Role::Group)
                            .aria_label(demo.label.clone())
                            .child(demo.label.clone())
                            .into_any_element()
                    },
                    cx,
                ),
            }
        }
    }

    impl Render for Demo {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
        }
    }

    struct SlotHost {
        owner: Option<Entity<Demo>>,
        slot: Entity<Slot<Demo>>,
    }

    impl Render for SlotHost {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size_full().p_4().child(self.slot.clone())
        }
    }

    #[gpui_kit::test]
    fn slot_renders_owner_updates_on_notify_and_handles_dropped_owner(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::theme::apply(true, cx);
        });
        let host_cell: std::rc::Rc<std::cell::RefCell<Option<Entity<SlotHost>>>> =
            std::rc::Rc::new(std::cell::RefCell::new(None));
        let host_ref = host_cell.clone();
        let window = cx.update(|cx| {
            let bounds = Bounds {
                origin: gpui_kit::Point::default(),
                size: size(px(400.), px(200.)),
            };
            let (window, _) = gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                cx,
                move |_, cx| {
                    let owner = cx.new(|cx| Demo::new("Initial", cx));
                    let slot = owner.read(cx).slot.clone();
                    let host = cx.new(|_| SlotHost {
                        owner: Some(owner),
                        slot,
                    });
                    *host_ref.borrow_mut() = Some(host.clone());
                    host
                },
            )
            .expect("open test window");
            window.downcast::<gpui_kit::base::Root>().expect("root window")
        });
        let host = host_cell.borrow().clone().expect("host created");

        cx.update_window(window.into(), |_, window, cx| {
            window.render_frame(cx);
            let el = window.find("demo-slot");
            assert_eq!(el.role(), Some(Role::Group));
            assert_eq!(el.label(), Some("Initial"));

            // Updating the owner repaints the slot through its observer.
            let owner = host.read(cx).owner.clone().expect("owner alive");
            owner.update(cx, |demo, cx| {
                demo.label = "Updated".to_owned();
                cx.notify();
            });
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(window.into(), |_, window, cx| {
            window.render_frame(cx);
            assert_eq!(window.find("demo-slot").label(), Some("Updated"));

            // Dropping the owner leaves the slot rendering an empty element cleanly.
            host.update(cx, |host, cx| {
                host.owner = None;
                cx.notify();
            });
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(window.into(), |_, window, cx| {
            window.render_frame(cx);
            assert!(window.try_find("demo-slot").is_none());
        })
        .unwrap();
    }
}
