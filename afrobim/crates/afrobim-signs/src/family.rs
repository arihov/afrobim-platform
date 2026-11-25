// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Human labels for sign families. Port of C# `SignFamily.Label(SignFamily.Token(kind), …)`,
//! the label diagnostics use to name a sign's family.

use crate::model::SignKind;

/// A short, human label for the family of a layout kind.
///
/// As in the C# engine, the label goes through the kind's family token
/// ([`SignKind::family_token`]), and a kind without a token of its own (warning, regulatory
/// and supplementary signs) takes the stack-type token, so it reads "Stack type".
pub(crate) fn label(kind: SignKind) -> &'static str {
    match kind.family_token() {
        "map-roundabout" => "Map · roundabout",
        "map-tee" => "Map · T-intersection",
        "map-crossroads" => "Map · crossroads",
        "direction" => "Direction",
        "confirmatory" => "Confirmatory",
        "place" => "Place identification",
        "lane" => "Lane pre-selection",
        "information" => "Information",
        "diversion" => "Diversion (temporary)",
        _ => "Stack type",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_guidance_family_has_its_csharp_label() {
        assert_eq!(label(SignKind::StackType), "Stack type");
        assert_eq!(label(SignKind::DirectionSign), "Direction");
        assert_eq!(label(SignKind::Confirmatory), "Confirmatory");
        assert_eq!(label(SignKind::PlaceIdentification), "Place identification");
        assert_eq!(label(SignKind::LanePreselection), "Lane pre-selection");
        assert_eq!(label(SignKind::Diversion), "Diversion (temporary)");
        assert_eq!(label(SignKind::MapTypeRoundabout), "Map · roundabout");
        assert_eq!(label(SignKind::MapTypeTee), "Map · T-intersection");
        assert_eq!(label(SignKind::MapTypeCrossroads), "Map · crossroads");
        assert_eq!(label(SignKind::InformationSign), "Information");
    }

    /// C# `SignFamily.Token` gives these kinds the stack token, so their label is the stack one.
    #[test]
    fn a_kind_without_a_family_token_reads_as_stack_type() {
        assert_eq!(label(SignKind::WarningSign), "Stack type");
        assert_eq!(label(SignKind::SupplementaryPlate), "Stack type");
    }
}
