//! The UI is built from shared blocks (see docs/UI.md, "Building blocks"): these tests fail when a screen
//! uses FLTK's raw widget for something a block already does, so every screen keeps one look.

use std::path::{Path, PathBuf};

/// Every UI source file, with its text.
fn sources() -> Vec<(PathBuf, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = vec![root.join("ui.rs")];
    if let Ok(dir) = std::fs::read_dir(root.join("ui")) {
        files.extend(
            dir.flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x == "rs")),
        );
    }
    files
        .into_iter()
        .filter_map(|p| Some((p.clone(), std::fs::read_to_string(&p).ok()?)))
        .collect()
}

fn name(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Lines (as `file:line`) holding `what`, outside the files allowed to, and outside this file.
fn uses(what: &str, allowed: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    for (p, text) in sources() {
        let file = name(&p);
        if file == "rules.rs" || allowed.contains(&file.as_str()) {
            continue;
        }
        for (i, line) in text.lines().enumerate() {
            let code = line.split("//").next().unwrap_or("");
            if code.contains(what) {
                out.push(format!("{file}:{}", i + 1));
            }
        }
    }
    out
}

/// Lines holding `what` with no `then` in the `within` lines that follow (where the block is applied).
fn unstyled(what: &str, then: &[&str], within: usize) -> Vec<String> {
    let mut out = Vec::new();
    for (p, text) in sources() {
        let file = name(&p);
        if file == "rules.rs" {
            continue;
        }
        let lines: Vec<&str> = text.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            if !line.split("//").next().unwrap_or("").contains(what) {
                continue;
            }
            let next = &lines[i..(i + within + 1).min(lines.len())];
            if !next.iter().any(|l| then.iter().any(|t| l.contains(t))) {
                out.push(format!("{file}:{}", i + 1));
            }
        }
    }
    out
}

#[test]
fn menus_are_ours() {
    // FLTK's popups are square and cramped: every menu is a `menu::Menu`.
    for raw in [".popup()", ".pulldown(", "MenuButton::new(", "menu_at_pointer"] {
        assert_eq!(
            uses(raw, &["menu.rs"]),
            Vec::<String>::new(),
            "{raw}: use ui::menu::Menu"
        );
    }
    // A dropdown opens our menu: a Choice comes from `fit::choice()`, the others get `menu::dropdown`.
    let plain_choices: Vec<String> = uses("Choice::default()", &["fit.rs"])
        .into_iter()
        .filter(|at| !uses("InputChoice::default()", &[]).contains(at))
        .collect();
    assert_eq!(plain_choices, Vec::<String>::new(), "a Choice comes from fit::choice()");
    assert_eq!(
        unstyled("MenuButton::default()", &["menu::dropdown("], 3),
        Vec::<String>::new(),
        "a MenuButton opens ui::menu: call menu::dropdown on it"
    );
    assert_eq!(
        unstyled("InputChoice::default()", &["menu::input_choice("], 2),
        Vec::<String>::new(),
        "an InputChoice opens ui::menu: call menu::input_choice on it"
    );
}

#[test]
fn scrolling_is_ours() {
    assert_eq!(
        uses("Scroll::new(", &["scroll.rs"]),
        Vec::<String>::new(),
        "a scroll area is scroll::page, scroll::bare or scroll::area"
    );
    // FLTK's bars have arrows and grips: a view with bars paints them as ours.
    for raw in [
        "TextDisplay::default()",
        "TextEditor::default()",
        "Tree::default()",
        "CheckBrowser::default()",
        "HoldBrowser::default()",
        "Browser::default()",
    ] {
        assert_eq!(
            unstyled(raw, &["scroll::view(", "scroll::paint_bars("], 12),
            Vec::<String>::new(),
            "{raw}: call scroll::view on it, or scroll::paint_bars at the end of its draw"
        );
    }
}

#[test]
fn overlays_are_popovers() {
    // A window inside the window (a menu, a card) is a Popover: rounded, closed by a click outside.
    assert_eq!(
        uses("override()", &[]),
        Vec::<String>::new(),
        "a floating panel is a popover::Popover"
    );
}
