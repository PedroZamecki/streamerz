//! Lucide controls and the existing platform SVGs share one asset source.

use gpui_kit::{AssetSource, SharedString};
use std::borrow::Cow;

#[path = "../../assets/lucide.rs"]
mod lucide;

/// The badges and logos the app is built with, by the path the window loads them from.
const BUNDLED: &[(&str, &[u8])] = &[
    (
        "badges/kick-broadcaster.svg",
        include_bytes!("../../assets/badges/kick-broadcaster.svg"),
    ),
    (
        "badges/kick-moderator.svg",
        include_bytes!("../../assets/badges/kick-moderator.svg"),
    ),
    (
        "badges/kick-vip.svg",
        include_bytes!("../../assets/badges/kick-vip.svg"),
    ),
    ("badges/kick-og.svg", include_bytes!("../../assets/badges/kick-og.svg")),
    (
        "badges/kick-founder.svg",
        include_bytes!("../../assets/badges/kick-founder.svg"),
    ),
    (
        "badges/kick-verified.svg",
        include_bytes!("../../assets/badges/kick-verified.svg"),
    ),
    (
        "badges/kick-staff.svg",
        include_bytes!("../../assets/badges/kick-staff.svg"),
    ),
    (
        "badges/kick-sub_gifter.svg",
        include_bytes!("../../assets/badges/kick-sub_gifter.svg"),
    ),
    (
        "badges/kick-subscriber.svg",
        include_bytes!("../../assets/badges/kick-subscriber.svg"),
    ),
    (
        "badges/kick-sidekick.svg",
        include_bytes!("../../assets/badges/kick-sidekick.svg"),
    ),
    (
        "badges/youtube-owner.svg",
        include_bytes!("../../assets/badges/youtube-owner.svg"),
    ),
    (
        "badges/youtube-moderator.svg",
        include_bytes!("../../assets/badges/youtube-moderator.svg"),
    ),
    (
        "badges/youtube-verified.svg",
        include_bytes!("../../assets/badges/youtube-verified.svg"),
    ),
    (
        "badges/youtube-member.svg",
        include_bytes!("../../assets/badges/youtube-member.svg"),
    ),
    (
        "badges/tiktok-host.svg",
        include_bytes!("../../assets/badges/tiktok-host.svg"),
    ),
    (
        "badges/tiktok-moderator.svg",
        include_bytes!("../../assets/badges/tiktok-moderator.svg"),
    ),
    (
        "badges/tiktok-subscriber.svg",
        include_bytes!("../../assets/badges/tiktok-subscriber.svg"),
    ),
    (
        "badges/tiktok-friend.svg",
        include_bytes!("../../assets/badges/tiktok-friend.svg"),
    ),
    (
        "brands/streamerz.svg",
        include_bytes!("../../assets/brand/streamerz.svg"),
    ),
    (
        "brands/streamerz-dark.svg",
        include_bytes!("../../assets/brand/streamerz-dark.svg"),
    ),
    (
        "brands/streamerz-dev.svg",
        include_bytes!("../../assets/brand/streamerz-dev-proposal.svg"),
    ),
    ("brands/7tv.svg", include_bytes!("../../assets/icons/7tv.svg")),
    ("brands/bttv.svg", include_bytes!("../../assets/icons/bttv.svg")),
    ("brands/ffz.svg", include_bytes!("../../assets/icons/ffz.svg")),
    ("brands/amp.svg", include_bytes!("../../assets/icons/amp.svg")),
    (
        "brands/antigravity.svg",
        include_bytes!("../../assets/icons/antigravity.svg"),
    ),
    ("brands/auggie.svg", include_bytes!("../../assets/icons/auggie.svg")),
    ("brands/brave.svg", include_bytes!("../../assets/icons/brave.svg")),
    ("brands/chromium.svg", include_bytes!("../../assets/icons/chromium.svg")),
    ("brands/claude.svg", include_bytes!("../../assets/icons/claude.svg")),
    ("brands/cline.svg", include_bytes!("../../assets/icons/cline.svg")),
    ("brands/codex.svg", include_bytes!("../../assets/icons/codex.svg")),
    ("brands/copilot.svg", include_bytes!("../../assets/icons/copilot.svg")),
    (
        "brands/crowdcontrol.svg",
        include_bytes!("../../assets/icons/crowdcontrol.svg"),
    ),
    ("brands/crush.svg", include_bytes!("../../assets/icons/crush.svg")),
    ("brands/cursor.svg", include_bytes!("../../assets/icons/cursor.svg")),
    ("brands/devin.svg", include_bytes!("../../assets/icons/devin.svg")),
    ("brands/discord.svg", include_bytes!("../../assets/icons/discord.svg")),
    (
        "brands/donordrive.svg",
        include_bytes!("../../assets/icons/donordrive.svg"),
    ),
    ("brands/elgato.svg", include_bytes!("../../assets/icons/elgato.svg")),
    ("brands/firefox.svg", include_bytes!("../../assets/icons/firefox.svg")),
    (
        "brands/fourthwall.png",
        include_bytes!("../../assets/icons/fourthwall.png"),
    ),
    ("brands/gemini.svg", include_bytes!("../../assets/icons/gemini.svg")),
    (
        "brands/googlechrome.svg",
        include_bytes!("../../assets/icons/googlechrome.svg"),
    ),
    ("brands/hyperate.png", include_bytes!("../../assets/icons/hyperate.png")),
    ("brands/kick.svg", include_bytes!("../../assets/icons/kick.svg")),
    ("brands/kimi.svg", include_bytes!("../../assets/icons/kimi.svg")),
    ("brands/kiro.svg", include_bytes!("../../assets/icons/kiro.svg")),
    ("brands/ko-fi.png", include_bytes!("../../assets/icons/ko-fi.png")),
    ("brands/lmstudio.svg", include_bytes!("../../assets/icons/lmstudio.svg")),
    (
        "brands/meld-studio.png",
        include_bytes!("../../assets/icons/meld-studio.png"),
    ),
    (
        "brands/microsoftedge.svg",
        include_bytes!("../../assets/icons/microsoftedge.svg"),
    ),
    ("brands/mistral.svg", include_bytes!("../../assets/icons/mistral.svg")),
    (
        "brands/obsstudio.svg",
        include_bytes!("../../assets/icons/obsstudio.svg"),
    ),
    ("brands/opencode.svg", include_bytes!("../../assets/icons/opencode.svg")),
    ("brands/opera.svg", include_bytes!("../../assets/icons/opera.svg")),
    ("brands/operagx.svg", include_bytes!("../../assets/icons/operagx.svg")),
    ("brands/pallygg.svg", include_bytes!("../../assets/icons/pallygg.svg")),
    ("brands/patreon.svg", include_bytes!("../../assets/icons/patreon.svg")),
    ("brands/polypop.png", include_bytes!("../../assets/icons/polypop.png")),
    ("brands/pulsoid.png", include_bytes!("../../assets/icons/pulsoid.png")),
    ("brands/qwen.svg", include_bytes!("../../assets/icons/qwen.svg")),
    ("brands/roocode.svg", include_bytes!("../../assets/icons/roocode.svg")),
    ("brands/shopify.svg", include_bytes!("../../assets/icons/shopify.svg")),
    (
        "brands/speakerbot.png",
        include_bytes!("../../assets/icons/speakerbot.png"),
    ),
    ("brands/spotify.png", include_bytes!("../../assets/icons/spotify.png")),
    ("brands/spotify.svg", include_bytes!("../../assets/icons/spotify.svg")),
    (
        "brands/streamelements.svg",
        include_bytes!("../../assets/icons/streamelements.svg"),
    ),
    (
        "brands/streamerbot.svg",
        include_bytes!("../../assets/icons/streamerbot.svg"),
    ),
    (
        "brands/streamlabs.svg",
        include_bytes!("../../assets/icons/streamlabs.svg"),
    ),
    ("brands/tiktok.svg", include_bytes!("../../assets/icons/tiktok.svg")),
    ("brands/twitch.svg", include_bytes!("../../assets/icons/twitch.svg")),
    ("brands/vivaldi.svg", include_bytes!("../../assets/icons/vivaldi.svg")),
    ("brands/voicemod.png", include_bytes!("../../assets/icons/voicemod.png")),
    ("brands/vscode.svg", include_bytes!("../../assets/icons/vscode.svg")),
    (
        "brands/vtube-studio.png",
        include_bytes!("../../assets/icons/vtube-studio.png"),
    ),
    ("brands/windsurf.svg", include_bytes!("../../assets/icons/windsurf.svg")),
    ("brands/youtube.svg", include_bytes!("../../assets/icons/youtube.svg")),
    ("brands/zed.svg", include_bytes!("../../assets/icons/zed.svg")),
    ("brands/zen.svg", include_bytes!("../../assets/icons/zen.svg")),
    ("brands/helium.svg", include_bytes!("../../assets/icons/helium.svg")),
    ("brands/arc.svg", include_bytes!("../../assets/icons/arc.svg")),
    ("brands/tor.svg", include_bytes!("../../assets/icons/tor.svg")),
    (
        "brands/librewolf.svg",
        include_bytes!("../../assets/icons/librewolf.svg"),
    ),
    ("brands/waterfox.svg", include_bytes!("../../assets/icons/waterfox.svg")),
    ("brands/browser.svg", include_bytes!("../../assets/icons/browser.svg")),
];

