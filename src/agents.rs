//! The AI agents on this computer (Claude Code, Codex, OpenCode, Gemini CLI, Antigravity, Cursor...):
//! which are installed, whether Streamerz's MCP server (`Streamerz --mcp`, mcp.rs) is in each one's own
//! config, adding or taking it out there, and opening a CLI agent in a terminal to chat with it. Only
//! files are read and written (no agent is run to find out); nothing runs until the page asks.

use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// How an agent keeps its MCP servers.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Format {
    /// `{"<key>": {"streamerz": {command, args, env}}}`, plus what `extra` adds to the entry.
    Json(&'static str, Extra),
    /// Codex's `config.toml`: `[mcp_servers.streamerz]`.
    Toml,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Extra {
    None,
    /// `"type": "stdio"` (Claude Code, VS Code, Crush).
    Stdio,
    /// OpenCode: `{"type": "local", "command": [exe, args...], "environment": {}, "enabled": true}`.
    Opencode,
    /// GitHub Copilot CLI: `"type": "local"` and every tool.
    Copilot,
}

/// Where a config lives, from these folders.
pub struct Places {
    pub home: PathBuf,
    /// `~/.config` (or XDG_CONFIG_HOME); `%APPDATA%` on Windows; `~/Library/Application Support` on macOS.
    pub config: PathBuf,
    /// `%LOCALAPPDATA%` on Windows, `config` elsewhere.
    pub local: PathBuf,
    /// Where commands are looked for: PATH, then where installers put them when the desktop's PATH
    /// doesn't have them.
    pub path: Vec<PathBuf>,
    pub codex_home: Option<PathBuf>,
    pub claude_dir: Option<PathBuf>,
}

impl Places {
    pub fn here() -> Places {
        let var = |k: &str| std::env::var_os(k).filter(|s| !s.is_empty()).map(PathBuf::from);
        let home = var("HOME").or_else(|| var("USERPROFILE")).unwrap_or_default();
        #[cfg(windows)]
        let (config, local) = (
            var("APPDATA").unwrap_or_else(|| home.join("AppData").join("Roaming")),
            var("LOCALAPPDATA").unwrap_or_else(|| home.join("AppData").join("Local")),
        );
        #[cfg(target_os = "macos")]
        let (config, local) = {
            let c = home.join("Library").join("Application Support");
            (c.clone(), c)
        };
        #[cfg(not(any(windows, target_os = "macos")))]
        let (config, local) = {
            let c = var("XDG_CONFIG_HOME").unwrap_or_else(|| home.join(".config"));
            (c.clone(), c)
        };
        let mut path: Vec<PathBuf> = std::env::var_os("PATH")
            .map(|p| std::env::split_paths(&p).collect())
            .unwrap_or_default();
        for d in [
            ".local/bin",
            ".local/share/mise/shims",
            ".npm-global/bin",
            ".bun/bin",
            ".opencode/bin",
            ".claude/local",
            ".cargo/bin",
            ".volta/bin",
            "go/bin",
        ] {
            path.push(home.join(d));
        }
        #[cfg(windows)]
        {
            path.push(config.join("npm"));
            path.push(local.join("npm"));
            path.push(home.join(".opencode").join("bin"));
            path.push(home.join("scoop").join("shims"));
            path.push(home.join("scoop").join("apps").join("opencode").join("current"));
            path.push(local.join("Programs").join("opencode-desktop"));
            path.push(
                local
                    .join("Programs")
                    .join("opencode-desktop")
                    .join("resources")
                    .join("bin"),
            );
            path.push(local.join("Programs").join("OpenCode"));
            path.push(local.join("Programs").join("opencode"));
            path.push(local.join("opencode"));
            path.push(PathBuf::from(r"C:\Program Files\OpenCode"));
            path.push(PathBuf::from(r"C:\Program Files\opencode"));
            path.push(PathBuf::from(r"C:\ProgramData\chocolatey\bin"));
            if let Some(p) = std::env::var_os("OPENCODE_INSTALL_DIR").map(PathBuf::from) {
                path.push(p.join("bin"));
                path.push(p);
            }
            if let Some(p) = std::env::var_os("OPENCODE_BIN").map(PathBuf::from) {
                if let Some(d) = p.parent() {
                    path.push(d.to_path_buf());
                }
            }
        }
        #[cfg(not(windows))]
        path.extend(["/usr/local/bin", "/opt/homebrew/bin"].map(PathBuf::from));
        Places {
            config,
            local,
            path,
            codex_home: var("CODEX_HOME"),
            claude_dir: var("CLAUDE_CONFIG_DIR"),
            home,
        }
    }

    /// The command `name` where it's installed.
    pub fn find(&self, name: &str) -> Option<PathBuf> {
        #[cfg(windows)]
        let names: Vec<String> = ["exe", "cmd", "bat", "ps1"]
            .iter()
            .flat_map(|x| {
                let mut v = vec![format!("{name}.{x}")];
                if name == "opencode" {
                    v.push(format!("OpenCode.{x}"));
                    v.push(format!("opencode-cli.{x}"));
                } else if name == "zed" {
                    v.push(format!("zedit.{x}"));
                } else if name == "kiro-cli" {
                    v.push(format!("kiro.{x}"));
                }
                v
            })
            .collect();
        #[cfg(not(windows))]
        let names = if name == "opencode" {
            vec![
                "opencode".to_string(),
                "opencode-cli".to_string(),
                "OpenCode".to_string(),
            ]
        } else if name == "zed" {
            vec!["zed".to_string(), "zedit".to_string()]
        } else if name == "kiro-cli" {
            vec!["kiro-cli".to_string(), "kiro".to_string()]
        } else {
            vec![name.to_string()]
        };
        self.path
            .iter()
            .flat_map(|d| names.iter().map(move |n| d.join(n)))
            .find(|p| p.is_file())
    }
}

/// One agent Streamerz knows.
pub struct Kind {
    pub id: &'static str,
    pub name: &'static str,
    /// Its command, for chatting in a terminal (and finding it).
    pub cli: Option<&'static str>,
    format: Format,
    /// Its config file.
    file: fn(&Places) -> PathBuf,
    /// A folder whose presence says the app is installed (for apps with no command).
    app: Option<fn(&Places) -> PathBuf>,
}

const MCP: Format = Format::Json("mcpServers", Extra::None);

fn opencode_file(p: &Places) -> PathBuf {
    if let Some(f) = std::env::var_os("OPENCODE_CONFIG").map(PathBuf::from) {
        if f.is_file() && (p.home != Path::new("/h") || f.starts_with("/h")) {
            return f;
        }
    }
    if let Some(dir) = std::env::var_os("OPENCODE_CONFIG_DIR").map(PathBuf::from) {
        if p.home != Path::new("/h") || dir.starts_with("/h") {
            let jsonc = dir.join("opencode.jsonc");
            if jsonc.is_file() {
                return jsonc;
            }
            let json = dir.join("opencode.json");
            if json.is_file() {
                return json;
            }
        }
    }
    let default_dir = p.home.join(".config").join("opencode");
    let default_json = default_dir.join("opencode.json");
    let default_jsonc = default_dir.join("opencode.jsonc");
    if default_json.is_file() {
        return default_json;
    }
    if default_jsonc.is_file() {
        return default_jsonc;
    }
    #[cfg(windows)]
    {
        let win_json = p.config.join("opencode").join("opencode.json");
        let win_jsonc = p.config.join("opencode").join("opencode.jsonc");
        if win_json.is_file() {
            return win_json;
        }
        if win_jsonc.is_file() {
            return win_jsonc;
        }
        let local_json = p.local.join("opencode").join("opencode.json");
        let local_jsonc = p.local.join("opencode").join("opencode.jsonc");
        if local_json.is_file() {
            return local_json;
        }
        if local_jsonc.is_file() {
            return local_jsonc;
        }
    }
    default_json
}

fn opencode_app(p: &Places) -> PathBuf {
    for d in [
        p.home.join(".opencode"),
        p.home.join(".config").join("opencode"),
        p.config.join("opencode"),
        p.local.join("opencode"),
        p.local.join("Programs").join("opencode-desktop"),
        p.local.join("Programs").join("OpenCode"),
        p.local.join("Programs").join("opencode"),
        p.config.join("ai.opencode.desktop"),
    ] {
        if d.is_dir() {
            return d;
        }
    }
    #[cfg(windows)]
    for d in [
        PathBuf::from(r"C:\Program Files\OpenCode"),
        PathBuf::from(r"C:\Program Files\opencode"),
    ] {
        if d.is_dir() {
            return d;
        }
    }
    p.home.join(".opencode")
}

fn cline_file(p: &Places) -> PathBuf {
    p.config
        .join("Code")
        .join("User")
        .join("globalStorage")
        .join("saoudrizwan.claude-dev")
        .join("settings")
        .join("cline_mcp_settings.json")
}

fn cline_app(p: &Places) -> PathBuf {
    p.config
        .join("Code")
        .join("User")
        .join("globalStorage")
        .join("saoudrizwan.claude-dev")
}

fn zed_file(p: &Places) -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        p.home
            .join("Library")
            .join("Application Support")
            .join("Zed")
            .join("settings.json")
    }
    #[cfg(windows)]
    {
        p.config.join("Zed").join("settings.json")
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        p.config.join("zed").join("settings.json")
    }
}

