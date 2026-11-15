//! Reading an ASCII DXF as a stream of records.
//!
//! An ASCII DXF is a sequence of lines in pairs: a group code (an integer) and a value. A
//! *record* here is a group code 0 pair (`0`, TYPE) and every pair up to the next 0. Everything
//! in the file, section markers, table entries, blocks and entities, is such a record, which
//! lets one loop read the whole file with memory proportional to one record.
//! (Structure as read by `ezdxf` 1.4.5, `ezdxf.lldxf.tagger`; the Autodesk DXF Reference pages were
//! not available here and are not cited.)
//!
//! Group 999 is a comment and is skipped wherever it appears.

use std::io::BufRead;

use super::ReadError;

/// One pair's value, as text. Numbers are parsed on demand.
pub type Pair = (i32, String);

/// A group code 0 record: its type and the pairs that follow.
#[derive(Debug, Clone, Default)]
pub struct Record {
    pub kind: String,
    pub pairs: Vec<Pair>,
}

/// Lookup helpers over a record's pairs. All take the first occurrence unless stated.
pub struct View<'a>(pub &'a [Pair]);

impl<'a> View<'a> {
    pub fn s(&self, code: i32) -> Option<&'a str> {
        self.0.iter().find(|p| p.0 == code).map(|p| p.1.as_str())
    }
    pub fn f(&self, code: i32) -> Option<f64> {
        self.s(code).and_then(parse_f64)
    }
    pub fn i(&self, code: i32) -> Option<i64> {
        self.s(code).and_then(|v| v.trim().parse::<i64>().ok().or_else(|| parse_f64(v).map(|f| f as i64)))
    }
    pub fn all_f(&self, code: i32) -> Vec<f64> {
        self.0.iter().filter(|p| p.0 == code).filter_map(|p| parse_f64(&p.1)).collect()
    }
    /// X, Y of a point whose X is group `code` and Y is `code + 10`.
    pub fn xy(&self, code: i32) -> Option<(f64, f64)> {
        Some((self.f(code)?, self.f(code + 10).unwrap_or(0.0)))
    }
    /// X, Y, Z of a point.
    pub fn xyz(&self, code: i32) -> Option<(f64, f64, f64)> {
        Some((self.f(code)?, self.f(code + 10).unwrap_or(0.0), self.f(code + 20).unwrap_or(0.0)))
    }
}

pub fn parse_f64(v: &str) -> Option<f64> {
    let t = v.trim();
    // DXF writers use plain decimals; some add a leading '+', and Fortran-style D exponents occur in the wild.
    let r = t.parse::<f64>().ok().or_else(|| t.replace(['D', 'd'], "e").parse::<f64>().ok())?;
    r.is_finite().then_some(r)
}

/// Pair reader with one record of lookahead.
pub struct Records<R: BufRead> {
    r: R,
    buf: Vec<u8>,
    pub line: usize,
    pending: Option<Pair>,
    done: bool,
    pub warnings: Vec<String>,
}

fn decode(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        // Older files use a code page (usually Windows 1252). Without a table here, map each
        // byte to the Unicode code point of the same number (exact for ASCII and Latin-1).
        Err(_) => bytes.iter().map(|&b| b as char).collect(),
    }
}

impl<R: BufRead> Records<R> {
    pub fn new(r: R) -> Self {
        Records { r, buf: Vec::with_capacity(256), line: 0, pending: None, done: false, warnings: Vec::new() }
    }

    fn read_line(&mut self) -> Result<Option<String>, ReadError> {
        self.buf.clear();
        let n = self.r.read_until(b'\n', &mut self.buf)?;
        if n == 0 {
            return Ok(None);
        }
        self.line += 1;
        while matches!(self.buf.last(), Some(b'\n' | b'\r')) {
            self.buf.pop();
        }
        Ok(Some(decode(&self.buf)))
    }

