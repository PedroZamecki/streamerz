//! Stable page identities and translated navigation metadata for any frontend.

/// Sections and their pages: (page id, label key, icon).
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
            ("Moderation", "nav.page.moderation", "shield"),
            ("Polls", "nav.page.polls", "vote"),
            ("Rewards", "nav.page.rewards", "gift"),
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
            ("Sounds", "nav.page.sounds", "volume-2"),
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

/// A page's domain: its id in lower case with underscores (`Stream Apps` is `stream_apps`).
pub fn domain(page: &str) -> String {
    page.to_lowercase().replace(' ', "_")
}

/// The sidebar page a domain belongs to, if it is one (a dialog's or a popup's domain isn't).
pub fn page_of(domain: &str) -> Option<&'static str> {
    NAV.iter()
        .flat_map(|(_, _, pages)| pages.iter())
        .map(|(id, _, _)| *id)
        .find(|id| self::domain(id) == domain)
}

pub fn snapshot() -> serde_json::Value {
    serde_json::json!(NAV.iter().map(|(id, label, pages)| {
        serde_json::json!({"id": id, "label": crate::i18n::tr(label), "pages": pages.iter().map(|(id, label, icon)| {
            serde_json::json!({"id": id, "label": crate::i18n::tr(label), "icon": icon, "view": crate::core::view::supports(&domain(id))})
        }).collect::<Vec<_>>()})
    }).collect::<Vec<_>>())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_page_has_a_unique_domain_that_leads_back_to_it() {
        let pages: Vec<&str> = NAV
            .iter()
            .flat_map(|(_, _, pages)| pages.iter())
            .map(|(id, _, _)| *id)
            .collect();
        let mut domains: Vec<String> = pages.iter().map(|page| domain(page)).collect();
        for (page, domain) in pages.iter().zip(&domains) {
            assert_eq!(page_of(domain), Some(*page));
            assert!(!domain.contains(' ') && *domain == domain.to_lowercase(), "{domain}");
        }
        domains.sort();
        domains.dedup();
        assert_eq!(domains.len(), pages.len(), "two pages share a domain");
        assert_eq!(domain("Stream Apps"), "stream_apps");
        assert_eq!(page_of("stream_apps"), Some("Stream Apps"));
        assert_eq!(page_of("user_card"), None);
        assert_eq!(page_of(""), None);
    }
}
