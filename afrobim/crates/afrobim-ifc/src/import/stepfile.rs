//! Reader for STEP physical files (ISO 10303-21 clear text encoding): header, and an instance
//! table over the DATA section.
//!
//! The table stores, per instance, its id, its entity name (interned) and the byte range of its
//! argument list inside the file buffer. Attribute lists are parsed on demand, so a file of a few
//! tens of megabytes costs the buffer plus 16 bytes per instance, not a value tree per instance.
//!
//! Grammar handled: `$` (null), `*` (derived), integers, reals (`1.`, `1.5E-3`), strings with `''`
//! and the `\X\`, `\X2\...\X0\`, `\X4\...\X0\`, `\S\` and `\P?\` control directives, enumerations
//! (`.T.`), entity references (`#12`), lists, typed values (`IFCLABEL('x')`), binaries (`"..."`),
//! `/* */` comments, and complex instances (`#5=(A(...)B(...));`, kept but not interpreted).

use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Derived,
    Int(i64),
    Real(f64),
    Str(String),
    Enum(String),
    Ref(u32),
    List(Vec<Value>),
    /// `NAME(value)`; several arguments become a list.
    Typed(String, Box<Value>),
    Binary(String),
}

impl Value {
    pub fn as_ref(&self) -> Option<u32> {
        match self {
            Value::Ref(r) => Some(*r),
            _ => None,
        }
    }
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Real(r) => Some(*r),
            Value::Int(i) => Some(*i as f64),
            Value::Typed(_, v) => v.as_f64(),
            _ => None,
        }
    }
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            Value::Real(r) if r.fract() == 0.0 => Some(*r as i64),
            Value::Typed(_, v) => v.as_i64(),
            _ => None,
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            Value::Typed(_, v) => v.as_str(),
            _ => None,
        }
    }
    pub fn as_enum(&self) -> Option<&str> {
        match self {
            Value::Enum(s) => Some(s),
            Value::Typed(_, v) => v.as_enum(),
            _ => None,
        }
    }
    pub fn as_list(&self) -> Option<&[Value]> {
        match self {
            Value::List(l) => Some(l),
            _ => None,
        }
    }
    /// A list of numbers, `None` if any element is not one.
    pub fn as_reals(&self) -> Option<Vec<f64>> {
        self.as_list()?.iter().map(Value::as_f64).collect()
    }
    /// A list of references (non-references are skipped).
    pub fn refs(&self) -> Vec<u32> {
        match self {
            Value::List(l) => l.iter().filter_map(Value::as_ref).collect(),
            Value::Ref(r) => vec![*r],
            _ => Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepError {
    NotStep(String),
    Syntax { offset: usize, message: String },
    TooLarge(String),
}

impl std::fmt::Display for StepError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StepError::NotStep(s) => write!(f, "not a STEP physical file: {s}"),
            StepError::Syntax { offset, message } => write!(f, "STEP syntax error at byte {offset}: {message}"),
            StepError::TooLarge(s) => write!(f, "file too large: {s}"),
        }
    }
}

impl std::error::Error for StepError {}

#[derive(Debug, Clone, Copy)]
struct Inst {
    id: u32,
    ty: u16,
    start: u32,
    end: u32,
}

/// What the HEADER section says.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StepHeader {
    /// `FILE_SCHEMA` identifiers, e.g. `["IFC4"]`.
    pub schemas: Vec<String>,
    /// `FILE_NAME` originating system and preprocessor version, when present.
    pub originating_system: String,
    pub file_name: String,
}

enum IdIndex {
    Dense(Vec<u32>),
    Sparse(HashMap<u32, u32>),
}

/// A parsed file: buffer, header and instance table.
pub struct StepFile {
    data: Vec<u8>,
    types: Vec<String>,
    inst: Vec<Inst>,
    index: IdIndex,
    pub header: StepHeader,
    /// Complex instances (`#n=(A()B());`): counted, not interpreted.
    pub complex_instances: usize,
}

