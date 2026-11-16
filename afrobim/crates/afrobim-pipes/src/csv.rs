//! A small CSV reader and writer (RFC 4180 quoting) so that no other dependency is needed.

pub(crate) fn escape(field: &str) -> String {
    if field.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}

pub(crate) fn row(fields: &[String]) -> String {
    let mut s = fields.iter().map(|f| escape(f)).collect::<Vec<_>>().join(",");
    s.push('\n');
    s
}

/// Split CSV text into records of fields. Blank lines are skipped. A quoted field may contain
/// commas, doubled quotes and line breaks.
pub(crate) fn parse(text: &str) -> Result<Vec<Vec<String>>, String> {
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut was_quoted = false;
    let mut chars = text.trim_start_matches('\u{feff}').chars().peekable();
    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    in_quotes = false;
                }
            } else {
                field.push(c);
            }
            continue;
        }
        match c {
            '"' if field.is_empty() => {
                in_quotes = true;
                was_quoted = true;
            }
            ',' => {
                row.push(std::mem::take(&mut field));
                was_quoted = false;
            }
            '\r' => {}
            '\n' => {
                row.push(std::mem::take(&mut field));
                if !(row.len() == 1 && row[0].is_empty() && !was_quoted) {
                    rows.push(std::mem::take(&mut row));
                } else {
                    row.clear();
                }
                was_quoted = false;
            }
            _ => field.push(c),
        }
    }
    if in_quotes {
        return Err("a quoted field is not closed".to_string());
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    Ok(rows)
}
