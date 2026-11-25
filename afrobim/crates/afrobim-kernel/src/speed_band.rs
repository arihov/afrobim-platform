// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The speed band rule: how a posted speed limit is compared with a standard's thresholds.
//!
//! UK requirements are written in mph bands ("50 mph or more"). Uganda posts km/h limits, and
//! 80 km/h is 49.71 mph, which is not 50. A project therefore needs one stated rule for how a
//! limit in the other unit meets a threshold. The rule is a document-level project setting
//! ([`speed_band_rule`], [`set_speed_band_rule`]); [`resolve`] applies it for one check.
//!
//! The thresholds are never built in here. Each engine check knows its own bands and passes
//! them in, in mph. This module only converts and decides.
//!
//! # Conversion
//!
//! 1 international mile = 1.609344 km exactly. That is a definition, not a measurement
//! ([`KM_PER_MILE`]). So mph = km/h / 1.609344.
//!
//! # RoundUp (the project default)
//!
//! Let `v` be the exact converted value in mph and `t` the smallest threshold with `t >= v`.
//!
//! * If `v == t` the value is used as it is.
//! * If `v < t` and `t - v` is within the tolerance, `t` is used, so the requirement of the
//!   band at `t` applies. The tolerance says how close "just under" means:
//!   * [`RoundUpTolerance::ConversionRounding`] (the default): `t - v <= 0.5` mph, which is
//!     the case where `v` rounds to `t` at the nearest whole mph (halves round up). So
//!     80 km/h (49.71 mph) rounds to 50, and 70 km/h (43.50 mph) does not.
//!   * [`RoundUpTolerance::Mph`]`(x)`: `t - v <= x` mph, a wider band chosen by the user.
//!   * [`RoundUpTolerance::NextBandUp`]: any `v < t` uses `t`: always the next band up.
//! * Otherwise (no such `t`, or `t` out of reach) the exact value `v` is used.
//!
//! `ConversionRounding` is about conversion, so it applies only to a limit given in km/h. A
//! limit given in mph is already in the standard's unit and is used exactly. The wider
//! tolerances apply to either unit because the user chose them deliberately.
//!
//! The other rules: [`SpeedBandRule::Exact`] always uses `v`. [`SpeedBandRule::Table`] looks
//! the km/h limit up in a user table of (km/h limit, mph value to use) and falls back to the
//! exact value, saying so, when the limit is not listed or is given in mph.
//! [`SpeedBandRule::AskEachTime`] returns the exact value and, when the default rule would have
//! rounded up, an [`Ask`] for the caller to put to the user; [`Resolved::answered`] settles it.
//!
//! ```
//! use afrobim_kernel::speed_band::{resolve, SpeedBandRule, SpeedUnit};
//! let r = resolve(80.0, SpeedUnit::Kmh, &[30.0, 40.0, 50.0, 60.0], &SpeedBandRule::default());
//! assert_eq!(r.mph_used, 50.0);
//! assert_eq!(r.band_note, "80 km/h = 49.71 mph; project rule 'round up to the next band' treats it as 50 mph");
//! ```

use serde::{Deserialize, Serialize};

use crate::{project_setting, set_project_setting, Document};

/// Kilometres in one international mile. Exact by definition: 1 mile = 1.609344 km.
pub const KM_PER_MILE: f64 = 1.609344;

/// The key of the rule in the document's project settings.
pub const SPEED_BAND_RULE_KEY: &str = "speed_band_rule";

/// Comparison slack for "equal" values, mph. Far below any real speed difference.
const EPS: f64 = 1e-9;

/// Converts km/h to mph.
pub fn kmh_to_mph(kmh: f64) -> f64 {
    kmh / KM_PER_MILE
}

/// Converts mph to km/h.
pub fn mph_to_kmh(mph: f64) -> f64 {
    mph * KM_PER_MILE
}

/// The unit a speed limit is stated in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpeedUnit {
    Kmh,
    Mph,
}

