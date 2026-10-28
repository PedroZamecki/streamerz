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
}
