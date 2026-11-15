//! Plain text from TEXT and MTEXT content.
//!
//! The inline codes handled here are those listed in `ezdxf` 1.4.5 (`ezdxf/tools/text.py`, the
//! comment block "MTEXT inline codes" and `MTextParser`) and its special character table
//! (`ezdxf.lldxf.const.SPECIAL_CHAR_ENCODING`):
//!
//! | content | result |
//! |---|---|
//! | `\P` | line break |
//! | `\N` | column break, read as a line break |
//! | `\~` | a space (non-breaking space) |
//! | `\\`, `\{`, `\}` | the character after the backslash |
//! | `{` `}` | grouping, removed |
//! | `\L \l \O \o \K \k \X` | underline, overline, strike-through, dimension wrap: removed |
//! | `\H \W \F \f \C \c \T \Q \A \p` followed by arguments up to `;` | height, width, font, colour, tracking, oblique, alignment, paragraph: removed with the arguments |
//! | `\S` num `/` or `#` or `^` den `;` | stacked text, written `num/den` (`num^den` for `^`) |
//! | `\U+XXXX` | the Unicode character |
//! | `%%d`, `%%p`, `%%c`, `%%%` | degree sign, plus-minus sign, diameter sign, percent sign |
//! | `%%u`, `%%o`, `%%k` | underline, overline, strike-through toggles: removed |
//! | `^I` and other caret codes | the control character `chr((c - 64) % 126)`, as `ezdxf.tools.text.caret_decode` |
//!
//! Anything else after a backslash is dropped with the backslash and reported as `\?`.

use std::collections::BTreeSet;

fn caret_decode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut it = text.chars().peekable();
    while let Some(c) = it.next() {
        if c == '^' {
            if let Some(&n) = it.peek() {
                it.next();
                let v = (n as i64 - 64).rem_euclid(126) as u32; // Python's modulo, as ezdxf uses
                out.push(char::from_u32(v).unwrap_or(' '));
                continue;
            }
        }
        out.push(c);
    }
    out
}

/// `%%` codes and `\U+XXXX` in a TEXT value.
pub fn decode_text(text: &str, stripped: &mut BTreeSet<String>) -> String {
    decode_special(&caret_decode(text), stripped)
}