/// How close below a threshold counts as "just under" for [`SpeedBandRule::RoundUp`].
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub enum RoundUpTolerance {
    /// The value rounds to the threshold at the nearest whole mph: `t - v <= 0.5`. Applies to
    /// limits converted from km/h only. The documented default.
    #[default]
    ConversionRounding,
    /// `t - v <= x` mph, for a limit in either unit.
    Mph(f64),
    /// Any value below a threshold uses it: always the next band up.
    NextBandUp,
}

/// The project's rule for meeting a mph threshold with a limit that may be in km/h.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SpeedBandRule {
    /// Round up to the next band (see the module docs). The project default.
    RoundUp { tolerance: RoundUpTolerance },
    /// Use the exact converted value.
    Exact,
    /// Do not decide: the caller asks the user each time the answer matters.
    AskEachTime,
    /// A user table of (km/h limit, mph value to use).
    Table(Vec<(f64, f64)>),
}

impl Default for SpeedBandRule {
    fn default() -> Self {
        SpeedBandRule::RoundUp { tolerance: RoundUpTolerance::ConversionRounding }
    }
}

/// How the value was arrived at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Basis {
    /// The exact value (converted if need be).
    Exact,
    /// Rounded up to a threshold by the round-up rule, or by the user's answer.
    RoundedUp,
    /// Taken from the user's table.
    Table,
    /// A table rule with no usable entry for the limit: the exact value was used.
    TableMiss,
    /// The rule is "ask each time" and the answer matters: the exact value is returned as a
    /// placeholder and [`Resolved::ask`] is set.
    NeedsAnswer,
}

/// The question an `AskEachTime` rule leaves to the caller.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Ask {
    /// The exact converted value, mph.
    pub exact_mph: f64,
    /// The threshold it would round up to, mph.
    pub band_mph: f64,
}

/// What [`resolve`] returns.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Resolved {
    /// The mph value to compare against the thresholds.
    pub mph_used: f64,
    /// The exact converted value, mph.
    pub exact_mph: f64,
    /// A sentence for the calculation sheet.
    pub band_note: String,
    pub basis: Basis,
    /// Set when the caller must ask the user. See [`Resolved::answered`].
    pub ask: Option<Ask>,
}

impl Resolved {
    /// Settles an [`Ask`]: `use_band` true takes the threshold, false the exact value. A result
    /// with no question is returned unchanged.
    pub fn answered(mut self, use_band: bool) -> Resolved {
        let Some(ask) = self.ask.take() else { return self };
        let head = self.band_note.split(';').next().unwrap_or("").to_string();
        if use_band {
            self.mph_used = ask.band_mph;
            self.basis = Basis::RoundedUp;
            self.band_note = format!("{head}; the user chose to treat it as {} mph", num(ask.band_mph));
        } else {
            self.mph_used = ask.exact_mph;
            self.basis = Basis::Exact;
            self.band_note = format!("{head}; the user chose the exact value, {:.2} mph", ask.exact_mph);
        }
        self
    }
}

/// Formats a whole value without a decimal part, anything else to two places.
fn num(v: f64) -> String {
    if (v - v.round()).abs() < EPS {
        format!("{}", v.round() as i64)
    } else {
        format!("{v:.2}")
    }
}

