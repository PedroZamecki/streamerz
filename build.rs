use std::fmt::Write;

// Puts assets/brand/streamerz.ico into the Windows exe (Explorer, taskbar); no-op on other targets.
// And lists the languages: every assets/locales/<code>.json is built in, so a new language is just a file.
fn main() {
    dev_version();
    windows_resources();

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

// Windows file metadata uses the same release version as the app and GitHub artifact.
fn windows_resources() {
    println!("cargo:rerun-if-env-changed=STREAMERZ_VERSION");
    println!("cargo:rerun-if-changed=streamerz.rc");
    println!("cargo:rerun-if-changed=assets/brand/streamerz.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let version = std::env::var("STREAMERZ_VERSION").unwrap_or_default();
    let parts: Vec<u16> = version
        .strip_prefix('v')
        .unwrap_or(&version)
        .split('.')
        .map(str::parse)
        .collect::<Result<_, _>>()
        .unwrap_or_default();
    let parts = if parts.len() == 3 {
        parts
    } else if version.is_empty() || version == "dev" {
        vec![0, 0, 0]
    } else {
        panic!("invalid Windows release version: {version}");
    };
    let [major, minor, patch] = parts.as_slice() else {
        unreachable!()
    };
    let numeric = format!("{major},{minor},{patch},0");
    let shown = format!("{major}.{minor}.{patch}.0");
    let icon = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("assets/brand/streamerz.ico")
        .to_string_lossy()
        .replace('\\', "/");
    let icon_resource = include_str!("streamerz.rc").replace("assets/brand/streamerz.ico", &icon);
    let rc = format!(
        r#"{icon_resource}
1 VERSIONINFO
 FILEVERSION {numeric}
 PRODUCTVERSION {numeric}
 FILEFLAGSMASK 0x3fL
 FILEFLAGS 0x0L
 FILEOS 0x40004L
 FILETYPE 0x1L
 FILESUBTYPE 0x0L
BEGIN
 BLOCK "StringFileInfo"
 BEGIN
  BLOCK "040904b0"
  BEGIN
   VALUE "FileDescription", "Streamerz\0"
   VALUE "FileVersion", "{shown}\0"
   VALUE "InternalName", "Streamerz\0"
   VALUE "OriginalFilename", "Streamerz.exe\0"
   VALUE "ProductName", "Streamerz\0"
   VALUE "ProductVersion", "{shown}\0"
  END
 END
 BLOCK "VarFileInfo"
 BEGIN
  VALUE "Translation", 0x409, 1200
 END
END
"#
    );
    let path = std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("streamerz-version.rc");
    std::fs::write(&path, rc).unwrap();
    embed_resource::compile(path, embed_resource::NONE)
        .manifest_optional()
        .unwrap();
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
    let mut entries = Vec::new();
    for (file_index, (rel, p)) in files.into_iter().enumerate() {
        if rel.starts_with("reference/") && !rel.ends_with("README.md") {
            let text = std::fs::read_to_string(&p).unwrap();
            let starts: Vec<usize> = text.match_indices("\n## ").map(|(at, _)| at + 1).collect();
            for (i, start) in starts.iter().copied().enumerate() {
                let end = starts.get(i + 1).copied().unwrap_or(text.len());
                let section = &text[start..end];
                let field = |key: &str| {
                    section
                        .lines()
                        .take_while(|l| !l.starts_with("### "))
                        .find_map(|l| l.strip_prefix(key))
                };
                let kind = match field("- Kind: ") {
                    Some("trigger") => "trigger",
                    Some("sub-action") => "subaction",
                    _ => continue,
                };
                let code: u32 = field("- Code: ").unwrap().trim_matches('`').parse().unwrap();
                entries.push((kind, code, file_index, start, end));
            }
        }
        writeln!(out, "    ({rel:?}, include_str!({:?})),", p.display()).unwrap();
    }
    out.push_str("];\n");
    entries.sort_unstable_by_key(|e| (e.0, e.1));
    out.push_str("pub const REFERENCE: &[(&str, u32, usize, usize, usize)] = &[\n");
    for (kind, code, file_index, start, end) in entries {
        writeln!(out, "    ({kind:?}, {code}, {file_index}, {start}, {end}),").unwrap();
    }
    out.push_str("];\n");
    std::fs::write(
        std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("docs.rs"),
        out,
    )
    .unwrap();
}

// Identify local builds without changing the version supplied by release CI.
fn dev_version() {
    use std::process::Command;
    for name in ["HEAD", "index"] {
        if let Ok(path) = Command::new("git").args(["rev-parse", "--git-path", name]).output() {
            if path.status.success() {
                println!(
                    "cargo:rerun-if-changed={}",
                    String::from_utf8_lossy(&path.stdout).trim()
                );
            }
        }
    }
    if let Ok(reference) = Command::new("git").args(["symbolic-ref", "-q", "HEAD"]).output() {
        if reference.status.success() {
            let name = String::from_utf8_lossy(&reference.stdout);
            if let Ok(path) = Command::new("git")
                .args(["rev-parse", "--git-path", name.trim()])
                .output()
            {
                if path.status.success() {
                    println!(
                        "cargo:rerun-if-changed={}",
                        String::from_utf8_lossy(&path.stdout).trim()
                    );
                }
            }
        }
    }
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=assets");
    let output = Command::new("git").args(["rev-parse", "--short=12", "HEAD"]).output();
    let hash = output
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".into());
    let dirty = Command::new("git")
        .args(["status", "--porcelain", "--untracked-files=normal"])
        .output()
        .ok()
        .is_some_and(|o| o.status.success() && !o.stdout.is_empty());
    println!(
        "cargo:rustc-env=STREAMERZ_DEV_VERSION={}{}",
        hash,
        if dirty { "-dirty" } else { "" }
    );
}
