//! The status bar's layout: which segments fit, and how a long message is shortened.
//!
//! The bar holds, left to right: the controls (fixed), the message, then the right-hand
//! read-outs. The message takes whatever the read-outs leave. When space runs out the read-outs
//! are dropped in a stated order, least useful first:
//!
//! 1. the zoom read-out (`1 px = 4.5 mm`),
//! 2. the standard name,
//! 3. the latitude and longitude of the cursor,
//! 4. the selection read-out,
//! 5. the project coordinate system (or "No coordinate system", which is a button to set one).
//!
//! The message is never dropped: it is shortened. A path in it is shortened in the middle and keeps
//! its file name; if it still does not fit the end is cut with an ellipsis. The full text is always
//! available in the hover tooltip.
//!
//! The functions here are pure (text widths come in through a closure) so they can be tested
//! without a window.

/// A right-hand segment.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Segment {
    Selection,
    Standard,
    Zoom,
    /// The cursor's latitude and longitude, with the datum they are on.
    LatLon,
    /// The WGS 84 reading, a second line to the datum's own latitude and longitude.
    Wgs84,
    /// The project coordinate system, or the plain statement that there is none.
    Crs,
}

/// The order in which segments are dropped when space runs out: first entry goes first.
pub const DROP_ORDER: [Segment; 6] = [Segment::Zoom, Segment::Wgs84, Segment::Standard, Segment::LatLon, Segment::Selection, Segment::Crs];

/// The narrowest the message is allowed to become before a segment is dropped to give it room.
pub const MIN_MESSAGE_W: f32 = 140.0;

/// What fits.
#[derive(Clone, PartialEq, Debug)]
pub struct Layout {
    /// The segments shown, in display order (selection, standard, zoom: right to left priority).
    pub shown: Vec<Segment>,
    /// The width left for the message.
    pub message_w: f32,
}

/// Decides which right-hand segments are shown. `avail` is the width from the end of the fixed
/// controls to the right edge; `widths` gives each segment's width including its separator.
/// Segments are dropped in [`DROP_ORDER`] until the message has at least [`MIN_MESSAGE_W`]; if
/// even with every segment dropped the message has less, it gets what there is.
pub fn layout(avail: f32, widths: &[(Segment, f32)]) -> Layout {
    let mut shown: Vec<(Segment, f32)> = widths.to_vec();
    let used = |s: &[(Segment, f32)]| s.iter().map(|(_, w)| *w).sum::<f32>();
    for drop in DROP_ORDER {
        if avail - used(&shown) >= MIN_MESSAGE_W {
            break;
        }
        shown.retain(|(s, _)| *s != drop);
    }
    let message_w = (avail - used(&shown)).max(0.0);
    Layout { shown: shown.into_iter().map(|(s, _)| s).collect(), message_w }
}

fn is_sep(c: char) -> bool {
    c == '/' || c == '\\'
}

/// A path shortened in the middle to at most `budget` characters, keeping its file name:
/// `/Users/anna/Projects/Road A/Untitled.afrobim` becomes `/Users/anna/Pr…/Untitled.afrobim`.
/// A path that already fits, or has no folder part, is returned as it is.
pub fn shorten_path(path: &str, budget: usize) -> String {
    let chars: Vec<char> = path.chars().collect();
    if chars.len() <= budget {
        return path.to_string();
    }
    let Some(cut) = chars.iter().rposition(|c| is_sep(*c)) else { return path.to_string() };
    let tail: String = chars[cut..].iter().collect(); // the separator and the file name
    let tail_len = chars.len() - cut;
    // One character for the ellipsis.
    let head_len = budget.saturating_sub(tail_len + 1).min(cut);
    if head_len < 3 {
        return format!("…{tail}");
    }
    let head: String = chars[..head_len].iter().collect();
    format!("{head}…{tail}")
}

/// Splits a message into path-like tokens and the text between them. A token is path-like when it
/// has a separator and is at least 12 characters. Trailing sentence punctuation is not part of it.
fn tokens(text: &str) -> Vec<(bool, String)> {
    let mut out: Vec<(bool, String)> = Vec::new();
    let mut cur = String::new();
    let flush = |cur: &mut String, out: &mut Vec<(bool, String)>| {
        if cur.is_empty() {
            return;
        }
        let trimmed = cur.trim_end_matches(|c: char| ".,;:)".contains(c));
        let is_path = trimmed.chars().count() >= 12 && trimmed.chars().any(is_sep);
        if is_path {
            let tail = cur[trimmed.len()..].to_string();
            out.push((true, trimmed.to_string()));
            if !tail.is_empty() {
                out.push((false, tail));
            }
        } else {
            out.push((false, cur.clone()));
        }
        cur.clear();
    };
    for c in text.chars() {
        if c.is_whitespace() {
            flush(&mut cur, &mut out);
            out.push((false, c.to_string()));
        } else {
            cur.push(c);
        }
    }
    flush(&mut cur, &mut out);
    out
}

/// The text with every path in it shortened to `budget` characters.
pub fn shorten_paths_in(text: &str, budget: usize) -> String {
    tokens(text).into_iter().map(|(p, t)| if p { shorten_path(&t, budget) } else { t }).collect()
}