/// Resolves one limit against the thresholds of one check.
///
/// `thresholds_mph` is the ascending list of mph thresholds the calling engine's check uses; an
/// unsorted list is sorted rather than trusted. See the module docs for the exact definition of
/// each rule.
pub fn resolve(limit: f64, unit: SpeedUnit, thresholds_mph: &[f64], rule: &SpeedBandRule) -> Resolved {
    let mut thresholds: Vec<f64> = thresholds_mph.iter().copied().filter(|t| t.is_finite()).collect();
    thresholds.sort_by(|a, b| a.total_cmp(b));
    let (exact, given) = match unit {
        SpeedUnit::Kmh => (kmh_to_mph(limit), format!("{} km/h = {:.2} mph", num(limit), kmh_to_mph(limit))),
        SpeedUnit::Mph => (limit, format!("{} mph (given in mph, no conversion)", num(limit))),
    };
    let make = |mph_used: f64, basis: Basis, tail: String, ask: Option<Ask>| Resolved {
        mph_used,
        exact_mph: exact,
        band_note: format!("{given}; {tail}"),
        basis,
        ask,
    };
    let exact_note = format!("the exact value, {exact:.2} mph, is used");
    // The smallest threshold at or above the exact value.
    let above = thresholds.iter().copied().find(|t| *t >= exact - EPS);

    match rule {
        SpeedBandRule::Exact => make(exact, Basis::Exact, format!("project rule 'exact conversion': {exact_note}"), None),
        SpeedBandRule::Table(table) => {
            let entry = match unit {
                SpeedUnit::Kmh => table.iter().find(|(l, _)| (l - limit).abs() < 1e-6),
                SpeedUnit::Mph => None,
            };
            match entry {
                Some((_, mph)) => make(*mph, Basis::Table, format!("project table gives {} mph for this limit", num(*mph)), None),
                None if unit == SpeedUnit::Mph => {
                    make(exact, Basis::TableMiss, format!("the project table is in km/h and does not apply; {exact_note}"), None)
                }
                None => make(exact, Basis::TableMiss, format!("project table has no entry for this limit; {exact_note}"), None),
            }
        }
        SpeedBandRule::RoundUp { tolerance } => {
            let reaches = |t: f64| {
                let gap = t - exact;
                match tolerance {
                    RoundUpTolerance::ConversionRounding => unit == SpeedUnit::Kmh && gap <= 0.5 + EPS,
                    RoundUpTolerance::Mph(x) => gap <= *x + EPS,
                    RoundUpTolerance::NextBandUp => true,
                }
            };
            match above {
                Some(t) if (t - exact).abs() <= EPS => {
                    make(t, Basis::Exact, format!("this is the {} mph threshold exactly", num(t)), None)
                }
                Some(t) if reaches(t) => {
                    make(t, Basis::RoundedUp, format!("project rule 'round up to the next band' treats it as {} mph", num(t)), None)
                }
                _ => make(
                    exact,
                    Basis::Exact,
                    format!("project rule 'round up to the next band': no threshold is within reach above it, {exact_note}"),
                    None,
                ),
            }
        }
        SpeedBandRule::AskEachTime => {
            // The question matters only where the default rule would round up.
            let by_default = resolve(limit, unit, &thresholds, &SpeedBandRule::default());
            if by_default.basis == Basis::RoundedUp {
                let ask = Ask { exact_mph: exact, band_mph: by_default.mph_used };
                make(
                    exact,
                    Basis::NeedsAnswer,
                    format!(
                        "project rule 'ask each time': is it to be treated as {} mph or kept at {exact:.2} mph?",
                        num(by_default.mph_used)
                    ),
                    Some(ask),
                )
            } else {
                make(exact, Basis::Exact, format!("project rule 'ask each time': no threshold is within reach, {exact_note}"), None)
            }
        }
    }
}

/// The document's speed band rule; [`SpeedBandRule::default`] (round up) when none is stored or
/// the stored value cannot be read.
pub fn speed_band_rule(doc: &Document) -> SpeedBandRule {
    project_setting(doc, SPEED_BAND_RULE_KEY).and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default()
}

/// Stores the speed band rule in the document's project settings.
pub fn set_speed_band_rule(doc: &mut Document, rule: &SpeedBandRule) {
    set_project_setting(doc, SPEED_BAND_RULE_KEY, serde_json::to_value(rule).expect("a rule always serialises"));
}

#[cfg(test)]
mod tests {
    use super::*;

    const UK: [f64; 4] = [30.0, 40.0, 50.0, 60.0];
    fn up() -> SpeedBandRule {
        SpeedBandRule::default()
    }

