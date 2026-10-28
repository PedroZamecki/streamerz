//! Text that doesn't fit: a label or a menu's choice is cut with "..." (the whole text on hover), a
//! button grows for its words (see `button`) and, when its row is still too narrow, shows only its
//! icon. All of it is decided as the widget is drawn, so a window resize or another language just works.

use super::icons;
use fltk::button::Button;
use fltk::draw;
use fltk::enums::{Align, Color};
use fltk::frame::Frame;
use fltk::menu::Choice;
use fltk::prelude::*;

/// `text` cut to `room` wide by `width`, with "..." where it was cut.
pub fn ellipsize(text: &str, room: i32, width: impl Fn(&str) -> i32) -> String {
    if width(text) <= room {
        return text.to_string();
    }
    let dots = "...";
    let budget = room - width(dots);
    let mut end = 0;
    for (i, c) in text.char_indices() {
        let next = i + c.len_utf8();
        if width(&text[..next]) > budget {
            break;
        }
        end = next;
    }
    format!("{}{dots}", text[..end].trim_end())
}

/// Measures with FLTK's current font.
fn measure(t: &str) -> i32 {
    draw::width(t) as i32
}

/// A label's text as drawn (FLTK doubles its `@`).
fn plain(label: &str) -> String {
    label.replace("@@", "@")
}

/// Tooltip of a widget that cut its text: the whole text; cleared again when it fits.
fn tip(w: &mut impl WidgetExt, full: &str, cut: bool) {
    let shown = w.tooltip().unwrap_or_default();
    if cut && shown != full {
        w.set_tooltip(&full.replace('@', "@@"));
    } else if !cut && shown == full.replace('@', "@@") {
        w.set_tooltip("");
    }
}

pub trait Ellipsis {
    /// A one-line label that is cut with "..." instead of clipped.
    fn ellipsis(self) -> Self;
}

impl Ellipsis for Frame {
    fn ellipsis(mut self) -> Self {
        self.draw(|f| {
            let label = plain(&f.label());
            draw::set_font(f.label_font(), f.label_size());
            let img = f.image();
            let pad = img.as_ref().map_or(0, |i| i.w());
            let text = ellipsize(&label, f.w() - pad, measure);
            tip(f, &label, text != label);
            if let Some(mut i) = img {
                i.draw(f.x(), f.y() + (f.h() - i.h()) / 2, i.w(), i.h());
            }
            draw::set_draw_color(if f.active() {
                f.label_color()
            } else {
                f.label_color().inactive()
            });
            draw::draw_text2(
                &text,
                f.x() + pad,
                f.y(),
                f.w() - pad,
                f.h(),
                f.align() & !Align::Inside | Align::Inside,
            );
        });
        self
    }
}

/// A drop-down whose chosen text is cut with "..." to the room it has.
pub fn choice() -> Choice {
    let mut c = Choice::default();
    c.draw(|c| {
        let ink = if c.active() {
            Color::Foreground
        } else {
            Color::Foreground.inactive()
        };
        draw::draw_box(c.frame(), c.x(), c.y(), c.w(), c.h(), c.color());
        let full = c.choice().unwrap_or_default();
        draw::set_font(c.text_font(), c.text_size());
        let text = ellipsize(&full, c.w() - 34, measure);
        tip(c, &full, text != full);
        draw::set_draw_color(ink);
        draw::draw_text2(&text, c.x() + 9, c.y(), c.w() - 30, c.h(), Align::Left | Align::Inside);
        if let Some(mut i) = icons::icon("chevrons-up-down", ink, 14) {
            i.draw(c.x() + c.w() - 22, c.y() + (c.h() - 14) / 2, 14, 14);
        }
    });
    c
}

/// A button that fits its room: icon and words, only the icon (the words as its tooltip), or, with no
/// icon, the words cut with "...". The icon is the button's image (`with_icon` sets it).
pub fn adaptive(b: &mut Button) {
    b.draw(|b| {
        let frame = if b.value() { b.down_frame() } else { b.frame() };
        draw::draw_box(frame, b.x(), b.y(), b.w(), b.h(), b.color());
        let ink = if b.active() {
            b.label_color()
        } else {
            b.label_color().inactive()
        };
        let label = plain(&b.label());
        draw::set_font(b.label_font(), b.label_size());
        let img = b.image();
        // The image carries a gap after it (before_label); the icon itself is the 14 px before it.
        let (icon_w, lead) = img.as_ref().map_or((0, 0), |i| (14, i.w()));
        let room = b.w() - 12;
        let words = measure(&label);
        let (text, icon_only) = if lead + words <= room || label.is_empty() {
            (label.clone(), false)
        } else if icon_w > 0 {
            (String::new(), true)
        } else {
            (ellipsize(&label, room, measure), false)
        };
        tip(b, &label, text != label);
        let used = if icon_only { icon_w } else { lead + measure(&text) };
        let mut x = b.x() + (b.w() - used) / 2;
        if let Some(mut i) = img {
            i.draw(x, b.y() + (b.h() - i.h()) / 2, i.w(), i.h());
            x += lead;
        }
        draw::set_draw_color(ink);
        draw::draw_text2(&text, x, b.y(), measure(&text) + 2, b.h(), Align::Left | Align::Inside);
    });
}

/// Wide enough for `label` and its icon, whatever `want` says (an estimate: measuring would open the
/// display before the window is shown).
pub fn grown(want: i32, label: &str, icon: bool) -> i32 {
    let words = label.chars().count() as i32 * 15 / 2;
    want.max(words + if icon { 21 } else { 0 } + 24)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(t: &str) -> i32 {
        t.chars().count() as i32 * 7
    }

    #[test]
    fn text_that_fits_is_left_alone_and_the_rest_ends_in_dots() {
        assert_eq!(ellipsize("Ask before updating", 200, w), "Ask before updating");
        let cut = ellipsize("Perguntar antes de atualizar", 150, w);
        assert!(cut.ends_with("...") && w(&cut) <= 150, "{cut}");
        assert!(cut.starts_with("Perguntar antes"), "{cut}");
    }

    #[test]
    fn no_room_never_panics_and_multibyte_text_is_cut_on_a_letter() {
        assert_eq!(ellipsize("abcdef", 10, w), "...");
        assert_eq!(ellipsize("", 0, w), "");
        let cut = ellipsize("ação de atualização", 80, w);
        assert!(w(&cut) <= 80, "{cut}");
    }

    #[test]
    fn a_button_grows_for_its_words() {
        assert!(grown(90, "Sair da conta", true) > 90);
        assert_eq!(grown(200, "OK", false), 200);
    }
}
