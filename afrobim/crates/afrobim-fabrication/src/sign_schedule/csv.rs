//! A small CSV reader and writer for the sign schedule.
//!
//! Reads comma, semicolon or tab separated text with quoted fields (`"` doubled inside a quoted
//! field, line breaks allowed inside quotes), LF, CRLF or CR line ends, and UTF-8 with or
//! without a byte order mark. Lines whose first cell starts with `#` are comments; lines with
//! no content at all are blank. Neither is data, and both are listed in the result so nothing
//! disappears without a trace.

/// A parsed file: the header cells and the data records.
#[derive(Debug, Clone, PartialEq)]
pub struct CsvTable {
    /// The delimiter that was used (read from the header line unless one was forced).
    pub delimiter: char,
    /// The header cells, trimmed.
    pub headers: Vec<String>,
    pub rows: Vec<CsvRow>,
    /// Line numbers of comment lines (first cell starts with `#`), including any before the header.
    pub comment_lines: Vec<usize>,
    /// Line numbers of blank lines.
    pub blank_lines: Vec<usize>,
}

/// One data record. `line` is the 1-based line of the file the record starts on.
#[derive(Debug, Clone, PartialEq)]
pub struct CsvRow {
    pub line: usize,
    /// The cells, untrimmed as quoted or not (surrounding blanks of unquoted cells removed).
    pub cells: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum CsvError {
    #[error("the file has no header row")]
    Empty,
    #[error("a quoted field that starts on line {line} is never closed")]
    UnterminatedQuote { line: usize },
}

/// Parses `text`, finding the delimiter from the header line.
pub fn parse(text: &str) -> Result<CsvTable, CsvError> {
    parse_with(text, None)
}

/// Parses `text` with a given delimiter, or finds one when `delimiter` is `None`: the one of
/// `,` `;` tab that occurs most often outside quotes on the header line (comma when none does).
pub fn parse_with(text: &str, delimiter: Option<char>) -> Result<CsvTable, CsvError> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let delimiter = delimiter.unwrap_or_else(|| detect(text));
    let records = split_records(text, delimiter)?;

    let mut table = CsvTable { delimiter, headers: Vec::new(), rows: Vec::new(), comment_lines: Vec::new(), blank_lines: Vec::new() };
    let mut have_header = false;
    for (line, cells) in records {
        if cells.iter().all(|c| c.trim().is_empty()) {
            table.blank_lines.push(line);
        } else if cells[0].trim_start().starts_with('#') {
            table.comment_lines.push(line);
        } else if !have_header {
            table.headers = cells.iter().map(|c| c.trim().to_string()).collect();
            have_header = true;
        } else {
            table.rows.push(CsvRow { line, cells });
        }
    }
    if !have_header {
        return Err(CsvError::Empty);
    }
    Ok(table)
}

/// The delimiter of the first line that has content, outside quotes.
fn detect(text: &str) -> char {
    let mut counts = [0usize; 3];
    let mut in_quotes = false;
    let mut seen_content = false;
    for c in text.chars() {
        match c {
            '"' => {
                in_quotes = !in_quotes;
                seen_content = true;
            }
            '\n' | '\r' if !in_quotes => {
                if seen_content {
                    break;
                }
            }
            ',' if !in_quotes => counts[0] += 1,
            ';' if !in_quotes => counts[1] += 1,
            '\t' if !in_quotes => counts[2] += 1,
            c if !c.is_whitespace() => seen_content = true,
            _ => {}
        }
    }
    let best = counts.iter().copied().max().unwrap_or(0);
    if best == 0 || counts[0] == best {
        ','
    } else if counts[1] == best {
        ';'
    } else {
        '\t'
    }
}