    // Hand work: 80 / 1.609344 = 49.7096, 100 / 1.609344 = 62.1371, 50 / 1.609344 = 31.0686,
    // 60 / 1.609344 = 37.2823, 70 / 1.609344 = 43.4960.
    #[test]
    fn conversions() {
        assert!((kmh_to_mph(80.0) - 49.7096).abs() < 1e-4);
        assert!((kmh_to_mph(100.0) - 62.1371).abs() < 1e-4);
        assert!((kmh_to_mph(50.0) - 31.0686).abs() < 1e-4);
        assert!((kmh_to_mph(60.0) - 37.2823).abs() < 1e-4);
        assert!((kmh_to_mph(70.0) - 43.4960).abs() < 1e-4);
        assert!((mph_to_kmh(1.0) - 1.609344).abs() < 1e-12);
    }

    #[test]
    fn eighty_kmh_rounds_up_to_fifty() {
        let r = resolve(80.0, SpeedUnit::Kmh, &UK, &up());
        assert_eq!(r.mph_used, 50.0);
        assert_eq!(r.basis, Basis::RoundedUp);
        assert_eq!(r.band_note, "80 km/h = 49.71 mph; project rule 'round up to the next band' treats it as 50 mph");
    }

    #[test]
    fn seventy_kmh_does_not_round_to_fifty() {
        // 43.50 mph is 6.5 mph below 50: not within 0.5.
        let r = resolve(70.0, SpeedUnit::Kmh, &UK, &up());
        assert!((r.mph_used - 43.4960).abs() < 1e-4);
        assert_eq!(r.basis, Basis::Exact);
        assert!(r.band_note.starts_with("70 km/h = 43.50 mph;"), "{}", r.band_note);
    }

    #[test]
    fn one_hundred_kmh_is_above_every_threshold() {
        let r = resolve(100.0, SpeedUnit::Kmh, &UK, &up());
        assert!((r.mph_used - 62.1371).abs() < 1e-4);
        assert_eq!(r.basis, Basis::Exact);
        assert!(r.band_note.starts_with("100 km/h = 62.14 mph;"));
    }

    #[test]
    fn fifty_and_sixty_kmh() {
        // 31.07 is 8.93 below 40; 37.28 is 2.72 below 40: neither is within 0.5.
        let a = resolve(50.0, SpeedUnit::Kmh, &UK, &up());
        assert_eq!(a.basis, Basis::Exact);
        assert!((a.mph_used - 31.0686).abs() < 1e-4);
        let b = resolve(60.0, SpeedUnit::Kmh, &UK, &up());
        assert_eq!(b.basis, Basis::Exact);
        assert!((b.mph_used - 37.2823).abs() < 1e-4);
    }

    #[test]
    fn boundary_of_conversion_rounding() {
        // 49.5 mph is exactly half a mph below 50: rounds up (halves round up).
        assert_eq!(resolve(mph_to_kmh(49.5), SpeedUnit::Kmh, &UK, &up()).mph_used, 50.0);
        // 49.4 mph is not.
        let r = resolve(mph_to_kmh(49.4), SpeedUnit::Kmh, &UK, &up());
        assert!((r.mph_used - 49.4).abs() < 1e-9);
    }

    #[test]
    fn a_limit_in_mph_is_used_exactly_by_the_default() {
        let r = resolve(50.0, SpeedUnit::Mph, &UK, &up());
        assert_eq!(r.mph_used, 50.0);
        assert_eq!(r.band_note, "50 mph (given in mph, no conversion); this is the 50 mph threshold exactly");
        // 49.7 mph typed in mph is not a conversion, so conversion rounding does not apply.
        assert_eq!(resolve(49.7, SpeedUnit::Mph, &UK, &up()).mph_used, 49.7);
    }

