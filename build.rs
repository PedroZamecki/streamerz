use std::fmt::Write;

// Puts assets/brand/streamerz.ico into the Windows exe (Explorer, taskbar); no-op on other targets.
// And lists the languages: every assets/locales/<code>.json is built in, so a new language is just a file.
fn main() {
    embed_resource::compile("streamerz.rc", embed_resource::NONE)
        .manifest_optional()
        .unwrap();

    println!("cargo:rerun-if-changed=assets/locales");
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/locales");
    let mut codes: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .filter_map(|e| e.file_name().to_str()?.strip_suffix(".json").map(str::to_string))
        .collect();
    // en_US first: it is the default and the fallback.
    codes.sort_by_key(|c| (c != "en_US", c.clone()));
    let mut out = String::from("const LOCALES: &[(&str, &str)] = &[\n");
    for c in codes {
        writeln!(
            out,
            "    ({c:?}, include_str!({:?})),",
            dir.join(format!("{c}.json")).display()
        )
        .unwrap();
    }
    out.push_str("];\n");
    std::fs::write(
        std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("locales.rs"),
        out,
    )
    .unwrap();
    docs();
}

// The reference and the guides (docs/, and the README, CHAT and PLUGINS guides), built in for the MCP
// server (src/docs.rs): `DOCS`, (path under docs/ or the file's name, text).
fn docs() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    println!("cargo:rerun-if-changed=docs");
    let mut files: Vec<(String, std::path::PathBuf)> = Vec::new();
    let mut dirs = vec![root.join("docs")];
    while let Some(d) = dirs.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                dirs.push(p);
            } else if p.extension().is_some_and(|x| x == "md") {
                let rel = p.strip_prefix(root.join("docs")).unwrap();
                files.push((rel.to_string_lossy().replace('\\', "/"), p));
            }
        }
    }
    for name in ["README.md", "CHAT.md", "PLUGINS.md"] {
        println!("cargo:rerun-if-changed={name}");
        files.push((name.into(), root.join(name)));
    }
    files.sort();
    let mut out = String::from("pub const DOCS: &[(&str, &str)] = &[\n");
    for (rel, p) in files {
        writeln!(out, "    ({rel:?}, include_str!({:?})),", p.display()).unwrap();
    }
    out.push_str("];\n");
    std::fs::write(
        std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("docs.rs"),
        out,
    )
    .unwrap();
}