/// Splits into (start line, cells) records.
fn split_records(text: &str, delim: char) -> Result<Vec<(usize, Vec<String>)>, CsvError> {
    let mut records: Vec<(usize, Vec<String>)> = Vec::new();
    let mut cells: Vec<String> = Vec::new();
    let mut cell = String::new();
    let mut quoted_cell = false;
    let mut in_quotes = false;
    let mut line = 1usize;
    let mut record_line = 1usize;
    let mut quote_line = 1usize;
    let mut chars = text.chars().peekable();
    let finish_cell = |cell: &mut String, quoted: &mut bool, cells: &mut Vec<String>| {
        let v = if *quoted { std::mem::take(cell) } else { cell.trim().to_string() };
        cell.clear();
        *quoted = false;
        cells.push(v);
    };
    while let Some(c) = chars.next() {
        if in_quotes {
            match c {
                '"' if chars.peek() == Some(&'"') => {
                    chars.next();
                    cell.push('"');
                }
                '"' => in_quotes = false,
                '\n' => {
                    line += 1;
                    cell.push('\n');
                }
                '\r' => {
                    if chars.peek() == Some(&'\n') {
                        chars.next();
                    }
                    line += 1;
                    cell.push('\n');
                }
                c => cell.push(c),
            }
            continue;
        }
        match c {
            '"' if cell.trim().is_empty() && !quoted_cell => {
                cell.clear();
                quoted_cell = true;
                in_quotes = true;
                quote_line = line;
            }
            c if c == delim => finish_cell(&mut cell, &mut quoted_cell, &mut cells),
            '\n' | '\r' => {
                if c == '\r' && chars.peek() == Some(&'\n') {
                    chars.next();
                }
                finish_cell(&mut cell, &mut quoted_cell, &mut cells);
                records.push((record_line, std::mem::take(&mut cells)));
                line += 1;
                record_line = line;
            }
            c => cell.push(c),
        }
    }
    if in_quotes {
        return Err(CsvError::UnterminatedQuote { line: quote_line });
    }
    if !cell.is_empty() || quoted_cell || !cells.is_empty() {
        finish_cell(&mut cell, &mut quoted_cell, &mut cells);
        records.push((record_line, cells));
    }
    Ok(records)
}

/// Quotes a value for output when it needs it.
pub fn quote(value: &str, delimiter: char) -> String {
    let needs = value.contains(delimiter)
        || value.contains(['"', '\n', '\r'])
        || value.starts_with(' ')
        || value.ends_with(' ')
        || value.starts_with('#');
    if needs {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

/// Joins already-unquoted cells into one line (no line end).
pub fn join_line(cells: &[String], delimiter: char) -> String {
    cells.iter().map(|c| quote(c, delimiter)).collect::<Vec<_>>().join(&delimiter.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comma_semicolon_and_tab_are_detected() {
        assert_eq!(parse("a,b,c\n1,2,3\n").unwrap().delimiter, ',');
        assert_eq!(parse("a;b;c\n1;2;3\n").unwrap().delimiter, ';');
        assert_eq!(parse("a\tb\tc\n1\t2\t3\n").unwrap().delimiter, '\t');
        let t = parse("a;b;c\n1;2;3\n").unwrap();
        assert_eq!(t.headers, ["a", "b", "c"]);
        assert_eq!(t.rows[0].cells, ["1", "2", "3"]);
    }

    #[test]
    fn a_decimal_comma_in_a_semicolon_file_does_not_confuse_detection() {
        let t = parse("a;b;c\n1,5;2,5;3\n").unwrap();
        assert_eq!(t.delimiter, ';');
        assert_eq!(t.rows[0].cells, ["1,5", "2,5", "3"]);
    }

    #[test]
    fn quoted_fields_hold_delimiters_quotes_and_line_breaks() {
        let t = parse("a,b\n\"x,y\",\"say \"\"hi\"\"\"\n\"two\nlines\",z\n").unwrap();
        assert_eq!(t.rows[0].cells, ["x,y", "say \"hi\""]);
        assert_eq!(t.rows[1].cells, ["two\nlines", "z"]);
        assert_eq!(t.rows[1].line, 3);
    }

    #[test]
    fn bom_crlf_and_cr_are_handled() {
        let t = parse("\u{feff}a,b\r\n1,2\r\n3,4\r5,6").unwrap();
        assert_eq!(t.headers, ["a", "b"]);
        assert_eq!(t.rows.len(), 3);
        assert_eq!(t.rows[2].cells, ["5", "6"]);
        assert_eq!(t.rows[2].line, 4);
    }

    #[test]
    fn comments_and_blanks_are_listed_not_dropped() {
        let t = parse("# a note\na,b\n\n#c,d\n1,2\n").unwrap();
        assert_eq!(t.comment_lines, [1, 4]);
        assert_eq!(t.blank_lines, [3]);
        assert_eq!(t.rows.len(), 1);
        assert_eq!(t.rows[0].line, 5);
    }

    #[test]
    fn errors() {
        assert_eq!(parse("").unwrap_err(), CsvError::Empty);
        assert_eq!(parse("\n\n").unwrap_err(), CsvError::Empty);
        assert_eq!(parse("a,b\n\"open,1\n").unwrap_err(), CsvError::UnterminatedQuote { line: 2 });
    }

    #[test]
    fn quoting_round_trips() {
        let cells = vec!["a,b".to_string(), "x\"y".to_string(), " pad".to_string(), "#c".to_string(), "plain".to_string()];
        let line = join_line(&cells, ',');
        let t = parse(&format!("h1,h2,h3,h4,h5\n{line}\n")).unwrap();
        assert_eq!(t.rows[0].cells, cells);
    }
}
