//! Exact decimal arithmetic for money, quantities and rates.
//!
//! Amounts are never `f64`. A binary float cannot hold 0.1 or 1250.35 exactly, and a bill adds
//! thousands of such numbers; the sum of rounded lines would drift from the rounded sum and a
//! total would not agree with its own lines. [`Decimal`] is a signed integer count of millionths
//! (6 decimals) held in an `i128`, so every sum is exact and every rounding is a stated
//! decision ([`Rounding`]). Six decimals is enough for a quantity (3 are normal), a rate, an
//! exchange rate and a percentage (4 are normal), and an `i128` holds amounts far beyond any
//! national budget.
//!
//! A product such as quantity times rate is computed exactly (up to 12 decimals) and rounded
//! once, to the places and by the mode the caller names. Nothing here rounds silently except
//! [`Decimal::from_f64`], which is the single point where a computed `f64` quantity from another
//! crate enters, and which states the number of decimals it keeps.

use std::fmt;
use std::iter::Sum;
use std::ops::{Add, AddAssign, Neg, Sub, SubAssign};

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{CostError, Result};

/// Decimals a [`Decimal`] holds.
pub const DECIMALS: u32 = 6;
const SCALE: i128 = 1_000_000;

/// How a value is brought to a number of decimals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum RoundingMode {
    /// Halves go away from zero: 2.5 gives 3 and -2.5 gives -3. The usual rule on a bill.
    #[default]
    HalfAwayFromZero,
    /// Halves go to the even neighbour: 2.5 gives 2 and 3.5 gives 4.
    HalfEven,
    /// Any remainder goes away from zero: 2.1 gives 3. Used to round quantities up, e.g. to whole
    /// units of supply.
    AwayFromZero,
    /// The remainder is cut off: 2.9 gives 2.
    TowardZero,
}

/// A number of decimals and a mode: one explicit rounding rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rounding {
    pub decimals: u32,
    #[serde(default)]
    pub mode: RoundingMode,
}

impl Rounding {
    pub fn new(decimals: u32, mode: RoundingMode) -> Rounding {
        Rounding { decimals: decimals.min(DECIMALS), mode }
    }

    /// Half away from zero at `decimals`.
    pub fn half_up(decimals: u32) -> Rounding {
        Rounding::new(decimals, RoundingMode::HalfAwayFromZero)
    }

    fn step(&self) -> i128 {
        10i128.pow(DECIMALS - self.decimals.min(DECIMALS))
    }
}

/// Signed decimal with 6 decimals. See the module documentation.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Decimal(i128);

fn round_div(n: i128, d: i128, mode: RoundingMode) -> i128 {
    debug_assert!(d != 0);
    let (n, d) = if d < 0 { (-n, -d) } else { (n, d) };
    let neg = n < 0;
    let a = n.abs();
    let (q, r) = (a / d, a % d);
    let twice = r * 2;
    let up = match mode {
        RoundingMode::HalfAwayFromZero => twice >= d,
        RoundingMode::HalfEven => twice > d || (twice == d && q % 2 == 1),
        RoundingMode::AwayFromZero => r > 0,
        RoundingMode::TowardZero => false,
    };
    let q = if up { q + 1 } else { q };
    if neg {
        -q
    } else {
        q
    }
}

impl Decimal {
    pub const ZERO: Decimal = Decimal(0);
    pub const ONE: Decimal = Decimal(SCALE);

    /// From a count of millionths: `from_units(1_500_000)` is 1.5.
    pub fn from_units(units: i128) -> Decimal {
        Decimal(units)
    }

    pub fn units(self) -> i128 {
        self.0
    }

    pub fn from_int(v: i64) -> Decimal {
        Decimal(v as i128 * SCALE)
    }

    /// Parses `-12.5`, `+3`, `.25`, `7.`; no thousands separators, no exponent. More than 6
    /// decimals is an error unless the extra ones are zeros (an entered rate is never altered).
    pub fn parse(text: &str) -> Result<Decimal> {
        let bad = || CostError::Number(text.to_string());
        let t = text.trim();
        let (neg, body) = match t.strip_prefix('-') {
            Some(r) => (true, r),
            None => (false, t.strip_prefix('+').unwrap_or(t)),
        };
        if body.is_empty() {
            return Err(bad());
        }
        let (int_s, frac_s) = body.split_once('.').unwrap_or((body, ""));
        if int_s.is_empty() && frac_s.is_empty() {
            return Err(bad());
        }
        if !int_s.bytes().all(|b| b.is_ascii_digit()) || !frac_s.bytes().all(|b| b.is_ascii_digit()) {
            return Err(bad());
        }
        let (kept, extra) = if frac_s.len() > DECIMALS as usize { frac_s.split_at(DECIMALS as usize) } else { (frac_s, "") };
        if extra.bytes().any(|b| b != b'0') {
            return Err(bad());
        }
        let int: i128 = if int_s.is_empty() { 0 } else { int_s.parse().map_err(|_| bad())? };
        let mut frac: i128 = if kept.is_empty() { 0 } else { kept.parse().map_err(|_| bad())? };
        frac *= 10i128.pow(DECIMALS - kept.len() as u32);
        let v = int.checked_mul(SCALE).and_then(|v| v.checked_add(frac)).ok_or_else(bad)?;
        Ok(Decimal(if neg { -v } else { v }))
    }

