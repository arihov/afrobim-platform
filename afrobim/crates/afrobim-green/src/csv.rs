//! Minimal CSV: comma separated, RFC 4180 quoting with `""`, `\n` line ends on output. A field
//! cannot contain a line break (the readers here work line by line so errors can name a line).

/// One field, quoted when it needs to be; line breaks become spaces.
pub fn field(s: &str) -> String {
    let s = if s.contains(['\n', '\r']) { s.replace(['\n', '\r'], " ") } else { s.to_string() };
    if s.contains([',', '"']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s
    }
}

/// One row with its line end.
pub fn row<S: AsRef<str>>(fields: &[S]) -> String {
    let mut out = fields.iter().map(|f| field(f.as_ref())).collect::<Vec<_>>().join(",");
    out.push('\n');
    out
}

/// Splits one line into fields.
pub fn split(line: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut f = String::new();
    let mut chars = line.chars().peekable();
    let mut quoted = false;
    let mut was_quoted = false;
    while let Some(c) = chars.next() {
        if quoted {
            match c {
                '"' if chars.peek() == Some(&'"') => {
                    chars.next();
                    f.push('"');
                }
                '"' => {
                    quoted = false;
                    if !matches!(chars.peek(), None | Some(',')) {
                        return Err("text after a closing quote".to_string());
                    }
                }
                _ => f.push(c),
            }
            continue;
        }
        match c {
            '"' if f.is_empty() && !was_quoted => {
                quoted = true;
                was_quoted = true;
            }
            '"' => return Err("a quote inside an unquoted field".to_string()),
            ',' => {
                out.push(std::mem::take(&mut f));
                was_quoted = false;
            }
            _ => f.push(c),
        }
    }
    if quoted {
        return Err("a quoted field is not closed".to_string());
    }
    out.push(f);
    Ok(out)
}