pub const NONE: u32 = u32::MAX;

fn is_ws(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\r' | b'\n' | 0x0B | 0x0C)
}

/// Skips whitespace and `/* */` comments.
fn skip_ws(d: &[u8], mut i: usize) -> usize {
    loop {
        while i < d.len() && is_ws(d[i]) {
            i += 1;
        }
        if i + 1 < d.len() && d[i] == b'/' && d[i + 1] == b'*' {
            i += 2;
            while i + 1 < d.len() && !(d[i] == b'*' && d[i + 1] == b'/') {
                i += 1;
            }
            i = (i + 2).min(d.len());
        } else {
            return i;
        }
    }
}

/// Index of the byte after the string that starts at `i` (`d[i] == '\''`).
fn skip_string(d: &[u8], mut i: usize) -> usize {
    i += 1;
    while i < d.len() {
        if d[i] == b'\'' {
            if i + 1 < d.len() && d[i + 1] == b'\'' {
                i += 2;
                continue;
            }
            return i + 1;
        }
        i += 1;
    }
    d.len()
}

/// Given `d[open] == '('`, returns the index of the matching `)`.
fn matching_paren(d: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut i = open;
    while i < d.len() {
        match d[i] {
            b'\'' => {
                i = skip_string(d, i);
                continue;
            }
            b'/' if i + 1 < d.len() && d[i + 1] == b'*' => {
                i = skip_ws(d, i);
                continue;
            }
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn find(d: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    d[from.min(d.len())..].windows(needle.len()).position(|w| w.eq_ignore_ascii_case(needle)).map(|p| p + from)
}

/// Decodes the body of a STEP string (without the quotes).
pub fn decode_string(raw: &[u8]) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut i = 0;
    let mut plain_start = 0;
    let flush = |out: &mut String, from: usize, to: usize| {
        if to > from {
            out.push_str(&String::from_utf8_lossy(&raw[from..to]));
        }
    };
    while i < raw.len() {
        match raw[i] {
            b'\'' if i + 1 < raw.len() && raw[i + 1] == b'\'' => {
                flush(&mut out, plain_start, i);
                out.push('\'');
                i += 2;
                plain_start = i;
            }
            b'\\' => {
                // Control directives: \\ \X\HH \X2\...\X0\ \X4\...\X0\ \S\c \P?\
                let rest = &raw[i..];
                let hex = |b: &[u8]| -> Option<u32> { u32::from_str_radix(std::str::from_utf8(b).ok()?, 16).ok() };
                if rest.starts_with(b"\\\\") {
                    flush(&mut out, plain_start, i);
                    out.push('\\');
                    i += 2;
                    plain_start = i;
                } else if rest.starts_with(b"\\X\\") && rest.len() >= 5 {
                    if let Some(v) = hex(&rest[3..5]) {
                        flush(&mut out, plain_start, i);
                        out.push(char::from_u32(v).unwrap_or('\u{FFFD}'));
                        i += 5;
                        plain_start = i;
                        continue;
                    }
                    i += 1;
                } else if rest.starts_with(b"\\X2\\") || rest.starts_with(b"\\X4\\") {
                    let width = if rest[2] == b'2' { 4 } else { 8 };
                    if let Some(end) = find(rest, 4, b"\\X0\\") {
                        flush(&mut out, plain_start, i);
                        let digits = &rest[4..end];
                        if width == 4 {
                            let units: Vec<u16> = digits.chunks(4).filter_map(|c| hex(c).map(|v| v as u16)).collect();
                            out.push_str(&String::from_utf16_lossy(&units));
                        } else {
                            for c in digits.chunks(8) {
                                out.push(hex(c).and_then(char::from_u32).unwrap_or('\u{FFFD}'));
                            }
                        }
                        i += end + 4;
                        plain_start = i;
                    } else {
                        i += 1;
                    }
                } else if rest.starts_with(b"\\S\\") && rest.len() >= 4 {
                    flush(&mut out, plain_start, i);
                    out.push(char::from_u32(rest[3] as u32 + 128).unwrap_or('\u{FFFD}'));
                    i += 4;
                    plain_start = i;
                } else if rest.len() >= 4 && rest[1] == b'P' && rest[3] == b'\\' {
                    // Code page selection: the following \S\ characters stay ISO 8859-1 here.
                    flush(&mut out, plain_start, i);
                    i += 4;
                    plain_start = i;
                } else {
                    i += 1;
                }
            }
            _ => i += 1,
        }
    }
    flush(&mut out, plain_start, raw.len());
    out
}

struct Parser<'a> {
    d: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn ws(&mut self) {
        self.i = skip_ws(self.d, self.i);
    }

    fn value(&mut self) -> Option<Value> {
        self.ws();
        let c = *self.d.get(self.i)?;
        match c {
            b'$' => {
                self.i += 1;
                Some(Value::Null)
            }
            b'*' => {
                self.i += 1;
                Some(Value::Derived)
            }
            b'#' => {
                let s = self.i + 1;
                let mut e = s;
                while e < self.d.len() && self.d[e].is_ascii_digit() {
                    e += 1;
                }
                let id = std::str::from_utf8(&self.d[s..e]).ok()?.parse().ok()?;
                self.i = e;
                Some(Value::Ref(id))
            }
            b'\'' => {
                let end = skip_string(self.d, self.i);
                let body = &self.d[self.i + 1..end.saturating_sub(1).max(self.i + 1)];
                self.i = end;
                Some(Value::Str(decode_string(body)))
            }
            b'"' => {
                let s = self.i + 1;
                let e = self.d[s..].iter().position(|&b| b == b'"').map(|p| p + s)?;
                self.i = e + 1;
                Some(Value::Binary(String::from_utf8_lossy(&self.d[s..e]).into_owned()))
            }
            b'.' if self.d.get(self.i + 1).is_some_and(|c| c.is_ascii_alphabetic() || *c == b'_') => {
                let s = self.i + 1;
                let e = self.d[s..].iter().position(|&b| b == b'.').map(|p| p + s)?;
                self.i = e + 1;
                Some(Value::Enum(String::from_utf8_lossy(&self.d[s..e]).to_ascii_uppercase()))
            }
            b'(' => {
                self.i += 1;
                let mut items = Vec::new();
                loop {
                    self.ws();
                    match self.d.get(self.i)? {
                        b')' => {
                            self.i += 1;
                            return Some(Value::List(items));
                        }
                        b',' => self.i += 1,
                        _ => items.push(self.value()?),
                    }
                }
            }
            b'-' | b'+' | b'.' | b'0'..=b'9' => {
                let s = self.i;
                let mut e = s + 1;
                let mut real = c == b'.';
                while e < self.d.len() {
                    match self.d[e] {
                        b'0'..=b'9' => {}
                        b'.' | b'e' | b'E' => real = true,
                        b'-' | b'+' if matches!(self.d[e - 1], b'e' | b'E') => {}
                        _ => break,
                    }
                    e += 1;
                }
                let t = std::str::from_utf8(&self.d[s..e]).ok()?;
                self.i = e;
                if real {
                    t.parse::<f64>().ok().map(Value::Real)
                } else {
                    match t.parse::<i64>() {
                        Ok(v) => Some(Value::Int(v)),
                        Err(_) => t.parse::<f64>().ok().map(Value::Real),
                    }
                }
            }
            c if c.is_ascii_alphabetic() || c == b'_' => {
                let s = self.i;
                let mut e = s;
                while e < self.d.len() && (self.d[e].is_ascii_alphanumeric() || self.d[e] == b'_') {
                    e += 1;
                }
                let name = String::from_utf8_lossy(&self.d[s..e]).to_ascii_uppercase();
                self.i = e;
                self.ws();
                if self.d.get(self.i) == Some(&b'(') {
                    let inner = self.value()?;
                    let v = match inner {
                        Value::List(mut l) if l.len() == 1 => l.remove(0),
                        other => other,
                    };
                    Some(Value::Typed(name, Box::new(v)))
                } else {
                    Some(Value::Enum(name))
                }
            }
            _ => None,
        }
    }
}

/// Parses one value at the start of `d`.
pub fn parse_value(d: &[u8]) -> Option<Value> {
    Parser { d, i: 0 }.value()
}

/// A view of one instance. `args` is the text between the outer parentheses.
#[derive(Clone, Copy)]
pub struct Entity<'a> {
    pub id: u32,
    /// Upper case entity name; `"(COMPLEX)"` for a complex instance.
    pub ty: &'a str,
    args: &'a [u8],
}

