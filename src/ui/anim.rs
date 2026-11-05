//! Animations: short and eased, the same everywhere (dialogs, floating panels and menus). Off in
//! Settings (Appearance, Animations): then everything is there at once. An animation lasts a fifth of a
//! second at most; events are handled between its frames (a window just shown is drawn only once the
//! system has shown it).

use fltk::app;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

static ON: AtomicBool = AtomicBool::new(true);

/// Turns animations on or off (Settings).
pub fn set(on: bool) {
    ON.store(on, Ordering::Relaxed);
}

/// Whether things move: off in Settings, and with the memory saver (it keeps no picture to fade).
pub fn on() -> bool {
    ON.load(Ordering::Relaxed) && !crate::memory_saver()
}

/// How long something takes to come in (ms).
pub const IN: u32 = 170;
/// And to go: quicker, nobody waits to see it leave.
pub const OUT: u32 = 120;
/// A menu or a floating panel unrolling: quick, it's asked for often.
pub const POP: u32 = 110;

/// Eased out: quick at first, settling at the end.
pub fn ease_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

/// Calls `frame(p)` with `p` going from 0 to 1 (eased) for `ms`, drawing after each, about 60 times a
/// second; the last call is always `p` = 1. With animations off, only that one.
pub fn play(ms: u32, mut frame: impl FnMut(f32)) {
    if on() && ms > 0 {
        let total = Duration::from_millis(ms.into()).as_secs_f32();
        let start = Instant::now();
        loop {
            let t = start.elapsed().as_secs_f32() / total;
            if t >= 1.0 {
                break;
            }
            frame(ease_out(t));
            app::flush();
            let next = Instant::now() + Duration::from_millis(14);
            while let Some(left) = next.checked_duration_since(Instant::now()) {
                if app::wait_for(left.as_secs_f64()).is_err() {
                    break;
                }
            }
        }
    }
    frame(1.0);
    app::flush();
}

/// From `a` to `b`, at `p` (0 to 1).
pub fn mix(a: i32, b: i32, p: f32) -> i32 {
    a + ((b - a) as f32 * p.clamp(0.0, 1.0)).round() as i32
}

/// `from` faded into `to` at `p` (0 to 1), written to `out`: pixels of the same size.
pub fn blend(from: &[u8], to: &[u8], p: f32, out: &mut Vec<u8>) {
    let a = (p.clamp(0.0, 1.0) * 256.0) as u16;
    out.clear();
    out.extend(
        from.iter()
            .zip(to)
            .map(|(&f, &t)| ((u16::from(f) * (256 - a) + u16::from(t) * a) >> 8) as u8),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn easing_starts_at_zero_ends_at_one_and_never_goes_back() {
        assert_eq!(ease_out(0.0), 0.0);
        assert_eq!(ease_out(1.0), 1.0);
        let steps: Vec<f32> = (0..=20).map(|i| ease_out(i as f32 / 20.0)).collect();
        assert!(steps.windows(2).all(|w| w[0] <= w[1]));
        assert!(ease_out(0.5) > 0.5, "eased out: most of the way by half time");
    }

    #[test]
    fn a_fade_goes_from_one_picture_to_the_other() {
        let (from, to) = ([0u8, 100, 255], [255u8, 100, 0]);
        let mut out = Vec::new();
        blend(&from, &to, 0.0, &mut out);
        assert_eq!(out, from);
        blend(&from, &to, 1.0, &mut out);
        assert_eq!(out, to);
        blend(&from, &to, 0.5, &mut out);
        assert_eq!(out, [127, 100, 127]);
        assert_eq!((mix(14, 0, 0.0), mix(14, 0, 0.5), mix(14, 0, 1.0)), (14, 7, 0));
    }

    #[test]
    fn with_animations_off_only_the_end_is_drawn() {
        set(false);
        let mut seen = Vec::new();
        play(IN, |p| seen.push(p));
        set(true);
        assert_eq!(seen, [1.0]);
    }
}
