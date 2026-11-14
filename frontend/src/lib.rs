mod app;
mod assets;
#[cfg(feature = "test-support")]
mod capture;
mod controls;
#[cfg(feature = "test-support")]
mod dev;
mod i18n;
mod theme;

// Presentation process. It exchanges data with the core through inherited pipes only.

use gpui_kit::*;
use serde_json::json;
use serde_json::Value;
use std::io::{BufRead, Write};
use std::sync::{Arc, Mutex};

use app::Frontend;

gpui_kit::actions!(streamerz, [CoreUndo, CoreRedo]);

/// A picture the app is built with, by the path the window loads it from: a platform's logo
/// (`brands/twitch.svg`), a badge (`badges/kick-vip.svg`) or a Lucide icon (`icons/heart.svg`).
/// Overlay pages are served the same ones, so they are in the program once.
pub fn asset(path: &str) -> Option<std::borrow::Cow<'static, [u8]>> {
    assets::Assets.load(path).ok().flatten()
}

fn send(value: Value) {
    let mut output = std::io::stdout().lock();
    if serde_json::to_writer(&mut output, &value).is_ok() {
        let _ = output.write_all(b"\n");
        let _ = output.flush();
    }
}

/// Enter only in the renderer child. Stdout is reserved for the private core protocol.
pub fn run() -> Result<(), String> {
    let mut input = std::io::BufReader::new(std::io::stdin());
    let mut line = String::new();
    input.read_line(&mut line).map_err(|error| error.to_string())?;
    let mut ready: Value = serde_json::from_str(&line).map_err(|error| error.to_string())?;
    if ready["event"] != "core.ready" || ready["protocol"] != 1 {
        return Err("incompatible core protocol".into());
    }
    i18n::init(ready["translations"].take())?;
    let input = Arc::new(Mutex::new(input));
    application().with_assets(assets::Assets).run(move |cx| {
        cx.set_app_identity("streamerz", "Streamerz");
        gpui_kit::init(cx);
        cx.bind_keys([
            KeyBinding::new("ctrl-z", CoreUndo, None),
            KeyBinding::new("ctrl-y", CoreRedo, None),
            KeyBinding::new("ctrl-shift-z", CoreRedo, None),
            #[cfg(target_os = "macos")]
            KeyBinding::new("cmd-z", CoreUndo, None),
            #[cfg(target_os = "macos")]
            KeyBinding::new("cmd-shift-z", CoreRedo, None),
            #[cfg(not(target_os = "macos"))]
            KeyBinding::new("ctrl-shift-z", gpui_kit::base::input::Redo, Some("Input")),
        ]);
        cx.set_reduce_motion(!ready["animations"].as_bool().unwrap_or(true));
        // One static sans in two weights: the system font may be a variable font that GPUI cannot make bold.
        let _ = cx.text_system().add_fonts(vec![
            std::borrow::Cow::Borrowed(include_bytes!("../../assets/fonts/StreamerzSans-Regular.ttf").as_slice()),
            std::borrow::Cow::Borrowed(include_bytes!("../../assets/fonts/StreamerzSans-Bold.ttf").as_slice()),
        ]);
        theme::apply(ready["dark"].as_bool().unwrap_or(true), cx);
        let win_cfg = &ready["window"];
        let w = win_cfg["w"]
            .as_f64()
            .map(|v| v as f32)
            .filter(|&v| v >= 400.0)
            .unwrap_or(1100.0);
        let h = win_cfg["h"]
            .as_f64()
            .map(|v| v as f32)
            .filter(|&v| v >= 300.0)
            .unwrap_or(800.0);
        let x = win_cfg["x"].as_f64().map(|v| v as f32);
        let y = win_cfg["y"].as_f64().map(|v| v as f32);
        let maximized = win_cfg["maximized"].as_bool().unwrap_or(false);
        let initial_tab = win_cfg["tab"]
            .as_str()
            .filter(|t| !t.is_empty())
            .unwrap_or("Chat")
            .to_owned();

        let screens: Vec<Bounds<Pixels>> = cx.displays().iter().map(|d| d.visible_bounds()).collect();
        let primary = cx.primary_display().map(|d| d.visible_bounds());
        let saved = x.zip(y).map(|(x, y)| point(px(x), px(y)));
        let bounds = fit(saved, size(px(w), px(h)), &screens, primary);

        let window_bounds = if maximized {
            WindowBounds::Maximized(bounds)
        } else {
            WindowBounds::Windowed(bounds)
        };

        let (window_handle, view) = gpui_kit::open_window(
            WindowOptions {
                app_id: Some("streamerz".into()),
                window_bounds: Some(window_bounds),
                titlebar: Some(TitlebarOptions {
                    title: Some(ready["title"].as_str().unwrap_or("Streamerz").to_owned().into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            cx,
            |window, cx| {
                let initial_tab_clone = initial_tab.clone();
                let frontend = cx.new(|cx| {
                    cx.observe_window_bounds(window, move |this: &mut Frontend, window, cx| {
                        this.window_bounds_changed(window, cx);
                    })
                    .detach();
                    Frontend::new(
                        ready["navigation"].clone(),
                        ready["title"].as_str().unwrap_or("Streamerz").to_owned(),
                        initial_tab_clone,
                    )
                });
                window.on_window_should_close(cx, move |window, _| {
                    let (bounds, maximized) = match window.window_bounds() {
                        WindowBounds::Maximized(b) => (b, true),
                        WindowBounds::Windowed(b) => (b, false),
                        WindowBounds::Fullscreen(b) => (b, false),
                    };
                    let (x, y, w, h) = (
                        f32::from(bounds.origin.x) as i32,
                        f32::from(bounds.origin.y) as i32,
                        f32::from(bounds.size.width) as i32,
                        f32::from(bounds.size.height) as i32,
                    );
                    if w >= 400 && h >= 300 {
                        send(json!({
                            "request": "StreamerzWindowState",
                            "x": x,
                            "y": y,
                            "w": w,
                            "h": h,
                            "maximized": maximized,
                        }));
                    }
                    true
                });
                frontend
            },
        )
        .expect("Streamerz could not create its GPUI window");
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        view.update(cx, |view, _| {
            view.request_read();
            view.request_status();
        });
        send(json!({"request":"StreamerzPrompts", "id":"prompts"}));
        let executor = cx.background_executor().clone();
        cx.spawn(async move |cx| loop {
            let input = input.clone();
            let result = executor
                .spawn(async move {
                    let mut line = String::new();
                    let count = input
                        .lock()
                        .map_err(|_| "frontend input lock failed".to_owned())?
                        .read_line(&mut line)
                        .map_err(|error| error.to_string())?;
                    if count == 0 {
                        return Ok(None);
                    }
                    serde_json::from_str::<Value>(&line)
                        .map(Some)
                        .map_err(|error| error.to_string())
                })
                .await;
            match result {
                Ok(Some(value)) => {
                    if value["event"] == "frontend.dev" {
                        #[cfg(feature = "test-support")]
                        dev::dispatch(&value, window_handle, &view, cx);
                        #[cfg(not(feature = "test-support"))]
                        send(json!({"request": "FrontendDevResult", "commandId": value["commandId"], "result": "frontend instrumentation requires dev-ui"}));
                    } else if value["event"] == "core.changed" {
                        if let Some(dark) = value["appearance"]["dark"].as_bool() {
                            cx.update(|cx| {
                                if gpui_kit::component::Theme::global(cx).is_dark() != dark { theme::apply(dark, cx); }
                                cx.set_reduce_motion(!value["appearance"]["animations"].as_bool().unwrap_or(true));
                            });
                        }
                        if value["topics"].as_array().is_some_and(|topics| topics.iter().any(|topic| topic == "prompts")) {
                            send(json!({"request":"StreamerzPrompts", "id":"prompts"}));
                        }
                        view.update(cx, |view, cx| {
                            view.request_status();
                            let topics = value["topics"].as_array();
                            view.images_for_forms(topics, cx);
                            if view.chat_changed(topics, cx) {
                                return;
                            }
                            let relevant = topics.is_none_or(|topics| topics.iter().any(|topic| topic == "config" || (topic == "chat" && view.selected == "Chat") || (topic == "log" && view.selected == "Log") || (topic == "images" && view.selected == "Accounts")));
                            if relevant { view.request_read(); }
                        });
                    } else {
                        let _ = window_handle.update(cx, |_, window, cx| {
                            view.update(cx, |view, cx| view.apply(value, window, cx))
                        });
                    }
                }
                _ => {
                    cx.update(|cx| cx.quit());
                    break;
                }
            }
        })
        .detach();
    });
    Ok(())
}

/// Room around the window for what the system draws: Windows' title bar and borders sit outside the
/// size asked for.
const FRAME: f32 = 48.;

/// Where the window opens: where it was, if that is still on a screen, else centred on the primary
/// one; never larger than the screen's free area (without the taskbar) with room for the frame.
fn fit(
    saved: Option<Point<Pixels>>,
    wanted: Size<Pixels>,
    screens: &[Bounds<Pixels>],
    primary: Option<Bounds<Pixels>>,
) -> Bounds<Pixels> {
    let on = |screen: &Bounds<Pixels>, at: Point<Pixels>| {
        let centre = point(at.x + wanted.width / 2., at.y + wanted.height / 2.);
        screen.contains(&centre)
    };
    let screen = saved
        .and_then(|at| screens.iter().find(|s| on(s, at)).copied())
        .or(primary)
        .or_else(|| screens.first().copied());
    let Some(screen) = screen else {
        return Bounds::new(saved.unwrap_or_default(), wanted);
    };
    let room = size(
        (screen.size.width - px(FRAME / 3.)).max(px(400.)),
        (screen.size.height - px(FRAME)).max(px(300.)),
    );
    let size = size(wanted.width.min(room.width), wanted.height.min(room.height));
    let top = screen.origin.y + px(FRAME * 2. / 3.);
    let origin = match saved.filter(|at| on(&screen, *at)) {
        Some(at) => point(
            at.x.max(screen.origin.x).min(screen.right() - size.width),
            at.y.max(top).min(screen.bottom() - size.height),
        ),
        None => point(
            screen.center().x - size.width / 2.,
            (screen.center().y - size.height / 2.).max(top),
        ),
    };
    Bounds::new(origin, size)
}

#[cfg(test)]
mod fit_tests {
    use super::{fit, FRAME};
    use gpui_kit::{point, px, size, Bounds, Pixels};

    /// 1920x1080 at 125% with a 60 px taskbar: 1536x816 logical, where the default 1100x800 window once
    /// opened with its title bar above the screen.
    fn screen() -> Bounds<Pixels> {
        Bounds::new(point(px(0.), px(0.)), size(px(1536.), px(816.)))
    }

    #[test]
    fn the_window_and_its_frame_fit_the_free_area() {
        let b = fit(None, size(px(1100.), px(800.)), &[screen()], Some(screen()));
        assert!(b.origin.y >= px(FRAME * 2. / 3.), "{b:?}");
        assert!(b.bottom() <= screen().bottom(), "{b:?}");
        assert!(b.origin.x >= px(0.) && b.right() <= screen().right(), "{b:?}");
    }

    #[test]
    fn a_saved_place_on_a_screen_gone_comes_back_to_one() {
        let b = fit(
            Some(point(px(3000.), px(-200.))),
            size(px(1100.), px(700.)),
            &[screen()],
            Some(screen()),
        );
        assert!(screen().contains(&b.center()), "{b:?}");
        // On a screen, it stays where it was.
        let kept = fit(
            Some(point(px(100.), px(80.))),
            size(px(800.), px(600.)),
            &[screen()],
            Some(screen()),
        );
        assert_eq!(kept.origin, point(px(100.), px(80.)));
    }
}