    /// A computed `f64` taken to `decimals` places (half to even on the exact binary value, as
    /// Rust's formatter does). The one place a float enters this crate.
    pub fn from_f64(v: f64, decimals: u32) -> Result<Decimal> {
        if !v.is_finite() {
            return Err(CostError::Invalid(format!("{v} is not a finite quantity")));
        }
        Decimal::parse(&format!("{:.*}", decimals.min(DECIMALS) as usize, v))
    }

    pub fn to_f64(self) -> f64 {
        self.0 as f64 / SCALE as f64
    }

    pub fn is_zero(self) -> bool {
        self.0 == 0
    }

    pub fn is_negative(self) -> bool {
        self.0 < 0
    }

    pub fn abs(self) -> Decimal {
        Decimal(self.0.abs())
    }

    pub fn min(self, other: Decimal) -> Decimal {
        if self <= other {
            self
        } else {
            other
        }
    }

    pub fn max(self, other: Decimal) -> Decimal {
        if self >= other {
            self
        } else {
            other
        }
    }

    /// Rounded to the rule.
    pub fn round(self, r: Rounding) -> Decimal {
        let step = r.step();
        Decimal(round_div(self.0, step, r.mode) * step)
    }

    /// `self * other`, computed exactly and rounded once.
    pub fn mul_rounded(self, other: Decimal, r: Rounding) -> Decimal {
        let step = r.step();
        Decimal(round_div(self.0 * other.0, SCALE * step, r.mode) * step)
    }

    /// `self * percent / 100`, computed exactly and rounded once.
    pub fn percent_of(self, percent: Decimal, r: Rounding) -> Decimal {
        let step = r.step();
        Decimal(round_div(self.0 * percent.0, SCALE * 100 * step, r.mode) * step)
    }

    /// `self * num / den` for whole `num` and positive whole `den`, rounded once.
    pub fn mul_ratio(self, num: i128, den: i128, r: Rounding) -> Decimal {
        debug_assert!(den > 0);
        let step = r.step();
        Decimal(round_div(self.0 * num, den * step, r.mode) * step)
    }

    /// `self / other` rounded once; None when `other` is zero.
    pub fn div_rounded(self, other: Decimal, r: Rounding) -> Option<Decimal> {
        if other.0 == 0 {
            return None;
        }
        let step = r.step();
        Some(Decimal(round_div(self.0 * SCALE, other.0 * step, r.mode) * step))
    }

    /// Exactly `decimals` places, rounded half away from zero: `1234.5` at 2 gives `1234.50`.
    /// For output; [`Decimal`]'s `Display` gives the shortest exact text.
    pub fn to_fixed(self, decimals: u32) -> String {
        let d = decimals.min(DECIMALS);
        let v = self.round(Rounding::half_up(d));
        let neg = v.0 < 0;
        let a = v.0.abs();
        let (int, frac) = (a / SCALE, a % SCALE);
        let mut s = String::new();
        if neg {
            s.push('-');
        }
        s.push_str(&int.to_string());
        if d > 0 {
            let f = format!("{frac:06}");
            s.push('.');
            s.push_str(&f[..d as usize]);
        }
        if neg && v.0 == 0 {
            s.remove(0);
        }
        s
    }
}

impl fmt::Display for Decimal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let full = self.to_fixed(DECIMALS);
        let t = if full.contains('.') { full.trim_end_matches('0').trim_end_matches('.') } else { &full };
        f.write_str(t)
    }
}

impl fmt::Debug for Decimal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self}")
    }
}

impl Add for Decimal {
    type Output = Decimal;
    fn add(self, o: Decimal) -> Decimal {
        Decimal(self.0 + o.0)
    }
}

impl Sub for Decimal {
    type Output = Decimal;
    fn sub(self, o: Decimal) -> Decimal {
        Decimal(self.0 - o.0)
    }
}

impl Neg for Decimal {
    type Output = Decimal;
    fn neg(self) -> Decimal {
        Decimal(-self.0)
    }
}

impl AddAssign for Decimal {
    fn add_assign(&mut self, o: Decimal) {
        self.0 += o.0;
    }
}