fn zed_app(p: &Places) -> PathBuf {
    if p.config.join("zed").is_dir() {
        return p.config.join("zed");
    }
    #[cfg(windows)]
    if p.config.join("Zed").is_dir() {
        return p.config.join("Zed");
    }
    p.config.join("zed")
}

fn roo_file(p: &Places) -> PathBuf {
    p.config
        .join("Code")
        .join("User")
        .join("globalStorage")
        .join("rooveterinaryinc.roo-cline")
        .join("settings")
        .join("cline_mcp_settings.json")
}

fn roo_app(p: &Places) -> PathBuf {
    p.config
        .join("Code")
        .join("User")
        .join("globalStorage")
        .join("rooveterinaryinc.roo-cline")
}

fn auggie_app(p: &Places) -> PathBuf {
    if p.home.join(".auggie").is_dir() {
        return p.home.join(".auggie");
    }
    p.home.join(".augment")
}

fn crush_app(p: &Places) -> PathBuf {
    if p.home.join(".config").join("crush").is_dir() {
        return p.home.join(".config").join("crush");
    }
    p.local.join("crush")
}

pub const KINDS: &[Kind] = &[
    Kind {
        id: "claude",
        name: "Claude Code",
        cli: Some("claude"),
        format: Format::Json("mcpServers", Extra::Stdio),
        file: |p| p.claude_dir.as_ref().unwrap_or(&p.home).join(".claude.json"),
        app: None,
    },
    Kind {
        id: "codex",
        name: "Codex",
        cli: Some("codex"),
        format: Format::Toml,
        file: |p| {
            p.codex_home
                .clone()
                .unwrap_or_else(|| p.home.join(".codex"))
                .join("config.toml")
        },
        app: None,
    },
    Kind {
        id: "opencode",
        name: "OpenCode",
        cli: Some("opencode"),
        format: Format::Json("mcp", Extra::Opencode),
        file: opencode_file,
        app: Some(opencode_app),
    },
    Kind {
        id: "gemini",
        name: "Gemini CLI",
        cli: Some("gemini"),
        format: MCP,
        file: |p| p.home.join(".gemini").join("settings.json"),
        app: None,
    },
    Kind {
        id: "antigravity",
        name: "Antigravity",
        cli: None,
        format: MCP,
        file: |p| p.home.join(".gemini").join("antigravity").join("mcp_config.json"),
        app: Some(|p| p.home.join(".gemini").join("antigravity")),
    },
    Kind {
        id: "cursor",
        name: "Cursor",
        cli: Some("cursor-agent"),
        format: MCP,
        file: |p| p.home.join(".cursor").join("mcp.json"),
        app: Some(|p| p.home.join(".cursor")),
    },
    Kind {
        id: "windsurf",
        name: "Windsurf",
        cli: None,
        format: MCP,
        file: |p| p.home.join(".codeium").join("windsurf").join("mcp_config.json"),
        app: Some(|p| p.home.join(".codeium").join("windsurf")),
    },
    Kind {
        id: "vscode",
        name: "VS Code (Copilot)",
        cli: None,
        format: Format::Json("servers", Extra::Stdio),
        file: |p| p.config.join("Code").join("User").join("mcp.json"),
        app: Some(|p| p.config.join("Code").join("User")),
    },
    Kind {
        id: "copilot",
        name: "GitHub Copilot CLI",
        cli: Some("copilot"),
        format: Format::Json("mcpServers", Extra::Copilot),
        file: |p| p.home.join(".copilot").join("mcp-config.json"),
        app: None,
    },
    Kind {
        id: "claude-desktop",
        name: "Claude Desktop",
        cli: None,
        format: MCP,
        file: |p| p.config.join("Claude").join("claude_desktop_config.json"),
        app: Some(|p| p.config.join("Claude")),
    },
    Kind {
        id: "qwen",
        name: "Qwen Code",
        cli: Some("qwen"),
        format: MCP,
        file: |p| p.home.join(".qwen").join("settings.json"),
        app: None,
    },
    Kind {
        id: "kiro",
        name: "Kiro",
        cli: Some("kiro-cli"),
        format: MCP,
        file: |p| p.home.join(".kiro").join("settings").join("mcp.json"),
        app: Some(|p| p.home.join(".kiro")),
    },
    Kind {
        id: "crush",
        name: "Crush",
        cli: Some("crush"),
        format: Format::Json("mcp", Extra::Stdio),
        file: |p| p.local.join("crush").join("crush.json"),
        app: Some(crush_app),
    },
    Kind {
        id: "amp",
        name: "Amp",
        cli: Some("amp"),
        format: Format::Json("amp.mcpServers", Extra::None),
        file: |p| p.home.join(".config").join("amp").join("settings.json"),
        app: None,
    },
    Kind {
        id: "lmstudio",
        name: "LM Studio",
        cli: None,
        format: MCP,
        file: |p| p.home.join(".lmstudio").join("mcp.json"),
        app: Some(|p| p.home.join(".lmstudio")),
    },
    Kind {
        id: "cline",
        name: "Cline",
        cli: Some("cline"),
        format: MCP,
        file: cline_file,
        app: Some(cline_app),
    },
    Kind {
        id: "zed",
        name: "Zed",
        cli: Some("zed"),
        format: Format::Json("context_servers", Extra::None),
        file: zed_file,
        app: Some(zed_app),
    },
    Kind {
        id: "roo-code",
        name: "Roo Code",
        cli: None,
        format: MCP,
        file: roo_file,
        app: Some(roo_app),
    },
    Kind {
        id: "devin",
        name: "Devin",
        cli: Some("devin"),
        format: MCP,
        file: |p| p.home.join(".devin").join("mcp.json"),
        app: Some(|p| p.home.join(".devin")),
    },
    Kind {
        id: "vibe",
        name: "Mistral Vibe",
        cli: Some("vibe"),
        format: MCP,
        file: |p| p.home.join(".vibe").join("mcp.json"),
        app: Some(|p| p.home.join(".vibe")),
    },
    Kind {
        id: "kimi",
        name: "Kimi Code",
        cli: Some("kimi"),
        format: MCP,
        file: |p| p.home.join(".kimi").join("mcp.json"),
        app: Some(|p| p.home.join(".kimi")),
    },
    Kind {
        id: "auggie",
        name: "Auggie",
        cli: Some("auggie"),
        format: MCP,
        file: |p| p.home.join(".augment").join("mcp.json"),
        app: Some(auggie_app),
    },
];

