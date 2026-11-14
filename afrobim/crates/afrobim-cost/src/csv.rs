//! Minimal CSV: RFC 4180 quoting, comma separated, `\n` line ends on output, `\r\n` or `\n` on
//! input, optional byte-order mark.

use crate::{CostError, Result};

/// One field, quoted when it needs to be.
pub fn field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// One row with its line end.
pub fn row<S: AsRef<str>>(fields: &[S]) -> String {
    let mut out = fields.iter().map(|f| field(f.as_ref())).collect::<Vec<_>>().join(",");
    out.push('\n');
    out
}

/// Parses text into rows of fields. Blank lines are skipped.
pub fn parse(text: &str) -> Result<Vec<Vec<String>>> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut cur: Vec<String> = Vec::new();
    let mut f = String::new();
    let mut quoted = false;
    let mut was_quoted = false;
    let mut chars = text.chars().peekable();
    let mut line = 1usize;
    let finish_row = |cur: &mut Vec<String>, f: &mut String, was_quoted: &mut bool, rows: &mut Vec<Vec<String>>| {
        if !(cur.is_empty() && f.is_empty() && !*was_quoted) {
            cur.push(std::mem::take(f));
            rows.push(std::mem::take(cur));
        }
        *was_quoted = false;
    };
    while let Some(c) = chars.next() {
        if quoted {
            match c {
                '"' if chars.peek() == Some(&'"') => {
                    chars.next();
                    f.push('"');
                }
                '"' => quoted = false,
                '\n' => {
                    line += 1;
                    f.push(c);
                }
                _ => f.push(c),
            }
            continue;
        }
        match c {
            '"' if f.is_empty() => {
                quoted = true;
                was_quoted = true;
            }
            '"' => return Err(CostError::Csv(format!("line {line}: a quote inside an unquoted field"))),
            ',' => {
                cur.push(std::mem::take(&mut f));
                was_quoted = false;
            }
            '\r' => {}
            '\n' => {
                finish_row(&mut cur, &mut f, &mut was_quoted, &mut rows);
                line += 1;
            }
            _ => f.push(c),
        }
    }
    if quoted {
        return Err(CostError::Csv(format!("line {line}: a quoted field is not closed")));
    }
    finish_row(&mut cur, &mut f, &mut was_quoted, &mut rows);
    Ok(rows)
}

/// Header-checked rows: the first row must hold exactly `expected` (case-insensitive, in order);
/// returns the data rows, each with exactly that many fields.
pub fn parse_with_header(text: &str, expected: &[&str]) -> Result<Vec<Vec<String>>> {
    let mut rows = parse(text)?;
    if rows.is_empty() {
        return Err(CostError::Csv("the file is empty".into()));
    }
    let header = rows.remove(0);
    let ok = header.len() == expected.len() && header.iter().zip(expected).all(|(a, b)| a.trim().eq_ignore_ascii_case(b));
    if !ok {
        return Err(CostError::Csv(format!("the header is '{}', expected '{}'", header.join(","), expected.join(","))));
    }
    for (i, r) in rows.iter().enumerate() {
        if r.len() != expected.len() {
            return Err(CostError::Csv(format!("row {} has {} fields, expected {}", i + 2, r.len(), expected.len())));
        }
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting_round_trip() {
        let fields = ["plain", "has,comma", "has \"quote\"", "two\nlines", ""];
        let text = row(&fields);
        let back = parse(&text).unwrap();
        assert_eq!(back, vec![fields.iter().map(|s| s.to_string()).collect::<Vec<_>>()]);
    }

    #[test]
    fn crlf_bom_blank_lines() {
        let rows = parse("\u{feff}a,b\r\n\r\nc,d\r\n").unwrap();
        assert_eq!(rows, vec![vec!["a", "b"], vec!["c", "d"]]);
    }

    #[test]
    fn errors() {
        assert!(parse("a,\"b").is_err());
        assert!(parse("a,b\"c").is_err());
        assert!(parse_with_header("x,y\n1,2\n", &["a", "b"]).is_err());
        assert!(parse_with_header("a,b\n1\n", &["a", "b"]).is_err());
    }
}
