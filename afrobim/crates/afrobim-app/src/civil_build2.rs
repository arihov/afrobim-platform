//! Scenes of corridors and sample line groups (see `civil_build`).

use afrobim_kernel::Element;

use crate::civil_build::{Built, Ctx};

pub fn build_corridor(_e: &Element, _ctx: &Ctx) -> Result<Built, String> {
    Ok(Built::default())
}

pub fn build_samples(_e: &Element, _ctx: &Ctx) -> Result<Built, String> {
    Ok(Built::default())
}