/// Streamerz's MCP server as an agent starts it.
#[derive(Clone, Debug)]
pub struct Server {
    /// Its name in the agent's config: `streamerz` (`streamerz-dev` for a development build).
    pub name: String,
    pub command: String,
    /// `STREAMERZ_URL` when the instance isn't where the config says (a development instance).
    pub url: Option<String>,
}

impl Server {
    /// This Streamerz, its WebSocket at `url` (written only for a development build: a release reads
    /// its config). The AppImage itself, not its mount, which moves each run.
    pub fn this(url: String) -> Server {
        let exe = crate::update::exe().unwrap_or_default();
        let dev = crate::brand::IS_DEV;
        Server {
            name: if dev { "streamerz-dev" } else { "streamerz" }.into(),
            command: exe.display().to_string(),
            url: dev.then_some(url),
        }
    }

    fn env(&self) -> Value {
        match &self.url {
            Some(u) => json!({ "STREAMERZ_URL": u }),
            None => json!({}),
        }
    }

    fn entry(&self, extra: Extra) -> Value {
        if extra == Extra::Opencode {
            return json!({"type": "local", "command": [self.command, "--mcp"], "environment": self.env(),
                "enabled": true});
        }
        let mut e = json!({"command": self.command, "args": ["--mcp"], "env": self.env()});
        match extra {
            Extra::Stdio => e["type"] = "stdio".into(),
            Extra::Copilot => {
                e["type"] = "local".into();
                e["tools"] = json!(["*"]);
            }
            _ => {}
        }
        e
    }

