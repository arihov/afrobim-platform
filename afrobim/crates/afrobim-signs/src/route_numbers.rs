// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Route-number text rules (UK TSM Chapter 7). Port of C# `RouteNumbers`.

use crate::model::{Destination, RouteStatus};

/// The space before a route-number run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RouteGap {
    None,
    Letter,
    Word,
}

/// One tile group of a route number and the space that precedes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RouteNumberRun {
    pub text: String,
    pub gap_before: RouteGap,
}

/// The status a route number implies on its own: motorway for `M1`, `(M11)` and `A1(M)`;
/// otherwise unspecified, because an A or B number can be primary or not.
pub(crate) fn infer_status(number: Option<&str>) -> RouteStatus {
    let Some(number) = number.filter(|n| !n.trim().is_empty()) else {
        return RouteStatus::Unspecified;
    };
    let core = number
        .trim()
        .trim_start_matches('(')
        .trim_end_matches(')')
        .to_uppercase();
    if core.contains("(M") {
        return RouteStatus::Motorway;
    }
    let mut chars = core.chars();
    match (chars.next(), chars.next()) {
        (Some('M'), Some(d)) if is_digit(d) => RouteStatus::Motorway,
        _ => RouteStatus::Unspecified,
    }
}

/// The status that colours a destination's route-number patch.
pub(crate) fn patch_status(d: &Destination) -> RouteStatus {
    if d.number_status != RouteStatus::Unspecified {
        return d.number_status;
    }
    match infer_status(d.route_number.as_deref()) {
        RouteStatus::Unspecified => d.route,
        inferred => inferred,
    }
}

/// Splits a route number into the runs that are set as separate tile groups. Chapter 7
/// Figure 3-9 puts a 1 sw gap between the route letter(s) and the number, and between the
/// number and an `(M)` suffix: `(A1(M))` becomes `(A`, `1`, `(M))`. Spaces in the input
/// separate words (e.g. `M1 North`).
pub(crate) fn runs(number: &str) -> Vec<RouteNumberRun> {
    let mut runs = Vec::new();
    for (w, word) in number
        .trim()
        .split(' ')
        .filter(|w| !w.is_empty())
        .enumerate()
    {
        let chars: Vec<char> = word.chars().collect();
        let mut start = 0;
        for i in 1..=chars.len() {
            let boundary = i == chars.len()
                || (chars[i - 1].is_alphabetic() && is_digit(chars[i]))
                || (is_digit(chars[i - 1]) && chars[i] == '(');
            if !boundary {
                continue;
            }
            let gap_before = if start == 0 && w > 0 {
                RouteGap::Word
            } else if start == 0 {
                RouteGap::None
            } else {
                RouteGap::Letter
            };
            runs.push(RouteNumberRun {
                text: chars[start..i].iter().collect(),
                gap_before,
            });
            start = i;
        }
    }
    runs
}

/// A decimal digit. C# `char.IsDigit` also accepts the decimal digits of other scripts;
/// route numbers are written with 0-9.
fn is_digit(c: char) -> bool {
    c.is_ascii_digit()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(number: &str) -> Vec<(String, RouteGap)> {
        runs(number)
            .into_iter()
            .map(|r| (r.text, r.gap_before))
            .collect()
    }

    /// Figure 3-9: 1 sw between the route letter(s) and the number, and before "(M)".
    #[test]
    fn a_route_number_splits_at_letter_digit_and_before_the_m_suffix() {
        assert_eq!(
            texts("A11"),
            vec![
                ("A".into(), RouteGap::None),
                ("11".into(), RouteGap::Letter)
            ]
        );
        assert_eq!(
            texts("(A1(M))"),
            vec![
                ("(A".into(), RouteGap::None),
                ("1".into(), RouteGap::Letter),
                ("(M))".into(), RouteGap::Letter)
            ]
        );
        assert_eq!(
            texts(" M1  North "),
            vec![
                ("M".into(), RouteGap::None),
                ("1".into(), RouteGap::Letter),
                ("North".into(), RouteGap::Word)
            ]
        );
        assert!(texts("  ").is_empty());
    }

    #[test]
    fn motorway_numbers_are_inferred() {
        assert_eq!(infer_status(Some("M1")), RouteStatus::Motorway);
        assert_eq!(infer_status(Some("(m11)")), RouteStatus::Motorway);
        assert_eq!(infer_status(Some("A1(M)")), RouteStatus::Motorway);
        assert_eq!(infer_status(Some("A11")), RouteStatus::Unspecified);
        assert_eq!(infer_status(Some("Main")), RouteStatus::Unspecified);
        assert_eq!(infer_status(Some("M")), RouteStatus::Unspecified);
        assert_eq!(infer_status(None), RouteStatus::Unspecified);
    }

    #[test]
    fn the_patch_status_prefers_explicit_then_inferred_then_route() {
        let mut d = Destination::new("x", crate::model::Direction::Ahead)
            .with_route("(M11)", RouteStatus::Primary);
        assert_eq!(patch_status(&d), RouteStatus::Motorway);
        d.number_status = RouteStatus::NonPrimary;
        assert_eq!(patch_status(&d), RouteStatus::NonPrimary);
        let d = Destination::new("x", crate::model::Direction::Ahead)
            .with_route("A40", RouteStatus::Primary);
        assert_eq!(patch_status(&d), RouteStatus::Primary);
    }
}