impl<'a> Entity<'a> {
    pub fn raw_args(&self) -> &'a [u8] {
        self.args
    }

    /// Byte offset of attribute `n` within the arguments, or `None` if there are fewer.
    pub fn attr_offset(&self, n: usize) -> Option<usize> {
        let d = self.args;
        let mut i = skip_ws(d, 0);
        let mut k = 0;
        while k < n {
            // Skip one attribute up to the next top-level comma.
            let mut depth = 0i32;
            loop {
                if i >= d.len() {
                    return None;
                }
                match d[i] {
                    b'\'' => {
                        i = skip_string(d, i);
                        continue;
                    }
                    b'/' if i + 1 < d.len() && d[i + 1] == b'*' => {
                        i = skip_ws(d, i);
                        continue;
                    }
                    b'(' => depth += 1,
                    b')' => depth -= 1,
                    b',' if depth == 0 => break,
                    _ => {}
                }
                i += 1;
            }
            i = skip_ws(d, i + 1);
            k += 1;
        }
        (i < d.len()).then_some(i)
    }

    /// Attribute `n` (zero based), parsed. `Value::Null` when the attribute is absent.
    pub fn attr(&self, n: usize) -> Value {
        match self.attr_offset(n) {
            Some(o) => parse_value(&self.args[o..]).unwrap_or(Value::Null),
            None => Value::Null,
        }
    }

    /// All attributes, parsed.
    pub fn attrs(&self) -> Vec<Value> {
        let mut p = Parser { d: self.args, i: 0 };
        let mut out = Vec::new();
        loop {
            p.ws();
            if p.i >= p.d.len() {
                break;
            }
            match p.value() {
                Some(v) => out.push(v),
                None => break,
            }
            p.ws();
            if p.d.get(p.i) == Some(&b',') {
                p.i += 1;
            } else {
                break;
            }
        }
        out
    }

    /// Attribute `n` as a list of 3D points `((x,y,z),...)`, read straight from the text without
    /// building a value tree. Missing third components are 0.
    pub fn attr_points(&self, n: usize) -> Option<Vec<[f64; 3]>> {
        let o = self.attr_offset(n)?;
        let d = &self.args[o..];
        if d.first() != Some(&b'(') {
            return None;
        }
        let end = matching_paren(d, 0)?;
        let mut out = Vec::new();
        let mut i = 1;
        while i < end {
            i = skip_ws(d, i);
            match d[i] {
                b'(' => {
                    let close = matching_paren(d, i)?;
                    let mut p = [0.0; 3];
                    for (k, tok) in d[i + 1..close].split(|&b| b == b',').take(3).enumerate() {
                        p[k] = std::str::from_utf8(tok).ok()?.trim().parse().ok()?;
                    }
                    out.push(p);
                    i = close + 1;
                }
                b',' => i += 1,
                b')' => break,
                _ => return None,
            }
        }
        Some(out)
    }

    /// Attribute `n` as a flat list of integers `(1,2,3)`.
    pub fn attr_ints(&self, n: usize) -> Option<Vec<u32>> {
        let o = self.attr_offset(n)?;
        let d = &self.args[o..];
        if d.first() != Some(&b'(') {
            return None;
        }
        let end = matching_paren(d, 0)?;
        let mut out = Vec::new();
        for tok in d[1..end].split(|&b| b == b',') {
            let s = std::str::from_utf8(tok).ok()?.trim();
            if !s.is_empty() {
                out.push(s.parse().ok()?);
            }
        }
        Some(out)
    }

    /// Attribute `n` as a list of integer tuples `((a,b,c),...)`.
    pub fn attr_index_lists(&self, n: usize) -> Option<Vec<Vec<u32>>> {
        let o = self.attr_offset(n)?;
        let d = &self.args[o..];
        if d.first() != Some(&b'(') {
            return None;
        }
        let end = matching_paren(d, 0)?;
        let mut out = Vec::new();
        let mut i = 1;
        while i < end {
            i = skip_ws(d, i);
            match d[i] {
                b'(' => {
                    let close = matching_paren(d, i)?;
                    let mut t = Vec::with_capacity(3);
                    for tok in d[i + 1..close].split(|&b| b == b',') {
                        let s = std::str::from_utf8(tok).ok()?.trim();
                        if !s.is_empty() {
                            t.push(s.parse().ok()?);
                        }
                    }
                    out.push(t);
                    i = close + 1;
                }
                b',' => i += 1,
                b')' => break,
                _ => return None,
            }
        }
        Some(out)
    }
}

