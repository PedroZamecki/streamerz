//! Development instrumentation reads the actual GPUI frame and captures the renderer's own window.

use crate::Frontend;
use gpui_kit::component::WindowExt;
use gpui_kit::*;
use serde_json::json;

pub fn dispatch(message: &serde_json::Value, handle: AnyWindowHandle, view: &Entity<Frontend>, cx: &mut AsyncApp) {
    let command = message["command"].as_str().unwrap_or_default().to_owned();
    let id = message["commandId"].clone();
    let view = view.clone();
    let failed_id = id.clone();
    if let Err(error) = handle.update(cx, move |_, window, _| {
        window.refresh();
        // Let the normal platform loop draw and present. Calling draw here would consume the dirty
        // state without presenting it, leaving native captures one page behind the semantic tree.
        window.on_next_frame(move |window, _| {
            window.on_next_frame(move |window, cx| {
                let result = run(&command, &view, window, cx);
                crate::send(json!({"request": "FrontendDevResult", "commandId": id, "result": result}));
            })
        });
    }) {
        crate::send(json!({"request": "FrontendDevResult", "commandId": failed_id, "result": error.to_string()}));
    }
}

fn run(command: &str, view: &Entity<Frontend>, window: &mut Window, cx: &mut App) -> String {
    let (name, argument) = command.split_once(' ').unwrap_or((command, ""));
    match name {
        "page" => {
            let known = view
                .read(cx)
                .navigation
                .as_array()
                .into_iter()
                .flatten()
                .any(|section| {
                    section["pages"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .any(|page| page["id"] == argument)
                });
            if !known {
                return "page not found".into();
            }
            view.update(cx, |view, cx| view.select(argument, window, cx));
            "ok".into()
        }
        "resize" => {
            let mut parts = argument.split_whitespace().map(str::parse::<f32>);
            match (parts.next(), parts.next()) {
                (Some(Ok(width)), Some(Ok(height))) => {
                    window.resize(size(px(width), px(height)));
                    "ok".into()
                }
                _ => "usage: resize WIDTH HEIGHT".into(),
            }
        }
        "process" => std::process::id().to_string(),
        "currentpage" => view.read(cx).selected.clone(),
        "pageready" => view.read(cx).page_ready().to_string(),
        "renderer" => format!("{:?}", window.gpu_specs()),
        "shot" => {
            let path = argument.trim();
            match window.render_to_image() {
                Ok(image) => image
                    .save(path)
                    .map_or_else(|error| error.to_string(), |()| format!("saved {path}")),
                Err(error) if error.to_string().contains("render_to_image not implemented") => {
                    crate::capture::save(window, path).map_or_else(|error| error, |()| format!("saved {path}"))
                }
                Err(error) => error.to_string(),
            }
        }
        "windows" => {
            let bounds = window.bounds();
            format!(
                "0: 'Streamerz' at {},{} {}x{} shown",
                f32::from(bounds.origin.x),
                f32::from(bounds.origin.y),
                f32::from(bounds.size.width),
                f32::from(bounds.size.height)
            )
        }
        "tree" | "find" | "bounds" => {
            let origin = window.bounds().origin;
            let snapshots = gpui_kit::base::test_support::snapshots(window);
            let dialog_open = window.has_active_dialog(cx);
            let modal = snapshots
                .iter()
                .filter(|entry| {
                    entry.visible()
                        && (entry.role() == Some(Role::Menu)
                            || dialog_open && matches!(entry.role(), Some(Role::Dialog | Role::AlertDialog)))
                })
                .max_by_key(|entry| (usize::from(entry.role() == Some(Role::Menu)), entry.path().len()))
                .map(|entry| entry.bounds());
            let modal = modal.or_else(|| {
                dialog_open
                    .then(|| {
                        ["delete-confirmation", "prompt-form", "editor-form"]
                            .into_iter()
                            .find_map(|id| {
                                snapshots
                                    .iter()
                                    .find(|entry| entry.visible() && entry.path().last() == Some(&ElementId::from(id)))
                                    .map(|entry| entry.bounds())
                            })
                    })
                    .flatten()
            });
            let mut snapshots = snapshots
                .into_iter()
                .filter(|entry| modal.is_none_or(|bounds| bounds.contains(&entry.bounds().center())))
                .collect::<Vec<_>>();
            snapshots.sort_by_key(|entry| {
                let priority = match entry.role() {
                    Some(Role::TextInput | Role::MultilineTextInput | Role::ComboBox) => 0,
                    Some(Role::Button | Role::Switch | Role::CheckBox) => 1,
                    _ => 2,
                };
                (priority, entry.bounds().origin.y, entry.bounds().origin.x)
            });
            if name == "bounds" {
                let entries: Vec<_> = snapshots.iter().filter(|entry| entry.visible() && entry.label().is_some_and(|label| label.to_lowercase().contains(&argument.to_lowercase()))).map(|entry| {
                        let bounds = entry.bounds();
                        json!({"label": entry.label(), "enabled": entry.disabled() != Some(true), "role": format!("{:?}", entry.role()), "path": format!("{:?}", entry.path()), "x": f32::from(bounds.origin.x + origin.x), "y": f32::from(bounds.origin.y + origin.y), "w": f32::from(bounds.size.width), "h": f32::from(bounds.size.height)})
                    }).collect();
                return json!(entries).to_string();
            }
            snapshots
                .iter()
                .filter(|entry| entry.visible())
                .filter_map(|entry| {
                    let label = entry.label()?;
                    let bounds = entry.bounds();
                    if name == "find" {
                        if !label.to_lowercase().contains(&argument.to_lowercase()) {
                            return None;
                        }
                        Some(format!(
                            "{label:?} at {},{}",
                            f32::from(origin.x + bounds.origin.x + bounds.size.width / 2.).round() as i32,
                            f32::from(origin.y + bounds.origin.y + bounds.size.height / 2.).round() as i32
                        ))
                    } else {
                        Some(format!(
                            "{label:?} {},{} {}x{}{}",
                            f32::from(bounds.origin.x),
                            f32::from(bounds.origin.y),
                            f32::from(bounds.size.width),
                            f32::from(bounds.size.height),
                            if entry.disabled() == Some(true) {
                                " (disabled)"
                            } else {
                                ""
                            }
                        ))
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
        }
        "overflow" => json!(overflow(window)).to_string(),
        _ => "unknown frontend command".into(),
    }
}

/// What the frame cuts off sideways: buttons, boxes and fields that their clip (a dialog, a card, the
/// window) only partly shows, such as a button pushed past a dialog's edge. Up and down is a scroll's
/// business, so only left and right count. Each finding is a box in window pixels.
fn overflow(window: &Window) -> Vec<serde_json::Value> {
    let scale = window.scale_factor();
    let width = f32::from(window.viewport_size().width) * scale;
    let mut found = Vec::new();
    for quad in window.painted_quads() {
        let (bounds, mask) = (quad.bounds, quad.content_mask.bounds);
        let (left, right) = (bounds.origin.x.0, bounds.origin.x.0 + bounds.size.width.0);
        let (from, to) = (
            mask.origin.x.0.max(0.),
            (mask.origin.x.0 + mask.size.width.0).min(width),
        );
        // A quad that paints nothing hides nothing; a rounded corner is a quad seen through a slice of
        // its own width, not a clip.
        let widths = &quad.border_widths;
        let bordered = !quad.border_color.is_transparent()
            && [widths.top, widths.right, widths.bottom, widths.left]
                .iter()
                .any(|w| w.0 > 0.);
        if (quad.background.is_transparent() && !bordered) || to - from < 16. * scale {
            continue;
        }
        // A table's header runs under its scrollbar's gutter, from the table's own left edge.
        if (left - from).abs() <= scale && right - to <= 14. * scale {
            continue;
        }
        // Only what is partly shown: wholly hidden is a scrolled-away row or a closed drawer.
        if right > from && left < to && (left < from - scale || right > to + scale) {
            let seen = |f: &serde_json::Value| {
                f["x"] == (left / scale).round() && f["y"] == (bounds.origin.y.0 / scale).round()
            };
            if found.iter().any(seen) {
                continue;
            }
            found.push(json!({
                "x": (left / scale).round(), "y": (bounds.origin.y.0 / scale).round(),
                "w": (bounds.size.width.0 / scale).round(), "h": (bounds.size.height.0 / scale).round(),
                "clip": [(from / scale).round(), (to / scale).round()],
            }));
        }
    }
    found
}