    /// What goes in a config by hand, for an agent not listed: the common `mcpServers` shape.
    pub fn snippet(&self) -> String {
        let v = json!({ "mcpServers": { &self.name: self.entry(Extra::None) } });
        serde_json::to_string_pretty(&v).unwrap_or_default()
    }

    fn toml(&self) -> String {
        let q = |s: &str| serde_json::to_string(s).unwrap_or_default();
        let mut t = format!(
            "[mcp_servers.{}]\ncommand = {}\nargs = [\"--mcp\"]\n",
            self.name,
            q(&self.command)
        );
        if let Some(u) = &self.url {
            t += &format!("env = {{ STREAMERZ_URL = {} }}\n", q(u));
        }
        t
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum State {
    /// Installed, without Streamerz.
    Ready,
    /// Streamerz is in its config, as this one starts it.
    Added,
    /// Streamerz is there, but another exe or address (moved, or another build).
    Outdated,
}

/// An agent found here.
pub struct Found {
    pub kind: &'static Kind,
    pub state: State,
    /// Its command, when it has one installed.
    pub cli: Option<PathBuf>,
    pub file: PathBuf,
}

/// The agents installed, in [`KINDS`]' order.
pub fn found(places: &Places, server: &Server) -> Vec<Found> {
    KINDS
        .iter()
        .filter_map(|kind| {
            let cli = kind.cli.and_then(|c| places.find(c));
            let file = (kind.file)(places);
            if cli.is_none() && !kind.app.is_some_and(|a| a(places).is_dir()) && !file.is_file() {
                return None;
            }
            let state = state(kind.format, &file, server);
            Some(Found { kind, state, cli, file })
        })
        .collect()
}

fn strip_jsonc_comments(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let n = chars.len();
    let mut i = 0;
    let mut in_str = false;
    let mut escape = false;

    while i < n {
        let c = chars[i];
        if in_str {
            out.push(c);
            if escape {
                escape = false;
            } else if c == '\\' {
                escape = true;
            } else if c == '"' {
                in_str = false;
            }
            i += 1;
            continue;
        }

        if c == '"' {
            in_str = true;
            out.push(c);
            i += 1;
        } else if c == '/' && i + 1 < n && chars[i + 1] == '/' {
            i += 2;
            while i < n && chars[i] != '\n' {
                i += 1;
            }
        } else if c == '/' && i + 1 < n && chars[i + 1] == '*' {
            i += 2;
            while i + 1 < n && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            i = (i + 2).min(n);
        } else if c == ',' {
            let mut j = i + 1;
            let mut is_trailing = false;
            while j < n {
                let next = chars[j];
                if next.is_whitespace() {
                    j += 1;
                } else if next == '/' && j + 1 < n && chars[j + 1] == '/' {
                    j += 2;
                    while j < n && chars[j] != '\n' {
                        j += 1;
                    }
                } else if next == '/' && j + 1 < n && chars[j + 1] == '*' {
                    j += 2;
                    while j + 1 < n && !(chars[j] == '*' && chars[j + 1] == '/') {
                        j += 1;
                    }
                    j = (j + 2).min(n);
                } else if next == '}' || next == ']' {
                    is_trailing = true;
                    break;
                } else {
                    break;
                }
            }
            if !is_trailing {
                out.push(',');
            }
            i += 1;
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

fn read_json(file: &Path) -> Result<Value, String> {
    match std::fs::read(file) {
        Ok(b) if b.iter().all(u8::is_ascii_whitespace) => Ok(json!({})),
        Ok(b) => {
            if let Ok(v) = serde_json::from_slice(&b) {
                return Ok(v);
            }
            let is_jsonc = file.extension().is_some_and(|e| e == "jsonc")
                || file.to_string_lossy().contains("opencode")
                || file.to_string_lossy().contains("zed");
            if is_jsonc {
                if let Ok(s) = std::str::from_utf8(&b) {
                    let cleaned = strip_jsonc_comments(s);
                    if let Ok(v) = serde_json::from_str(&cleaned) {
                        return Ok(v);
                    }
                }
            }
            serde_json::from_slice(&b).map_err(|e| format!("{}: {e}", file.display()))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(json!({})),
        Err(e) => Err(format!("{}: {e}", file.display())),
    }
}

fn state(format: Format, file: &Path, server: &Server) -> State {
    match format {
        Format::Json(key, extra) => {
            let v = read_json(file).unwrap_or_default();
            let entry = if extra == Extra::Opencode {
                v.get("mcp")
                    .and_then(|m| m.get("servers"))
                    .and_then(|s| s.get(&server.name))
                    .or_else(|| v.get("mcp").and_then(|m| m.get(&server.name)))
            } else {
                v.get(key).and_then(|m| m.get(&server.name))
            };
            match entry {
                None => State::Ready,
                // Other settings the user added (timeouts...) don't count.
                Some(e) if same(e, &server.entry(extra)) => State::Added,
                Some(_) => State::Outdated,
            }
        }
        Format::Toml => {
            let text = std::fs::read_to_string(file).unwrap_or_default();
            match toml_block(&text, &server.name) {
                None => State::Ready,
                Some((a, b)) if text[a..b].trim_end() == server.toml().trim_end() => State::Added,
                Some(_) => State::Outdated,
            }
        }
    }
}

/// The same command, arguments and environment (none written is an empty one).
fn same(have: &Value, want: &Value) -> bool {
    let empty = json!({});
    ["command", "args", "env", "environment"].iter().all(|k| {
        let (h, w) = (have.get(k), want.get(k));
        let env = k.starts_with("env");
        h == w || env && h.unwrap_or(&empty) == w.unwrap_or(&empty)
    })
}

/// Where `[mcp_servers.NAME]` (and its subtables) sits in a TOML text: start and end bytes.
fn toml_block(text: &str, name: &str) -> Option<(usize, usize)> {
    let head = format!("[mcp_servers.{name}]");
    let sub = format!("[mcp_servers.{name}.");
    let mut at = 0;
    let mut start = None;
    for line in text.split_inclusive('\n') {
        let t = line.trim();
        if t.starts_with('[') {
            match start {
                None if t == head => start = Some(at),
                Some(s) if !t.starts_with(&sub) => return Some((s, at)),
                _ => {}
            }
        }
        at += line.len();
    }
    start.map(|s| (s, text.len()))
}

/// Writes `text` to `file` through a file beside it, so the agent never reads half of it.
fn write(file: &Path, text: &str) -> Result<(), String> {
    if let Some(d) = file.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    let tmp = file.with_extension("streamerz-tmp");
    std::fs::write(&tmp, text)
        .and_then(|()| std::fs::rename(&tmp, file))
        .map_err(|e| format!("{}: {e}", file.display()))
}

/// Puts Streamerz in `kind`'s config (replacing an older entry), or takes it out. Everything else in
/// the file is kept as it was, in its order.
pub fn set(kind: &Kind, places: &Places, server: &Server, on: bool) -> Result<(), String> {
    let file = (kind.file)(places);
    match kind.format {
        Format::Json(key, extra) => {
            let mut v = read_json(&file)?;
            if !v.is_object() {
                return Err(format!("{}: not a JSON object", file.display()));
            }
            if extra == Extra::Opencode {
                if on {
                    if !v["mcp"].is_object() {
                        v["mcp"] = json!({});
                    }
                    if !v["mcp"]["servers"].is_object() {
                        v["mcp"]["servers"] = json!({});
                    }
                    v["mcp"]["servers"][&server.name] = server.entry(extra);
                    if let Some(m) = v.get_mut("mcp").and_then(Value::as_object_mut) {
                        m.shift_remove(&server.name);
                    }
                } else {
                    let mut removed = false;
                    if let Some(servers) = v
                        .get_mut("mcp")
                        .and_then(|m| m.get_mut("servers"))
                        .and_then(Value::as_object_mut)
                    {
                        if servers.shift_remove(&server.name).is_some() {
                            removed = true;
                        }
                    }
                    if let Some(m) = v.get_mut("mcp").and_then(Value::as_object_mut) {
                        if m.shift_remove(&server.name).is_some() {
                            removed = true;
                        }
                    }
                    if !removed {
                        return Ok(());
                    }
                }
            } else if on {
                if !v[key].is_object() {
                    v[key] = json!({});
                }
                v[key][&server.name] = server.entry(extra);
            } else if let Some(m) = v.get_mut(key).and_then(Value::as_object_mut) {
                if m.shift_remove(&server.name).is_none() {
                    return Ok(());
                }
            } else {
                return Ok(());
            }
            let text = serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?;
            write(&file, &(text + "\n"))
        }
        Format::Toml => {
            let mut text = match std::fs::read_to_string(&file) {
                Ok(t) => t,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
                Err(e) => return Err(format!("{}: {e}", file.display())),
            };
            let had = toml_block(&text, &server.name);
            if let Some((a, b)) = had {
                text.replace_range(a..b, "");
                // It was last: no blank line left at the end.
                if a == text.len() {
                    let keep = text.trim_end().len();
                    text.truncate(keep);
                    if keep > 0 {
                        text.push('\n');
                    }
                }
            } else if !on {
                return Ok(());
            }
            if on {
                let mut block = server.toml();
                match had {
                    // Where it was, before the next table.
                    Some((a, _)) if a < text.len() => {
                        block.push('\n');
                        text.insert_str(a, &block);
                    }
                    _ => {
                        if !text.is_empty() && !text.ends_with('\n') {
                            text.push('\n');
                        }
                        if !text.is_empty() && !text.ends_with("\n\n") {
                            text.push('\n');
                        }
                        text += &block;
                    }
                }
            }
            write(&file, &text)
        }
    }
}

/// The folder a chat opens in: notes telling the agent what it's there for (each agent reads its own
/// file name).
pub fn workspace(server: &Server) -> Result<PathBuf, String> {
    let dir = crate::config_path().with_file_name("assistant");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let notes = format!(
        "# Streamerz\n\nYou are helping a streamer set up Streamerz, a Streamer.bot compatible app for \
         Twitch, YouTube, Kick and TikTok. Use the `{}` MCP server's tools to look at and change it: \
         start with `guide` and `status`; find types with `list_triggers`, `list_subactions` and \
         `docs_search`; read `describe` and `docs` before writing one; build with `new_action` and \
         `save`. Changes apply at once, so say what you will change and ask before deleting \
         anything. Answer in the language the user writes in.\n",
        server.name
    );
    for f in [
        "AGENTS.md",
        "CLAUDE.md",
        "GEMINI.md",
        "QWEN.md",
        "OPENCODE.md",
        "ZED.md",
        "CLINE.md",
    ] {
        let p = dir.join(f);
        if std::fs::read_to_string(&p).ok().as_deref() != Some(notes.as_str()) {
            std::fs::write(&p, &notes).map_err(|e| e.to_string())?;
        }
    }
    Ok(dir)
}

/// Opens `cli` in a terminal window, in `dir`.
pub fn chat(cli: &Path, dir: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // `start` opens the user's terminal; cmd runs .cmd shims (npm's) as well as exes.
        std::process::Command::new("cmd")
            .args(["/C", "start", "Streamerz", "/D"])
            .arg(dir)
            .args(["cmd", "/K"])
            .arg(cli)
            .creation_flags(0x0800_0000)
            .spawn()
            .map(drop)
            .map_err(|e| e.to_string())
    }
    #[cfg(not(windows))]
    {
        let places = Places::here();
        let mut tried = Vec::new();
        let wanted = std::env::var("TERMINAL").ok().filter(|t| !t.is_empty());
        for t in wanted.iter().map(String::as_str).chain(TERMINALS.iter().copied()) {
            let Some(exe) = (if t.contains('/') {
                Some(PathBuf::from(t))
            } else {
                places.find(t)
            }) else {
                continue;
            };
            let base = exe.file_name().and_then(|n| n.to_str()).unwrap_or(t).to_string();
            let mut cmd = std::process::Command::new(&exe);
            cmd.args(terminal_args(&base, dir)).arg(cli);
            cmd.current_dir(dir)
                .env_remove("GDK_BACKEND")
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null());
            match cmd.spawn() {
                Ok(_) => return Ok(()),
                Err(e) => tried.push(format!("{base}: {e}")),
            }
        }
        Err(if tried.is_empty() {
            crate::i18n::tr("agents.no_terminal").to_string()
        } else {
            tried.join(", ")
        })
    }
}

/// Terminals tried, in order, after `$TERMINAL`.
#[cfg(not(windows))]
const TERMINALS: &[&str] = &[
    "xdg-terminal-exec",
    "x-terminal-emulator",
    "kitty",
    "ghostty",
    "alacritty",
    "foot",
    "wezterm",
    "konsole",
    "gnome-terminal",
    "ptyxis",
    "kgx",
    "xfce4-terminal",
    "tilix",
    "terminator",
    "xterm",
];

/// What goes before the command for terminal `name` (its file name) to run it in `dir`.
#[cfg(not(windows))]
fn terminal_args(name: &str, dir: &Path) -> Vec<std::ffi::OsString> {
    let d = dir.as_os_str().to_owned();
    let s = |x: &str| std::ffi::OsString::from(x);
    match name {
        "xdg-terminal-exec" | "kitty" | "foot" => vec![],
        "wezterm" => vec![s("start"), s("--cwd"), d, s("--")],
        "gnome-terminal" | "ptyxis" | "kgx" => {
            let mut w = s("--working-directory=");
            w.push(&d);
            vec![w, s("--")]
        }
        "konsole" => vec![s("--workdir"), d, s("-e")],
        "xfce4-terminal" => vec![s("--working-directory"), d, s("-x")],
        "terminator" => vec![s("--working-directory"), d, s("-x")],
        _ => vec![s("-e")],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn places(dir: &Path) -> Places {
        Places {
            home: dir.into(),
            config: dir.join("cfg"),
            local: dir.join("cfg"),
            path: vec![dir.join("bin")],
            codex_home: None,
            claude_dir: None,
        }
    }

    fn server(url: Option<&str>) -> Server {
        Server {
            name: "streamerz".into(),
            command: "/opt/Streamerz \"x\"".into(),
            url: url.map(Into::into),
        }
    }

    fn kind(id: &str) -> &'static Kind {
        KINDS.iter().find(|k| k.id == id).unwrap()
    }

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("streamerz-agents-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn json_configs_keep_everything_else_in_order() {
        let d = tmp("json");
        let p = places(&d);
        let f = d.join(".claude.json");
        std::fs::write(&f, r#"{"z": 1, "mcpServers": {"other": {"command": "o"}}, "a": [2]}"#).unwrap();
        let s = server(None);
        assert_eq!(state(kind("claude").format, &f, &s), State::Ready);
        set(kind("claude"), &p, &s, true).unwrap();
        let v: Value = serde_json::from_slice(&std::fs::read(&f).unwrap()).unwrap();
        assert_eq!(
            v.as_object().unwrap().keys().collect::<Vec<_>>(),
            ["z", "mcpServers", "a"]
        );
        assert_eq!(v["mcpServers"]["other"]["command"], "o");
        assert_eq!(v["mcpServers"]["streamerz"]["type"], "stdio");
        assert_eq!(v["mcpServers"]["streamerz"]["args"], json!(["--mcp"]));
        assert_eq!(state(kind("claude").format, &f, &s), State::Added);
        // Another build, or the exe moved.
        assert_eq!(
            state(kind("claude").format, &f, &server(Some("ws://127.0.0.1:8090/"))),
            State::Outdated
        );
        set(kind("claude"), &p, &s, false).unwrap();
        let v: Value = serde_json::from_slice(&std::fs::read(&f).unwrap()).unwrap();
        assert!(v["mcpServers"].get("streamerz").is_none());
        assert_eq!(v["mcpServers"]["other"]["command"], "o");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_file_that_isnt_json_is_left_alone() {
        let d = tmp("jsonc");
        let p = places(&d);
        let f = d.join(".gemini").join("settings.json");
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(&f, "{ // a comment\n}").unwrap();
        assert!(set(kind("gemini"), &p, &server(None), true).is_err());
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "{ // a comment\n}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn opencode_and_copilot_get_their_own_shapes() {
        let s = server(Some("ws://127.0.0.1:8090/"));
        let o = s.entry(Extra::Opencode);
        assert_eq!(o["command"], json!([s.command, "--mcp"]));
        assert_eq!(o["environment"]["STREAMERZ_URL"], "ws://127.0.0.1:8090/");
        assert_eq!(s.entry(Extra::Copilot)["tools"], json!(["*"]));
    }

    #[test]
    fn codex_toml_is_edited_in_place() {
        let d = tmp("toml");
        let p = places(&d);
        let f = d.join(".codex").join("config.toml");
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        let before = "model = \"x\"\n\n[mcp_servers.streamerz]\ncommand = \"old\"\n\n[mcp_servers.streamerz.env]\nA = \"1\"\n\n[mcp_servers.other]\ncommand = \"o\"\n";
        std::fs::write(&f, before).unwrap();
        let s = server(Some("ws://127.0.0.1:8090/"));
        assert_eq!(state(Format::Toml, &f, &s), State::Outdated);
        set(kind("codex"), &p, &s, true).unwrap();
        let t = std::fs::read_to_string(&f).unwrap();
        assert_eq!(
            t,
            "model = \"x\"\n\n[mcp_servers.streamerz]\ncommand = \"/opt/Streamerz \\\"x\\\"\"\nargs = [\"--mcp\"]\nenv = { STREAMERZ_URL = \"ws://127.0.0.1:8090/\" }\n\n[mcp_servers.other]\ncommand = \"o\"\n"
        );
        assert_eq!(state(Format::Toml, &f, &s), State::Added);
        set(kind("codex"), &p, &s, false).unwrap();
        assert_eq!(
            std::fs::read_to_string(&f).unwrap(),
            "model = \"x\"\n\n[mcp_servers.other]\ncommand = \"o\"\n"
        );
        // Added at the end of a file without it, after a blank line.
        set(kind("codex"), &p, &s, true).unwrap();
        let t = std::fs::read_to_string(&f).unwrap();
        assert!(t.starts_with("model = \"x\"\n\n[mcp_servers.other]\ncommand = \"o\"\n\n[mcp_servers.streamerz]\n"));
        set(kind("codex"), &p, &s, false).unwrap();
        assert_eq!(
            std::fs::read_to_string(&f).unwrap(),
            "model = \"x\"\n\n[mcp_servers.other]\ncommand = \"o\"\n"
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn finds_agents_by_command_or_app_folder() {
        let d = tmp("found");
        let p = places(&d);
        let s = server(None);
        assert!(found(&p, &s).is_empty());
        std::fs::create_dir_all(d.join("bin")).unwrap();
        #[cfg(windows)]
        std::fs::write(d.join("bin").join("codex.exe"), "").unwrap();
        #[cfg(not(windows))]
        std::fs::write(d.join("bin").join("codex"), "").unwrap();
        std::fs::create_dir_all(d.join(".cursor")).unwrap();
        let f = found(&p, &s);
        let ids: Vec<&str> = f.iter().map(|f| f.kind.id).collect();
        assert_eq!(ids, ["codex", "cursor"]);
        assert!(f[0].cli.is_some());
        assert_eq!(f[1].state, State::Ready);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn every_kind_has_a_unique_id_and_a_config_file() {
        let p = places(Path::new("/h"));
        let mut ids: Vec<&str> = KINDS.iter().map(|k| k.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), KINDS.len());
        assert!(KINDS.iter().all(|k| (k.file)(&p).starts_with("/h")));
    }

    #[test]
    fn opencode_v2_nested_servers_format_is_detected_and_managed() {
        let d = tmp("opencode_v2");
        let p = places(&d);
        let s = server(Some("ws://127.0.0.1:8090/"));
        let f = d.join(".config").join("opencode").join("opencode.json");
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();

        // Initially ready
        assert_eq!(state(kind("opencode").format, &f, &s), State::Ready);

        // Set writes OpenCode v2 structure: mcp -> servers -> streamerz
        set(kind("opencode"), &p, &s, true).unwrap();
        let v: Value = serde_json::from_slice(&std::fs::read(&f).unwrap()).unwrap();
        assert!(v["mcp"]["servers"].get("streamerz").is_some());
        assert_eq!(state(kind("opencode").format, &f, &s), State::Added);

        // Removing clears it from servers
        set(kind("opencode"), &p, &s, false).unwrap();
        let v_after: Value = serde_json::from_slice(&std::fs::read(&f).unwrap()).unwrap();
        assert!(v_after["mcp"]["servers"].get("streamerz").is_none());
        assert_eq!(state(kind("opencode").format, &f, &s), State::Ready);

        // Also detects legacy v1 mcp -> streamerz format as Added
        std::fs::write(
            &f,
            json!({ "mcp": { "streamerz": s.entry(Extra::Opencode) } }).to_string(),
        )
        .unwrap();
        assert_eq!(state(kind("opencode").format, &f, &s), State::Added);

        // Setting migrates legacy v1 to v2 servers table
        set(kind("opencode"), &p, &s, true).unwrap();
        let v_migrated: Value = serde_json::from_slice(&std::fs::read(&f).unwrap()).unwrap();
        assert!(v_migrated["mcp"]["servers"].get("streamerz").is_some());
        assert!(v_migrated["mcp"].get("streamerz").is_none());

        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn opencode_v2_jsonc_with_comments_is_supported() {
        let d = tmp("opencode_jsonc");
        let p = places(&d);
        let s = server(None);
        let f = d.join(".config").join("opencode").join("opencode.jsonc");
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();

        let jsonc = r#"{
            // OpenCode v2 configuration
            "mcp": {
                /* local servers */
                "servers": {
                    "other": { "type": "local", "command": ["other-tool"], },
                },
            },
        }"#;
        std::fs::write(&f, jsonc).unwrap();

        // State detection works despite comments and trailing commas
        assert_eq!(state(kind("opencode").format, &f, &s), State::Ready);

        // Adding updates the jsonc configuration
        set(kind("opencode"), &p, &s, true).unwrap();
        assert_eq!(state(kind("opencode").format, &f, &s), State::Added);

        let _ = std::fs::remove_dir_all(&d);
    }
}