impl StepFile {
    /// Parses the header and indexes the DATA section.
    pub fn parse(data: Vec<u8>) -> Result<StepFile, StepError> {
        if data.len() >= u32::MAX as usize {
            return Err(StepError::TooLarge("4 GiB or more (offsets are 32 bit)".into()));
        }
        let start = skip_ws(&data, if data.starts_with(&[0xEF, 0xBB, 0xBF]) { 3 } else { 0 });
        if !data[start..].starts_with(b"ISO-10303-21") {
            return Err(StepError::NotStep("missing ISO-10303-21 start".into()));
        }
        let data_at = find(&data, start, b"DATA;").ok_or_else(|| StepError::NotStep("no DATA section".into()))?;
        let header = parse_header(&data[start..data_at]);
        let mut types: Vec<String> = Vec::new();
        let mut type_ids: HashMap<Vec<u8>, u16> = HashMap::new();
        let mut inst: Vec<Inst> = Vec::new();
        let mut complex = 0usize;
        let mut i = data_at + 5;
        let d = &data[..];
        let mut ended = false;
        loop {
            i = skip_ws(d, i);
            if i >= d.len() {
                break;
            }
            if d[i] != b'#' {
                if d[i..].len() >= 6 && d[i..i + 6].eq_ignore_ascii_case(b"ENDSEC") {
                    ended = true;
                    break;
                }
                return Err(StepError::Syntax { offset: i, message: format!("expected '#' or ENDSEC, found {:?}", d[i] as char) });
            }
            let id_start = i + 1;
            let mut e = id_start;
            while e < d.len() && d[e].is_ascii_digit() {
                e += 1;
            }
            let id: u32 = std::str::from_utf8(&d[id_start..e]).ok().and_then(|s| s.parse().ok()).ok_or(StepError::Syntax { offset: i, message: "bad instance id".into() })?;
            i = skip_ws(d, e);
            if d.get(i) != Some(&b'=') {
                return Err(StepError::Syntax { offset: i, message: "expected '='".into() });
            }
            i = skip_ws(d, i + 1);
            let (name_range, open) = if d.get(i) == Some(&b'(') {
                complex += 1;
                (None, i)
            } else {
                let s = i;
                while i < d.len() && (d[i].is_ascii_alphanumeric() || d[i] == b'_') {
                    i += 1;
                }
                if i == s {
                    return Err(StepError::Syntax { offset: i, message: "expected an entity name".into() });
                }
                let name_end = i;
                i = skip_ws(d, i);
                if d.get(i) != Some(&b'(') {
                    return Err(StepError::Syntax { offset: i, message: "expected '(' after the entity name".into() });
                }
                (Some((s, name_end)), i)
            };
            let close = matching_paren(d, open).ok_or(StepError::Syntax { offset: open, message: "unbalanced parentheses".into() })?;
            let ty = match name_range {
                Some((s, e)) => {
                    let key = d[s..e].to_ascii_uppercase();
                    match type_ids.get(&key) {
                        Some(t) => *t,
                        None => {
                            let t = types.len() as u16;
                            types.push(String::from_utf8_lossy(&key).into_owned());
                            type_ids.insert(key, t);
                            t
                        }
                    }
                }
                None => {
                    let key = b"(COMPLEX)".to_vec();
                    *type_ids.entry(key.clone()).or_insert_with(|| {
                        types.push("(COMPLEX)".into());
                        (types.len() - 1) as u16
                    })
                }
            };
            inst.push(Inst { id, ty, start: (open + 1) as u32, end: close as u32 });
            i = skip_ws(d, close + 1);
            if d.get(i) != Some(&b';') {
                return Err(StepError::Syntax { offset: i, message: "expected ';' after the instance".into() });
            }
            i += 1;
        }
        if !ended {
            return Err(StepError::NotStep("the DATA section is not closed by ENDSEC (truncated file?)".into()));
        }
        let max_id = inst.iter().map(|x| x.id).max().unwrap_or(0) as usize;
        let index = if max_id <= inst.len() * 16 + 1024 {
            let mut v = vec![NONE; max_id + 1];
            for (k, x) in inst.iter().enumerate() {
                v[x.id as usize] = k as u32;
            }
            IdIndex::Dense(v)
        } else {
            IdIndex::Sparse(inst.iter().enumerate().map(|(k, x)| (x.id, k as u32)).collect())
        };
        Ok(StepFile { data, types, inst, index, header, complex_instances: complex })
    }