pub struct Assets;
impl AssetSource for Assets {
    fn load(&self, path: &str) -> gpui_kit::Result<Option<Cow<'static, [u8]>>> {
        if let Some(name) = path.strip_prefix("icons/").and_then(|name| name.strip_suffix(".svg")) {
            if let Some((_, body)) = lucide::ICONS.iter().find(|(id, _)| *id == name) {
                return Ok(Some(Cow::Owned(format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">{body}</svg>"#).into_bytes())));
            }
        }
        if let Some((_, bytes)) = BUNDLED.iter().find(|(name, _)| *name == path) {
            return Ok(Some(Cow::Borrowed(bytes)));
        }
        gpui_kit::assets::AllAssets.load(path)
    }
    fn list(&self, path: &str) -> gpui_kit::Result<Vec<SharedString>> {
        let mut assets = gpui_kit::assets::AllAssets.list(path)?;
        assets.extend(
            BUNDLED
                .iter()
                .map(|(name, _)| *name)
                .filter(|name| name.starts_with(path))
                .map(SharedString::from),
        );
        Ok(assets)
    }
}

#[cfg(test)]
mod tests {
    use super::{Assets, BUNDLED};
    use gpui_kit::AssetSource;

    #[test]
    fn every_bundled_picture_loads_and_is_listed_once() {
        let mut names: Vec<&str> = BUNDLED.iter().map(|(name, _)| *name).collect();
        names.sort_unstable();
        let count = names.len();
        names.dedup();
        assert_eq!(names.len(), count, "a path is in the table twice");
        for (name, bytes) in BUNDLED {
            assert!(!bytes.is_empty(), "{name}");
            assert_eq!(Assets.load(name).unwrap().as_deref(), Some(*bytes), "{name}");
        }
        let brands = Assets.list("brands/").unwrap();
        assert!(brands.iter().any(|name| name == "brands/streamerz-dev.svg"));
        assert!(brands.iter().all(|name| name.starts_with("brands/")));
        assert!(Assets
            .list("badges/")
            .unwrap()
            .iter()
            .any(|name| name == "badges/kick-vip.svg"));
    }

    #[test]
    fn a_lucide_icon_is_an_svg_in_the_text_colour() {
        let icon = Assets.load("icons/plug.svg").unwrap().expect("plug is a Lucide icon");
        let text = std::str::from_utf8(&icon).unwrap();
        assert!(text.starts_with("<svg ") && text.ends_with("</svg>"), "{text}");
        assert!(crate::asset("brands/no-such-brand.svg").is_none());
    }
}
