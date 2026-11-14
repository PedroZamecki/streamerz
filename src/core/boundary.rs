//! Guard the dependency direction while presentation adapters are replaced.

#[test]
fn backend_modules_do_not_import_a_frontend_toolkit() {
    fn check(folder: &std::path::Path) {
        for entry in std::fs::read_dir(folder).unwrap().flatten() {
            let path = entry.path();
            if path
                .file_name()
                .is_some_and(|name| name == "ui" || name == "ui.rs" || name == "main.rs" || name == "boundary.rs")
            {
                continue;
            }
            if path.is_dir() {
                check(&path);
                continue;
            }
            if path.extension().is_none_or(|extension| extension != "rs") {
                continue;
            }
            let source = std::fs::read_to_string(&path).unwrap();
            for forbidden in ["gpui::", "gpui_kit::", "streamerz_frontend::"] {
                assert!(!source.contains(forbidden), "{} imports {forbidden}", path.display());
            }
        }
    }
    check(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"));
}