    /// Reads a file from disk.
    pub fn open(path: &std::path::Path) -> Result<StepFile, StepError> {
        let data = std::fs::read(path).map_err(|e| StepError::NotStep(format!("{}: {e}", path.display())))?;
        StepFile::parse(data)
    }

    /// Size of the file buffer in bytes.
    pub fn byte_len(&self) -> usize {
        self.data.len()
    }

    /// Number of instances.
    pub fn len(&self) -> usize {
        self.inst.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inst.is_empty()
    }

    fn view(&self, x: &Inst) -> Entity<'_> {
        Entity { id: x.id, ty: &self.types[x.ty as usize], args: &self.data[x.start as usize..x.end as usize] }
    }

    /// The instance with this id.
    pub fn get(&self, id: u32) -> Option<Entity<'_>> {
        let k = match &self.index {
            IdIndex::Dense(v) => *v.get(id as usize)?,
            IdIndex::Sparse(m) => *m.get(&id)?,
        };
        (k != NONE).then(|| self.view(&self.inst[k as usize]))
    }

    /// Every instance, in file order.
    pub fn iter(&self) -> impl Iterator<Item = Entity<'_>> {
        self.inst.iter().map(|x| self.view(x))
    }

    /// Instances of one entity (upper case name), in file order.
    pub fn of_type<'a>(&'a self, name: &str) -> impl Iterator<Item = Entity<'a>> {
        let t = self.types.iter().position(|t| t == name).map(|p| p as u16);
        self.inst.iter().filter(move |x| Some(x.ty) == t).map(|x| self.view(x))
    }

    /// The distinct entity names and how many instances each has.
    pub fn type_counts(&self) -> std::collections::BTreeMap<String, usize> {
        let mut c = vec![0usize; self.types.len()];
        for x in &self.inst {
            c[x.ty as usize] += 1;
        }
        self.types.iter().cloned().zip(c).collect()
    }
}

