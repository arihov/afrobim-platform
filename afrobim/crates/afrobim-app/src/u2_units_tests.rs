// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Glue tests: typed MOVE/COPY offsets are read in the document's length unit.

use afrobim_annotate::{DisplayUnits, LengthUnit};
use afrobim_kernel::Pt;

use crate::anno_glue::parse_point;

#[test]
fn a_typed_offset_is_read_in_the_documents_length_unit_and_handed_over_in_millimetres() {
    let metres = DisplayUnits { length: LengthUnit::Metre, ..DisplayUnits::default() };
    let mm = DisplayUnits { length: LengthUnit::Millimetre, ..DisplayUnits::default() };
    let base = Some(Pt::new(5000.0, 1000.0));
    // Relative, in metres: 2 m east and 0.5 m north of the base point.
    let p = parse_point("@2,0.5", base, &metres).unwrap();
    assert!((p.x - 7000.0).abs() < 1e-9 && (p.y - 1500.0).abs() < 1e-9, "{p:?}");
    // The same text in a millimetre document is millimetres.
    let p = parse_point("@2,0.5", base, &mm).unwrap();
    assert!((p.x - 5002.0).abs() < 1e-9 && (p.y - 1000.5).abs() < 1e-9, "{p:?}");
    // A word is not a point and passes through to the controller unchanged.
    assert!(parse_point("all", base, &metres).is_none());
}
