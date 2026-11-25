// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Keys the AfroBIM dialogs ask for. `missing()` lists those a pack does not hold, with the
//! reason the DMRB pack does not hold them (what was looked at and what was found).
//!
//! The reasons describe the DMRB search made when the pack was read (2026-11-16). A keyword
//! search of a PDF is not exhaustive, so "not found" means "not found by that search", not
//! "the document has nothing on it".

pub const WANTED: &[(&str, &str)] = &[
    // Held by the DMRB pack (listed so a pack that lacks them is reported).
    ("link.ssd.desirable_min", "stopping sight distance by design speed"),
    ("link.horizontal.desirable_min_radius_superelevation_5", "minimum radius by design speed"),
    ("link.vertical.crest_k_desirable_min", "crest K by design speed"),
    ("link.vertical.sag_k_desirable_min", "sag K by design speed"),
    ("link.superelevation.max_rural", "maximum superelevation, rural"),
    ("link.superelevation.max_urban", "maximum superelevation, urban"),
    ("link.crossfall.normal", "normal crossfall"),
    ("link.gradient.desirable_max", "desirable maximum gradient by road type"),
    ("link.gradient.min_desirable", "minimum gradient for drainage"),
    ("xs.rural_all_purpose.lane_width", "lane width"),
    ("junction.diverge.length_up_0_4", "diverge taper and auxiliary lane length"),
    ("roundabout.icd.min", "roundabout inscribed circle diameter, lower limit"),
    ("drainage.storm.no_flooding_return_period", "design return period"),
    ("drainage.velocity.min_internal_pipe", "minimum self-cleansing velocity"),
    ("drainage.culvert.min_diameter", "minimum culvert diameter"),
    // Not held.
    ("xs.connector_road.lane_width", "CD 127 Figures 2.1.1N1b, d, f and h (connector road cross-sections) were not read into the pack"),
    ("xs.vrs_setback.minimum", "CD 127 Table 2.24 (VRS set-back) was not read into the pack"),
    ("xs.verge.width_where_varies", "CD 127 figures show verge and berm widths as 'Varies'; the document gives no number for those cases"),
    ("junction.signal.stop_line_and_lane_geometry", "CD 123 sections on signal-controlled junction geometry were not read into the pack"),
    ("junction.stagger.minimum_distance", "CD 123 Table 2.25 and clauses 2.23 and 2.24 were not read into the pack (table located, verbs not read)"),
    ("roundabout.mini.geometry", "CD 116 mini-roundabout tables (5.21, 5.23, 6.13) were not read into the pack"),
    ("drainage.pipe.min_diameter", "no minimum carrier pipe diameter was found in CG 501, CD 521, CD 522, CD 523, CD 527, CD 529 or CC 500 by keyword search (CD 527 mentions 150 mm only in worked examples)"),
    ("drainage.pipe.min_cover", "no minimum pipe cover was found in CG 501, CD 521, CD 522, CD 523, CD 527, CD 529 or CC 500 by keyword search; CD 524 (edge of pavement details) was not fetched"),
    ("drainage.pipe.max_velocity", "no maximum pipe velocity was found by the same keyword search"),
    ("drainage.chamber.max_spacing", "no chamber spacing was found by the same keyword search"),
    ("drainage.gully.spacing", "CD 526 Spacing of road gullies was fetched but its spacing procedure was not read into the pack"),
    ("drainage.velocity.min_sumpless_gully_network", "CD 527 Tables 2.37a to 2.37d (sumpless gully networks, cases A to D) were read but not stored; they apply only to networks with sumpless gullies"),
    ("earthworks.side_slope.angle", "DMRB gives no side slope angle; CD 622 Appendix D D6 leaves cutting and embankment side slopes to the geotechnical assessment (see entry earthworks.side_slopes.no_value)"),
    ("survey.setting_out.horizontal_tolerance", "no DMRB or MCHW Volume 1 clause tabulating setting-out tolerances was found; CC 601 clause 1.10 defers earthworks tolerances to the project's CC 601/WSR/001 (see entry earthworks.tolerance.no_value); GC 101 was fetched but not read in full"),
    ("survey.setting_out.vertical_tolerance", "as for survey.setting_out.horizontal_tolerance"),
    ("survey.earthworks.surface_tolerance", "CC 601 clause 1.10 gives no number; see entry earthworks.tolerance.no_value"),
    ("pavement.cd226.layer_thickness", "CD 226 was not fetched in this round; AfroBIM's pavement crate (design::dmrb) uses CD 224 and CD 226 equations from superseded revisions that should be refreshed"),
    ("lighting.design.illuminance", "no lighting document was fetched; road lighting design values are not in CD 377 and the DMRB lighting document was not located"),
];