fn parse_header(h: &[u8]) -> StepHeader {
    let mut out = StepHeader::default();
    let strings_in = |d: &[u8]| -> Vec<String> {
        let mut p = Parser { d, i: 0 };
        let mut all = Vec::new();
        fn flat(v: &Value, out: &mut Vec<String>) {
            match v {
                Value::Str(s) => out.push(s.clone()),
                Value::List(l) => l.iter().for_each(|x| flat(x, out)),
                _ => {}
            }
        }
        if let Some(v) = p.value() {
            flat(&v, &mut all);
        }
        all
    };
    if let Some(at) = find(h, 0, b"FILE_SCHEMA") {
        if let Some(open) = h[at..].iter().position(|&b| b == b'(').map(|p| p + at) {
            out.schemas = strings_in(&h[open..]);
        }
    }
    if let Some(at) = find(h, 0, b"FILE_NAME") {
        if let Some(open) = h[at..].iter().position(|&b| b == b'(').map(|p| p + at) {
            if let Some(close) = matching_paren(h, open) {
                let vals = Parser { d: &h[open..=close], i: 0 }.value();
                if let Some(Value::List(l)) = vals {
                    out.file_name = l.first().and_then(|v| v.as_str()).unwrap_or("").to_string();
                    out.originating_system = l.get(5).and_then(|v| v.as_str()).unwrap_or("").to_string();
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION(('ViewDefinition [CoordinationView]'),'2;1');\nFILE_NAME('a.ifc','2020-01-01T00:00:00',('me'),('org'),'pp','Sys 1.0','');\nFILE_SCHEMA(('IFC4'));\nENDSEC;\nDATA;\n/* comment ; with ( */\n#1=IFCPROJECT('2TaLqCNHvEn9_7cUVrypdX',#2,'It''s (here);',$,$,$,$,(#4),#5);\n#2 = IFCCARTESIANPOINT((0.,1.5E-3,-2));\n#3=IFCPROPERTYSINGLEVALUE('P',$,IFCLABEL('x'),$);\n#4=IFCDIRECTION((1.,0.,0.));\n#5=IFCSIUNIT(*,.LENGTHUNIT.,.MILLI.,.METRE.);\n#6=(IFCA()IFCB('x'));\n#7=IFCREAL(-.5);\nENDSEC;\nEND-ISO-10303-21;\n";

    #[test]
    fn header_and_instance_table() {
        let f = StepFile::parse(SAMPLE.as_bytes().to_vec()).unwrap();
        assert_eq!(f.header.schemas, vec!["IFC4"]);
        assert_eq!(f.header.file_name, "a.ifc");
        assert_eq!(f.header.originating_system, "Sys 1.0");
        assert_eq!(f.len(), 7);
        assert_eq!(f.complex_instances, 1);
        let e = f.get(1).unwrap();
        assert_eq!(e.ty, "IFCPROJECT");
        let a = e.attrs();
        assert_eq!(a.len(), 9);
        assert_eq!(a[2], Value::Str("It's (here);".into()));
        assert_eq!(a[1], Value::Ref(2));
        assert_eq!(a[7], Value::List(vec![Value::Ref(4)]));
        assert_eq!(e.attr(8), Value::Ref(5));
        assert_eq!(e.attr(20), Value::Null);
        assert_eq!(f.get(2).unwrap().attr(0), Value::List(vec![Value::Real(0.0), Value::Real(0.0015), Value::Int(-2)]));
        assert_eq!(f.get(3).unwrap().attr(2), Value::Typed("IFCLABEL".into(), Box::new(Value::Str("x".into()))));
        let u = f.get(5).unwrap().attrs();
        assert_eq!(u, vec![Value::Derived, Value::Enum("LENGTHUNIT".into()), Value::Enum("MILLI".into()), Value::Enum("METRE".into())]);
        assert_eq!(f.get(6).unwrap().ty, "(COMPLEX)");
        assert_eq!(f.get(7).unwrap().attr(0), Value::Real(-0.5));
        assert!(f.get(99).is_none());
        assert_eq!(f.type_counts()["IFCDIRECTION"], 1);
        assert_eq!(f.of_type("IFCCARTESIANPOINT").count(), 1);
    }

    #[test]
    fn points_and_index_lists_are_read_without_a_value_tree() {
        let t = "ISO-10303-21;\nHEADER;\nFILE_SCHEMA(('IFC4'));\nENDSEC;\nDATA;\n#1=IFCCARTESIANPOINTLIST3D(((0.,0.,0.),(1.5,2.,3.),(-1.E1,0.,1.)));\n#2=IFCTRIANGULATEDFACESET(#1,$,.T.,((1,2,3),(3,2,1)),$);\nENDSEC;\nEND-ISO-10303-21;\n";
        let f = StepFile::parse(t.as_bytes().to_vec()).unwrap();
        let p = f.get(1).unwrap().attr_points(0).unwrap();
        assert_eq!(p, vec![[0.0, 0.0, 0.0], [1.5, 2.0, 3.0], [-10.0, 0.0, 1.0]]);
        let i = f.get(2).unwrap().attr_index_lists(3).unwrap();
        assert_eq!(i, vec![vec![1, 2, 3], vec![3, 2, 1]]);
        assert_eq!(f.get(2).unwrap().attr(2), Value::Enum("T".into()));
    }

    #[test]
    fn string_escapes() {
        // Expected values: the results of IfcOpenShell 0.9.0.post1 reading the same strings, except
        // the surrogate pair, which it returns as an empty string.
        assert_eq!(decode_string(b"plain"), "plain");
        assert_eq!(decode_string(b"it''s"), "it's");
        assert_eq!(decode_string(b"back\\\\slash"), "back\\slash");
        assert_eq!(decode_string(b"caf\\X\\E9"), "caf\u{e9}");
        assert_eq!(decode_string(b"\\X2\\00E900FC\\X0\\x"), "\u{e9}\u{fc}x");
        assert_eq!(decode_string(b"\\X2\\D83DDE00\\X0\\"), "\u{1F600}");
        assert_eq!(decode_string(b"\\X4\\0001F600\\X0\\"), "\u{1F600}");
        assert_eq!(decode_string(b"\\S\\i"), "\u{e9}");
        assert_eq!(decode_string("raw \u{e9}".as_bytes()), "raw \u{e9}");
    }

    #[test]
    fn errors_are_reported_not_panicked() {
        assert!(matches!(StepFile::parse(b"hello".to_vec()), Err(StepError::NotStep(_))));
        assert!(matches!(StepFile::parse(b"ISO-10303-21;\nHEADER;ENDSEC;\n".to_vec()), Err(StepError::NotStep(_))));
        let bad = "ISO-10303-21;\nHEADER;\nENDSEC;\nDATA;\n#1=IFCX((1,2);\nENDSEC;\n";
        assert!(matches!(StepFile::parse(bad.as_bytes().to_vec()), Err(StepError::Syntax { .. })));
        let bad = "ISO-10303-21;\nHEADER;\nENDSEC;\nDATA;\n#1=IFCX(1)\n#2=IFCX(2);\nENDSEC;\n";
        assert!(StepFile::parse(bad.as_bytes().to_vec()).is_err());
    }

    #[test]
    fn sparse_ids_use_the_hash_index() {
        let t = "ISO-10303-21;\nHEADER;\nENDSEC;\nDATA;\n#1=IFCA(1);\n#4000000000=IFCB(2);\nENDSEC;\n";
        let f = StepFile::parse(t.as_bytes().to_vec()).unwrap();
        assert_eq!(f.get(4_000_000_000).unwrap().ty, "IFCB");
        assert_eq!(f.get(1).unwrap().attr(0), Value::Int(1));
    }
}