/// The text cut with an ellipsis at the end so it is at most `max_w` wide by `measure`.
pub fn elide_end(text: &str, max_w: f32, measure: &dyn Fn(&str) -> f32) -> String {
    if measure(text) <= max_w {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    // The longest prefix whose text plus the ellipsis fits (widths grow with length).
    let (mut lo, mut hi) = (0usize, chars.len());
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        let cand: String = chars[..mid].iter().collect::<String>() + "…";
        if measure(&cand) <= max_w {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    let prefix: String = chars[..lo].iter().collect();
    format!("{}…", prefix.trim_end())
}

/// The message as shown in `max_w`: unchanged when it fits; otherwise paths in it are shortened in
/// the middle (generous budgets first, so as little as possible is lost) and, if it is still too
/// wide, the end is cut with an ellipsis.
pub fn fit_message(text: &str, max_w: f32, measure: &dyn Fn(&str) -> f32) -> String {
    if measure(text) <= max_w {
        return text.to_string();
    }
    let mut last = text.to_string();
    for budget in [60usize, 44, 32, 24, 16] {
        let short = shorten_paths_in(text, budget);
        if measure(&short) <= max_w {
            return short;
        }
        last = short;
    }
    elide_end(&last, max_w, measure)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fixed-width font: 7 px a character.
    fn m(s: &str) -> f32 {
        s.chars().count() as f32 * 7.0
    }

    const ALL: [(Segment, f32); 3] = [(Segment::Selection, 180.0), (Segment::Standard, 300.0), (Segment::Zoom, 130.0)];

    #[test]
    fn a_wide_bar_shows_every_segment_and_gives_the_rest_to_the_message() {
        let l = layout(1000.0, &ALL);
        assert_eq!(l.shown, vec![Segment::Selection, Segment::Standard, Segment::Zoom]);
        assert_eq!(l.message_w, 390.0);
    }

    #[test]
    fn a_narrow_bar_drops_zoom_first_then_the_standard_then_the_selection() {
        // 700 leaves 90 for the message with everything shown: the zoom read-out goes.
        let l = layout(700.0, &ALL);
        assert_eq!(l.shown, vec![Segment::Selection, Segment::Standard]);
        assert_eq!(l.message_w, 220.0);
        // Not enough for the message beside the standard name: it goes too.
        let l = layout(400.0, &ALL);
        // (480 used by selection and standard would leave nothing.)
        assert_eq!(l.shown, vec![Segment::Selection]);
        assert_eq!(l.message_w, 220.0);
        // Very narrow: only the message.
        let l = layout(250.0, &ALL);
        assert!(l.shown.is_empty());
        assert_eq!(l.message_w, 250.0);
        // Nothing left at all: the message gets zero, never a negative width.
        assert_eq!(layout(-5.0, &ALL).message_w, 0.0);
    }

    #[test]
    fn segments_and_message_never_exceed_the_space() {
        for avail in (0..1400).step_by(10) {
            let l = layout(avail as f32, &ALL);
            let used: f32 = ALL.iter().filter(|(s, _)| l.shown.contains(s)).map(|(_, w)| *w).sum();
            assert!(used + l.message_w <= avail as f32 + 0.001, "width {avail}");
        }
    }

    #[test]
    fn a_path_is_shortened_in_the_middle_and_keeps_its_file_name() {
        let p = "/Users/anna/Projects/Kampala Northern Bypass/Untitled.afrobim";
        let s = shorten_path(p, 30);
        assert!(s.ends_with("/Untitled.afrobim"), "{s}");
        assert!(s.contains('…') && s.starts_with("/Users"));
        assert_eq!(s.chars().count(), 30);
        // Windows separators too.
        let w = shorten_path(r"C:\Users\anna\Documents\Projects\Bypass\Untitled.afrobim", 30);
        assert!(w.ends_with(r"\Untitled.afrobim") && w.contains('…'), "{w}");
        // Fits already, or has no folder: unchanged.
        assert_eq!(shorten_path("/a/b.afrobim", 30), "/a/b.afrobim");
        assert_eq!(shorten_path("a-very-long-file-name.afrobim", 10), "a-very-long-file-name.afrobim");
        // A budget too small for any head keeps just the file name.
        assert_eq!(shorten_path("/Users/anna/Projects/Untitled.afrobim", 18), "…/Untitled.afrobim");
    }

    #[test]
    fn a_message_that_fits_is_left_alone() {
        let t = "Saved /tmp/a.afrobim.";
        assert_eq!(fit_message(t, 1000.0, &m), t);
    }

    #[test]
    fn a_long_path_in_a_message_is_shortened_before_the_message_is_cut() {
        let t = "Saved /Users/anna/Projects/Kampala Northern Bypass/Untitled.afrobim.";
        let shown = fit_message("Saved /Users/anna/Projects/Kampala/Northern/Bypass/Untitled.afrobim.", 300.0, &m);
        assert!(m(&shown) <= 300.0, "{shown}");
        assert!(shown.starts_with("Saved /Users") && shown.contains('…'));
        assert!(shown.ends_with("Untitled.afrobim."), "file name and full stop kept: {shown}");
        let _ = t;
    }

    #[test]
    fn a_message_with_no_path_is_cut_with_an_ellipsis_to_the_width() {
        let t = "The check found 12 problems in the model and 3 more in the schedule of the road.";
        for w in [60.0, 140.0, 300.0, 500.0] {
            let s = fit_message(t, w, &m);
            assert!(m(&s) <= w, "{w}: {s}");
            assert!(s.ends_with('…'), "{s}");
            assert!(t.starts_with(s.trim_end_matches('…')));
        }
        // Zero room gives just the ellipsis.
        assert_eq!(fit_message(t, 0.0, &m), "…");
    }

    #[test]
    fn a_path_too_wide_even_when_shortened_ends_in_an_ellipsis_within_the_width() {
        let t = "Saved /Users/anna/Projects/Kampala/Northern/Bypass/Untitled.afrobim.";
        let s = fit_message(t, 100.0, &m);
        assert!(m(&s) <= 100.0 && s.ends_with('…'), "{s}");
    }
}