impl SubAssign for Decimal {
    fn sub_assign(&mut self, o: Decimal) {
        self.0 -= o.0;
    }
}

impl Sum for Decimal {
    fn sum<I: Iterator<Item = Decimal>>(iter: I) -> Decimal {
        iter.fold(Decimal::ZERO, |a, b| a + b)
    }
}

impl<'a> Sum<&'a Decimal> for Decimal {
    fn sum<I: Iterator<Item = &'a Decimal>>(iter: I) -> Decimal {
        iter.fold(Decimal::ZERO, |a, b| a + *b)
    }
}

impl Serialize for Decimal {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Decimal {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Decimal, D::Error> {
        struct V;
        impl Visitor<'_> for V {
            type Value = Decimal;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a decimal number as text or a number")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> std::result::Result<Decimal, E> {
                Decimal::parse(v).map_err(E::custom)
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> std::result::Result<Decimal, E> {
                Ok(Decimal::from_int(v))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> std::result::Result<Decimal, E> {
                Ok(Decimal(v as i128 * SCALE))
            }
            fn visit_f64<E: de::Error>(self, v: f64) -> std::result::Result<Decimal, E> {
                Decimal::from_f64(v, DECIMALS).map_err(E::custom)
            }
        }
        d.deserialize_any(V)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> Decimal {
        Decimal::parse(s).unwrap()
    }

    #[test]
    fn parse_and_print() {
        assert_eq!(d("12.5").to_string(), "12.5");
        assert_eq!(d("-0.000001").to_string(), "-0.000001");
        assert_eq!(d(".25").to_string(), "0.25");
        assert_eq!(d("7.").to_string(), "7");
        assert_eq!(d("1.5000000").to_string(), "1.5");
        assert!(Decimal::parse("1.0000001").is_err());
        assert!(Decimal::parse("1,000").is_err());
        assert!(Decimal::parse("").is_err());
        assert!(Decimal::parse("-").is_err());
        assert_eq!(d("1234.5").to_fixed(2), "1234.50");
        assert_eq!(d("-0.004").to_fixed(2), "0.00");
        assert_eq!(d("2.675").to_fixed(2), "2.68");
    }

    #[test]
    fn rounding_modes_by_hand() {
        let r = |mode| Rounding::new(0, mode);
        assert_eq!(d("2.5").round(r(RoundingMode::HalfAwayFromZero)), d("3"));
        assert_eq!(d("-2.5").round(r(RoundingMode::HalfAwayFromZero)), d("-3"));
        assert_eq!(d("2.5").round(r(RoundingMode::HalfEven)), d("2"));
        assert_eq!(d("3.5").round(r(RoundingMode::HalfEven)), d("4"));
        assert_eq!(d("2.1").round(r(RoundingMode::AwayFromZero)), d("3"));
        assert_eq!(d("-2.1").round(r(RoundingMode::AwayFromZero)), d("-3"));
        assert_eq!(d("2.9").round(r(RoundingMode::TowardZero)), d("2"));
        assert_eq!(d("2.0").round(r(RoundingMode::AwayFromZero)), d("2"));
    }

    #[test]
    fn product_is_exact_then_rounded_once() {
        // 12.345 x 1250.35 = 15435.57075 exactly; to 2 decimals 15435.57.
        let p = d("12.345").mul_rounded(d("1250.35"), Rounding::half_up(2));
        assert_eq!(p, d("15435.57"));
        // 15% of 200.05 = 30.0075.
        assert_eq!(d("200.05").percent_of(d("15"), Rounding::half_up(2)), d("30.01"));
        assert_eq!(d("1000").mul_ratio(3, 7, Rounding::half_up(2)), d("428.57"));
        assert_eq!(d("1").div_rounded(d("3700"), Rounding::half_up(6)), Some(d("0.000270")));
        assert_eq!(d("1").div_rounded(Decimal::ZERO, Rounding::half_up(2)), None);
    }

    #[test]
    fn floats_enter_at_a_stated_precision() {
        assert_eq!(Decimal::from_f64(0.1 + 0.2, 6).unwrap(), d("0.3"));
        assert_eq!(Decimal::from_f64(12.3456789, 3).unwrap(), d("12.346"));
        assert!(Decimal::from_f64(f64::NAN, 3).is_err());
    }

    #[test]
    fn serde_round_trip() {
        let v = d("-1250.35");
        let json = serde_json::to_string(&v).unwrap();
        assert_eq!(json, "\"-1250.35\"");
        assert_eq!(serde_json::from_str::<Decimal>(&json).unwrap(), v);
        assert_eq!(serde_json::from_str::<Decimal>("12").unwrap(), d("12"));
        assert_eq!(serde_json::from_str::<Decimal>("1.5").unwrap(), d("1.5"));
    }
}