    #[test]
    fn wider_tolerances() {
        let two = SpeedBandRule::RoundUp { tolerance: RoundUpTolerance::Mph(2.0) };
        // 43.50 is 6.5 below 50: out of reach. 48.5 mph is 1.5 below: in reach.
        assert_eq!(resolve(70.0, SpeedUnit::Kmh, &UK, &two).basis, Basis::Exact);
        assert_eq!(resolve(48.5, SpeedUnit::Mph, &UK, &two).mph_used, 50.0);
        let always = SpeedBandRule::RoundUp { tolerance: RoundUpTolerance::NextBandUp };
        assert_eq!(resolve(70.0, SpeedUnit::Kmh, &UK, &always).mph_used, 50.0);
        // 62.14 mph has no threshold above it.
        assert_eq!(resolve(100.0, SpeedUnit::Kmh, &UK, &always).basis, Basis::Exact);
    }

    #[test]
    fn exact_rule() {
        let r = resolve(80.0, SpeedUnit::Kmh, &UK, &SpeedBandRule::Exact);
        assert!((r.mph_used - 49.7096).abs() < 1e-4);
        assert_eq!(r.band_note, "80 km/h = 49.71 mph; project rule 'exact conversion': the exact value, 49.71 mph, is used");
    }

    #[test]
    fn table_rule() {
        let t = SpeedBandRule::Table(vec![(80.0, 50.0), (100.0, 60.0)]);
        let r = resolve(80.0, SpeedUnit::Kmh, &UK, &t);
        assert_eq!((r.mph_used, r.basis), (50.0, Basis::Table));
        assert_eq!(r.band_note, "80 km/h = 49.71 mph; project table gives 50 mph for this limit");
        let miss = resolve(70.0, SpeedUnit::Kmh, &UK, &t);
        assert_eq!(miss.basis, Basis::TableMiss);
        assert!((miss.mph_used - 43.4960).abs() < 1e-4);
        assert!(miss.band_note.contains("no entry"));
        assert_eq!(resolve(50.0, SpeedUnit::Mph, &UK, &t).basis, Basis::TableMiss);
    }

    #[test]
    fn ask_each_time() {
        let r = resolve(80.0, SpeedUnit::Kmh, &UK, &SpeedBandRule::AskEachTime);
        assert_eq!(r.basis, Basis::NeedsAnswer);
        let ask = r.ask.unwrap();
        assert_eq!(ask.band_mph, 50.0);
        assert!((ask.exact_mph - 49.7096).abs() < 1e-4);
        let yes = r.clone().answered(true);
        assert_eq!((yes.mph_used, yes.basis, yes.ask), (50.0, Basis::RoundedUp, None));
        assert_eq!(yes.band_note, "80 km/h = 49.71 mph; the user chose to treat it as 50 mph");
        let no = r.answered(false);
        assert!((no.mph_used - 49.7096).abs() < 1e-4);
        assert_eq!(no.basis, Basis::Exact);
        // Where the default rule would not round up, no question is asked.
        assert_eq!(resolve(100.0, SpeedUnit::Kmh, &UK, &SpeedBandRule::AskEachTime).ask, None);
    }

    #[test]
    fn unsorted_thresholds_are_sorted() {
        assert_eq!(resolve(80.0, SpeedUnit::Kmh, &[60.0, 30.0, 50.0, 40.0], &up()).mph_used, 50.0);
    }

    #[test]
    fn rule_is_stored_in_the_document() {
        let mut doc = Document::default();
        assert_eq!(speed_band_rule(&doc), SpeedBandRule::default());
        let rule = SpeedBandRule::Table(vec![(80.0, 50.0)]);
        set_speed_band_rule(&mut doc, &rule);
        let back = Document::from_json(&doc.to_json()).unwrap();
        assert_eq!(speed_band_rule(&back), rule);
        set_speed_band_rule(&mut doc, &SpeedBandRule::AskEachTime);
        assert_eq!(speed_band_rule(&doc), SpeedBandRule::AskEachTime);
        // An unreadable stored value falls back to the default.
        set_project_setting(&mut doc, SPEED_BAND_RULE_KEY, serde_json::json!("nonsense"));
        assert_eq!(speed_band_rule(&doc), SpeedBandRule::default());
    }
}
