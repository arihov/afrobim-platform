//! ISO 10303-21 (STEP physical file) encoding: strings, reals and the instance list.

/// A string as a STEP literal, quotes included.
///
/// Apostrophes and backslashes are doubled; printable ASCII is written as is; other characters in
/// the Basic Multilingual Plane become `\X2\hhhh\X0\`, characters beyond it `\X4\hhhhhhhh\X0\`,
/// and control characters `\X\hh` (ISO 10303-21, clause 6.4.3).
pub fn step_string(text: &str) -> String {
    #[derive(PartialEq, Clone, Copy)]
    enum Run {
        None,
        X2,
        X4,
    }
    let mut out = String::with_capacity(text.len() + 2);
    let mut run = Run::None;
    out.push('\'');
    for ch in text.chars() {
        let code = ch as u32;
        let wanted = match code {
            0x00..=0x7F => Run::None,
            0x80..=0xFFFF => Run::X2,
            _ => Run::X4,
        };
        if wanted != run {
            if run != Run::None {
                out.push_str("\\X0\\");
            }
            match wanted {
                Run::X2 => out.push_str("\\X2\\"),
                Run::X4 => out.push_str("\\X4\\"),
                Run::None => {}
            }
            run = wanted;
        }
        match wanted {
            Run::X2 => out.push_str(&format!("{code:04X}")),
            Run::X4 => out.push_str(&format!("{code:08X}")),
            Run::None => match ch {
                '\'' => out.push_str("''"),
                '\\' => out.push_str("\\\\"),
                _ if (0x20..=0x7E).contains(&code) => out.push(ch),
                _ => out.push_str(&format!("\\X\\{code:02X}")),
            },
        }
    }
    if run != Run::None {
        out.push_str("\\X0\\");
    }
    out.push('\'');
    out
}

/// A real as a STEP literal: always with a decimal point, never `NaN` or infinity (those are
/// written as `0.`), rounded to a micro-unit so that noise in the last bits does not reach the file.
pub fn step_real(value: f64) -> String {
    if !value.is_finite() {
        return "0.".to_string();
    }
    let rounded = (value * 1e6).round() / 1e6;
    let rounded = if rounded == 0.0 { 0.0 } else { rounded }; // no "-0."
    let mut text = format!("{rounded}");
    if !text.contains('.') {
        text.push('.');
    }
    text
}

/// `(a,b,c)` from already encoded parts.
pub(crate) fn list<I: IntoIterator<Item = String>>(parts: I) -> String {
    let mut out = String::from("(");
    for (i, part) in parts.into_iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&part);
    }
    out.push(')');
    out
}

/// `(1.,2.,3.)`.
pub(crate) fn reals(values: &[f64]) -> String {
    list(values.iter().map(|v| step_real(*v)))
}

/// `(#1,#2)`.
pub(crate) fn refs(ids: &[u32]) -> String {
    list(ids.iter().map(|id| format!("#{id}")))
}

/// An optional string: `$` when absent or empty.
pub(crate) fn opt_string(text: Option<&str>) -> String {
    match text {
        Some(t) if !t.is_empty() => step_string(t),
        _ => "$".to_string(),
    }
}

/// The DATA section under construction. Instance ids are handed out sequentially from 1.
#[derive(Default)]
pub(crate) struct Instances {
    lines: Vec<String>,
}

impl Instances {
    /// Adds `#n=TYPE(args);` and returns `n`.
    pub fn add(&mut self, entity: &str, args: &str) -> u32 {
        let id = self.lines.len() as u32 + 1;
        self.lines.push(format!("#{id}={entity}({args});"));
        id
    }

    pub fn lines(&self) -> &[String] {
        &self.lines
    }
}

/// Seconds since the Unix epoch as `YYYY-MM-DDThh:mm:ss` (UTC), the form FILE_NAME asks for.
pub(crate) fn iso_timestamp(seconds: i64) -> String {
    let days = seconds.div_euclid(86_400);
    let rem = seconds.rem_euclid(86_400);
    // Civil date from a day count (proleptic Gregorian calendar).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}", rem / 3600, rem % 3600 / 60, rem % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_follow_the_step_rules() {
        assert_eq!(step_string("ADS-001"), "'ADS-001'");
        assert_eq!(step_string("O'Brien"), "'O''Brien'");
        assert_eq!(step_string("a\\b"), "'a\\\\b'");
        assert_eq!(step_string("Zürich"), "'Z\\X2\\00FC\\X0\\rich'");
        assert_eq!(step_string("日本"), "'\\X2\\65E5672C\\X0\\'");
        assert_eq!(step_string("a😀b"), "'a\\X4\\0001F600\\X0\\b'");
        assert_eq!(step_string("é😀"), "'\\X2\\00E9\\X0\\\\X4\\0001F600\\X0\\'");
        assert_eq!(step_string("a\nb"), "'a\\X\\0Ab'");
        assert_eq!(step_string(""), "''");
    }

    #[test]
    fn reals_always_carry_a_decimal_point() {
        assert_eq!(step_real(3.0), "3.");
        assert_eq!(step_real(-0.1), "-0.1");
        assert_eq!(step_real(0.30000000000000004), "0.3");
        assert_eq!(step_real(-0.0), "0.");
        assert_eq!(step_real(-1e-9), "0.");
        assert_eq!(step_real(1e-5), "0.00001");
        assert_eq!(step_real(12345678.5), "12345678.5");
        assert_eq!(step_real(f64::NAN), "0.");
    }

    #[test]
    fn timestamps_are_utc_iso_8601() {
        assert_eq!(iso_timestamp(0), "1970-01-01T00:00:00");
        assert_eq!(iso_timestamp(1_794_528_000), "2026-11-13T00:00:00");
        assert_eq!(iso_timestamp(951_782_399), "2000-02-28T23:59:59");
        assert_eq!(iso_timestamp(951_868_800), "2000-03-01T00:00:00");
    }
}
