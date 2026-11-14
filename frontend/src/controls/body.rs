//! The inside of a button: a 14px icon in a quiet tone, then the label at 13px.
use gpui_kit::component::{ActiveTheme, Icon};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

/// The icon tone of buttons (dimmer than their label).
pub fn icon_tone(cx: &App) -> Hsla {
    rgb(if cx.theme().is_dark() { 0xbdbdbe } else { 0x434345 }).into()
}

/// Put it in a button with `.child(...)`, not `.label(...)`, so the label keeps the page's 13px size.
pub fn button_body(icon: Option<&str>, label: Option<String>, cx: &App) -> Div {
    div()
        .flex()
        .items_center()
        .gap_2()
        .text_size(px(13.))
        .when_some(icon, |body, icon| {
            body.child(
                Icon::empty()
                    .path(format!("icons/{icon}.svg"))
                    .size(px(14.))
                    .text_color(icon_tone(cx)),
            )
        })
        .when_some(label, |body, label| body.child(label))
}

/// Logos drawn in one light grey (the coding agents, OBS): they take the text's colour instead, so they
/// show on the light theme too. The test below keeps the list in step with assets/icons.
const ONE_COLOUR: [&str; 14] = [
    "antigravity",
    "auggie",
    "cline",
    "codex",
    "copilot",
    "cursor",
    "devin",
    "kimi",
    "kiro",
    "obsstudio",
    "opencode",
    "roocode",
    "windsurf",
    "zed",
];

/// A brand's logo (`assets/icons`), `size` square.
pub fn brand_logo(id: impl Into<ElementId>, name: &str, size: Pixels, cx: &App) -> AnyElement {
    let path = format!("brands/{name}.svg");
    if ONE_COLOUR.contains(&name) {
        div()
            .id(id)
            .size(size)
            .flex_shrink_0()
            .child(svg().path(path).size(size).text_color(cx.theme().foreground))
            .into_any_element()
    } else {
        img(path).id(id).size(size).flex_shrink_0().into_any_element()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_light_grey_logo_follows_the_theme() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/icons");
        let mut grey: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .filter_map(|entry| {
                let text = std::fs::read_to_string(entry.path()).ok()?;
                let colours: std::collections::BTreeSet<String> = text
                    .match_indices('#')
                    .filter_map(|(at, _)| text.get(at + 1..at + 7))
                    .filter(|hex| hex.chars().all(|c| c.is_ascii_hexdigit()))
                    .map(str::to_ascii_uppercase)
                    .collect();
                (colours.len() == 1 && colours.contains("C8C8C8"))
                    .then(|| entry.path().file_stem()?.to_str().map(str::to_owned))
                    .flatten()
            })
            .collect();
        grey.sort();
        assert_eq!(grey, super::ONE_COLOUR);
    }
}
