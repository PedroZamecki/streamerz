//! Stable page identities and translated navigation metadata for any frontend.

/// Sections and their pages: (page label, icon).
pub type Page = (&'static str, &'static str, &'static str);
pub type Section = (&'static str, &'static str, &'static [Page]);

pub const NAV: &[Section] = &[
    (
        "Live",
        "nav.section.live",
        &[
            ("Chat", "nav.page.chat", "message-square"),
            ("Stream", "nav.page.stream", "radio"),
            ("Accounts", "nav.page.accounts", "user-round"),
            ("Community", "nav.page.community", "users"),
            ("Polls", "nav.page.polls", "vote"),
        ],
    ),
    (
        "Automation",
        "nav.section.automation",
        &[
            ("Actions", "nav.page.actions", "zap"),
            ("Commands", "nav.page.commands", "terminal"),
            ("Timers", "nav.page.timers", "timer"),
            ("Files", "nav.page.files", "file-text"),
            ("Overlays", "nav.page.overlays", "layers"),
            ("Queues", "nav.page.queues", "list-ordered"),
            ("Groups", "nav.page.groups", "folder"),
            ("Plugins", "nav.page.plugins", "puzzle"),
        ],
    ),
    (
        "Connections",
        "nav.section.connections",
        &[
            ("Servers", "nav.page.servers", "server"),
            ("Stream Apps", "nav.page.stream_apps", "monitor-play"),
            ("Integrations", "nav.page.integrations", "plug"),
            ("Donations", "nav.page.donations", "heart"),
        ],
    ),
    (
        "Data",
        "nav.section.data",
        &[
            ("Globals", "nav.page.globals", "braces"),
            ("History", "nav.page.history", "history"),
        ],
    ),
    (
        "App",
        "nav.section.app",
        &[
            ("Settings", "nav.page.settings", "settings"),
            ("MCP", "nav.page.mcp", "bot"),
            ("Log", "nav.page.log", "scroll-text"),
            ("Crashes", "nav.page.crashes", "bug"),
            ("Backups", "nav.page.backups", "archive"),
        ],
    ),
];

pub fn snapshot() -> serde_json::Value {
    serde_json::json!(NAV.iter().map(|(id, label, pages)| {
        serde_json::json!({"id": id, "label": crate::i18n::tr(label), "pages": pages.iter().map(|(id, label, icon)| {
            serde_json::json!({"id": id, "label": crate::i18n::tr(label), "icon": icon, "view": crate::core::view::supports(&id.to_lowercase().replace(' ', "_"))})
        }).collect::<Vec<_>>()})
    }).collect::<Vec<_>>())
}