/// `%%` codes and `\U+XXXX`, without caret decoding.
fn decode_special(t: &str, stripped: &mut BTreeSet<String>) -> String {
    let chars: Vec<char> = t.chars().collect();
    let mut out = String::with_capacity(t.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '%' && chars.get(i + 1) == Some(&'%') {
            if let Some(&k) = chars.get(i + 2) {
                match k.to_ascii_lowercase() {
                    'd' => {
                        out.push('\u{b0}');
                        i += 3;
                        continue;
                    }
                    'p' => {
                        out.push('\u{b1}');
                        i += 3;
                        continue;
                    }
                    'c' => {
                        out.push('\u{d8}');
                        i += 3;
                        continue;
                    }
                    '%' => {
                        out.push('%');
                        i += 3;
                        continue;
                    }
                    'u' | 'o' | 'k' => {
                        stripped.insert(format!("%%{}", k.to_ascii_lowercase()));
                        i += 3;
                        continue;
                    }
                    _ => {}
                }
            }
        }
        if c == '\\' && chars.get(i + 1) == Some(&'U') && chars.get(i + 2) == Some(&'+') {
            let hex: String = chars[i + 3..].iter().take(4).collect();
            if hex.len() == 4 {
                if let Some(ch) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                    out.push(ch);
                    i += 7;
                    continue;
                }
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

/// Plain text of MTEXT content. Records each stripped code in `stripped`.
pub fn strip_mtext(content: &str, stripped: &mut BTreeSet<String>) -> String {
    // As ezdxf does, decode caret codes first: "^ " is the escape that writes a literal caret,
    // which is how a stacking caret survives in a file (`\S+0.1^ -0.1;`).
    let chars: Vec<char> = caret_decode(content).chars().collect();
    let mut out = String::with_capacity(content.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '{' | '}' => {
                stripped.insert("{}".into());
                i += 1;
            }
            '\\' => {
                let Some(&n) = chars.get(i + 1) else {
                    i += 1;
                    continue;
                };
                match n {
                    '\\' | '{' | '}' => {
                        out.push(n);
                        i += 2;
                    }
                    'P' => {
                        out.push('\n');
                        i += 2;
                    }
                    'N' => {
                        out.push('\n');
                        stripped.insert("\\N (read as a line break)".into());
                        i += 2;
                    }
                    '~' => {
                        out.push(' ');
                        i += 2;
                    }
                    'L' | 'l' | 'O' | 'o' | 'K' | 'k' | 'X' => {
                        stripped.insert(format!("\\{n}"));
                        i += 2;
                    }
                    'U' if chars.get(i + 2) == Some(&'+') => {
                        // Leave \U+XXXX for decode_text, which runs last.
                        out.push('\\');
                        out.push('U');
                        i += 2;
                    }
                    'S' => {
                        let end = chars[i + 2..].iter().position(|&c| c == ';').map(|p| i + 2 + p).unwrap_or(chars.len());
                        let body: String = chars[i + 2..end].iter().collect();
                        let rendered = stack(&body);
                        out.push_str(&rendered);
                        stripped.insert("\\S (stacked text written num/den)".into());
                        i = (end + 1).min(chars.len());
                    }
                    'H' | 'W' | 'F' | 'f' | 'C' | 'c' | 'T' | 'Q' | 'A' | 'p' => {
                        let end = chars[i + 2..].iter().position(|&c| c == ';').map(|p| i + 2 + p).unwrap_or(chars.len());
                        stripped.insert(format!("\\{n}"));
                        i = (end + 1).min(chars.len());
                    }
                    _ => {
                        stripped.insert("\\? (unknown code)".into());
                        i += 2;
                    }
                }
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    decode_special(&out, stripped)
}

/// `num^den`, `num/den` or `num#den` (backslash escapes the separator) as `num/den` or `num^den`.
fn stack(body: &str) -> String {
    let chars: Vec<char> = body.chars().collect();
    let (mut num, mut i) = (String::new(), 0);
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' && i + 1 < chars.len() {
            num.push(chars[i + 1]);
            i += 2;
            continue;
        }
        if matches!(c, '^' | '/' | '#') {
            let den: String = chars[i + 1..].iter().collect();
            let sep = if c == '^' { '^' } else { '/' };
            return format!("{num}{sep}{den}");
        }
        num.push(c);
        i += 1;
    }
    num
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strip(s: &str) -> (String, BTreeSet<String>) {
        let mut set = BTreeSet::new();
        (strip_mtext(s, &mut set), set)
    }

    #[test]
    fn paragraphs_escapes_and_groups() {
        assert_eq!(strip(r"Line 1\PLine 2").0, "Line 1\nLine 2");
        assert_eq!(strip(r"a\\b \{x\} {grouped}").0, r"a\b {x} grouped");
        assert_eq!(strip(r"no\~break").0, "no break");
        let (t, s) = strip(r"{\fArial|b0|i0;\H2.5;\C1;Hello} \Lunder\l");
        assert_eq!(t, "Hello under");
        for k in ["\\f", "\\H", "\\C", "\\L", "\\l", "{}"] {
            assert!(s.contains(k), "{k} {s:?}");
        }
    }

    #[test]
    fn stacking_unicode_and_percent_codes() {
        assert_eq!(strip(r"\S1/4;").0, "1/4");
        assert_eq!(strip(r"\S+0.1^ -0.1;").0, "+0.1^-0.1");
        assert_eq!(strip(r"\S1#2;").0, "1/2");
        assert_eq!(strip(r"45%%d %%p0.5 %%c20 100%%%").0, "45\u{b0} \u{b1}0.5 \u{d8}20 100%");
        assert_eq!(strip(r"\U+00B0C \U+4E2D").0, "\u{b0}C \u{4e2d}");
        let mut s = BTreeSet::new();
        assert_eq!(decode_text("%%uunder%%u", &mut s), "under");
        assert!(s.contains("%%u"));
    }

    #[test]
    fn caret_codes_and_unknown_codes() {
        assert_eq!(strip("a^Ib").0, "a\tb");
        let (t, s) = strip(r"x\Zy");
        assert_eq!(t, "xy");
        assert!(s.contains("\\? (unknown code)"));
        // A lone trailing backslash and an unterminated argument do not panic.
        assert_eq!(strip("end\\").0, "end");
        assert_eq!(strip(r"\Hnever ends").0, "");
    }
}
