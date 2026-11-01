//! The app's logo, one picture per look. Both the window's and the tray's follow the theme (Settings):
//! the window gets the colour logo, the tray the plainer black and white one. Pictures come from
//! `assets/brand` (`dev/icons.sh` redraws them from the SVGs) as raw 64x64 RGBA, built in.

pub const SIZE: i32 = 64;

/// The project's home on GitHub (owner/name): the updater and the About page use it.
pub const REPO: &str = "PedroZamecki/streamerz";

/// Only release builds supplied a version by the build pipeline are production builds.
pub const IS_DEV: bool = cfg!(debug_assertions) || option_env!("STREAMERZ_VERSION").is_none();

/// The release version, or the source revision for a development build.
pub const VERSION: &str = if IS_DEV {
    env!("STREAMERZ_DEV_VERSION")
} else {
    match option_env!("STREAMERZ_VERSION") {
        Some(v) => v,
        None => env!("STREAMERZ_DEV_VERSION"),
    }
};

const APP: [&[u8]; 2] = [
    include_bytes!("../assets/brand/rgba/streamerz-dark.rgba"),
    include_bytes!("../assets/brand/rgba/streamerz.rgba"),
];
const TRAY: [&[u8]; 2] = [
    include_bytes!("../assets/brand/rgba/streamerz-simple.rgba"),
    include_bytes!("../assets/brand/rgba/streamerz-simple-dark.rgba"),
];

/// The window's icon, for the dark or the light theme.
pub fn app(dark: bool) -> &'static [u8] {
    if IS_DEV {
        include_bytes!("../assets/brand/rgba/streamerz-dev-proposal.rgba")
    } else {
        APP[usize::from(!dark)]
    }
}

/// The tray's icon, for the dark or the light theme.
pub fn tray(dark: bool) -> &'static [u8] {
    if IS_DEV {
        include_bytes!("../assets/brand/rgba/streamerz-dev-proposal.rgba")
    } else {
        TRAY[usize::from(!dark)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn development_version_identifies_the_revision() {
        if IS_DEV {
            let revision = VERSION.strip_suffix("-dirty").unwrap_or(VERSION);
            assert!(revision == "unknown" || revision.chars().all(|c| c.is_ascii_hexdigit()));
            assert!(!revision.is_empty());
        } else {
            assert_eq!(Some(VERSION), option_env!("STREAMERZ_VERSION"));
        }
    }

    #[test]
    fn every_picture_is_a_whole_image_and_each_look_differs() {
        for set in [APP, TRAY] {
            assert!(set.iter().all(|p| p.len() == (SIZE * SIZE * 4) as usize));
            assert_ne!(set[0], set[1]);
        }
        if IS_DEV {
            assert_eq!(app(true), tray(true));
        } else {
            assert_ne!(app(true), tray(true));
            assert_eq!(app(true), APP[0]);
            assert_eq!(tray(true), TRAY[0]);
            assert_eq!(tray(false), TRAY[1]);
        }
    }
}