    /// The next pair, skipping comments. A group code that is not an integer is skipped with a
    /// warning (the line is taken to be stray) so one bad line does not lose the file.
    fn next_pair(&mut self) -> Result<Option<Pair>, ReadError> {
        if let Some(p) = self.pending.take() {
            return Ok(Some(p));
        }
        loop {
            let Some(code_line) = self.read_line()? else { return Ok(None) };
            let Ok(code) = code_line.trim().parse::<i32>() else {
                if code_line.trim().is_empty() {
                    continue;
                }
                if self.warnings.len() < 20 {
                    self.warnings.push(format!("Line {}: '{}' is not a group code; skipped.", self.line, code_line.trim().chars().take(30).collect::<String>()));
                }
                continue;
            };
            let Some(value) = self.read_line()? else {
                self.warnings.push(format!("The file ends after the group code on line {}.", self.line));
                return Ok(None);
            };
            if code == 999 {
                continue;
            }
            return Ok(Some((code, value)));
        }
    }

    /// The next record, or `None` at the end of the file.
    pub fn next_record(&mut self) -> Result<Option<Record>, ReadError> {
        if self.done {
            return Ok(None);
        }
        // Find the group 0 that starts the record.
        let first = loop {
            match self.next_pair()? {
                None => {
                    self.done = true;
                    return Ok(None);
                }
                Some((0, v)) => break v,
                Some(_) => {
                    // Pairs before the first 0 (a stray header) are ignored.
                    continue;
                }
            }
        };
        let mut rec = Record { kind: first.trim().to_ascii_uppercase(), pairs: Vec::new() };
        loop {
            match self.next_pair()? {
                None => {
                    self.done = true;
                    break;
                }
                Some((0, v)) => {
                    self.pending = Some((0, v));
                    break;
                }
                Some(p) => rec.pairs.push(p),
            }
        }
        Ok(Some(rec))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recs(text: &str) -> (Vec<Record>, Vec<String>) {
        let mut r = Records::new(text.as_bytes());
        let mut out = Vec::new();
        while let Some(x) = r.next_record().unwrap() {
            out.push(x);
        }
        (out, r.warnings)
    }

    #[test]
    fn records_split_on_group_zero_and_skip_comments() {
        let (r, w) = recs("0\nSECTION\n2\nENTITIES\n999\nmade by x\n0\nLINE\n8\nA\n10\n1.5\n20\n2\r\n0\nENDSEC\n0\nEOF\n");
        assert!(w.is_empty());
        let kinds: Vec<&str> = r.iter().map(|x| x.kind.as_str()).collect();
        assert_eq!(kinds, ["SECTION", "LINE", "ENDSEC", "EOF"]);
        let v = View(&r[1].pairs);
        assert_eq!(v.s(8), Some("A"));
        assert_eq!(v.xy(10), Some((1.5, 2.0))); // a CRLF value still parses
    }

    #[test]
    fn a_bad_group_code_line_is_skipped_with_a_warning() {
        let (r, w) = recs("0\nLINE\nxx\n8\nA\n0\nEOF\n");
        assert_eq!(r.len(), 2);
        assert_eq!(w.len(), 1);
        assert!(w[0].contains("Line 3"));
        // The stray line does not shift the pairing: 8 / A survive.
        assert_eq!(View(&r[0].pairs).s(8), Some("A"));
    }

    #[test]
    fn a_truncated_file_returns_what_it_has() {
        let (r, w) = recs("0\nLINE\n8\nA\n10\n");
        assert_eq!(r.len(), 1);
        assert!(!w.is_empty());
    }

    #[test]
    fn numbers_accept_plus_signs_exponents_and_d_exponents_but_not_nan() {
        assert_eq!(parse_f64(" +1.5e2 "), Some(150.0));
        assert_eq!(parse_f64("1.0D2"), Some(100.0));
        assert_eq!(parse_f64("nan"), None);
        assert_eq!(parse_f64("inf"), None);
        assert_eq!(parse_f64(""), None);
    }

    #[test]
    fn non_utf8_bytes_decode_as_latin_1() {
        assert_eq!(decode(&[b'a', 0xB0, b'b']), "a\u{b0}b");
    }
}
