//! The app's logo, one picture per look. Both the window's and the tray's follow the theme (Settings):
//! the window gets the colour logo, the tray the plainer black and white one. Pictures come from
//! `assets/brand` (`dev/icons.sh` redraws them from the SVGs) as raw 64x64 RGBA, built in.

pub const SIZE: i32 = 64;

const APP: [&[u8]; 2] = [
    include_bytes!("../assets/brand/rgba/streamerz-dark.rgba"),
    include_bytes!("../assets/brand/rgba/streamerz.rgba"),
];
const TRAY: [&[u8]; 2] = [
    include_bytes!("../assets/brand/rgba/streamerz-simple-dark.rgba"),
    include_bytes!("../assets/brand/rgba/streamerz-simple.rgba"),
];

/// The window's icon, for the dark or the light theme.
pub fn app(dark: bool) -> &'static [u8] {
    APP[usize::from(!dark)]
}

/// The tray's icon, for the dark or the light theme.
pub fn tray(dark: bool) -> &'static [u8] {
    TRAY[usize::from(!dark)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_picture_is_a_whole_image_and_each_look_differs() {
        for set in [APP, TRAY] {
            assert!(set.iter().all(|p| p.len() == (SIZE * SIZE * 4) as usize));
            assert_ne!(set[0], set[1]);
        }
        assert_ne!(app(true), tray(true));
        assert_eq!(app(true), APP[0]);
        assert_eq!(tray(false), TRAY[1]);
    }
}
