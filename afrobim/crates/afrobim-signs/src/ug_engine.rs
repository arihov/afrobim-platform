// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Loading a folder rule pack into a [`SignEngine`], and the engine's view of the sections only
//! the Uganda pack carries.

use std::path::Path;

use crate::font::FontMetrics;
use crate::pack::RulePack;
use crate::ug_pack::{ExtraRules, RoundelTriangleRules, StandardSizeRules};
use crate::{find_font, std_catalog, std_pack, xml_pack, SignEngine, SignError};

impl SignEngine {
    /// `pack_path`: a `pack.xml` manifest or the folder holding one (C#
    /// `RulePackLoader.LoadXmlPack`); `fonts_dir` as for [`SignEngine::load`].
    pub(crate) fn load_folder_pack(
        pack_path: &Path,
        fonts_dir: &Path,
    ) -> Result<SignEngine, SignError> {
        let folder = xml_pack::load(pack_path)?;
        let (value, shown) = (&folder.value, folder.manifest.as_str());
        let pack = RulePack::from_value(value, shown)?;
        let extra = ExtraRules::from_value(value, shown)?;
        let std = std_pack::StdRules::from_value(value, shown, &pack)?;
        let medium = FontMetrics::load(&find_font(fonts_dir, &pack.fonts.on_dark.file))?;
        let heavy = FontMetrics::load(&find_font(fonts_dir, &pack.fonts.on_light.file))?;
        let catalog = std_catalog::entries(&std);
        Ok(SignEngine {
            pack,
            medium,
            heavy,
            std,
            catalog,
            extra,
        })
    }

    /// The pack's speed-class body-size ladders (Uganda Vol.1 Table D.1 / D.3 / D.4 and the
    /// supplementary Table 2), when it carries them. They are document metadata: the layouts
    /// draw in stroke widths and do not read them.
    pub fn standard_sizes(&self) -> Option<&StandardSizeRules> {
        self.extra.standard_sizes.as_ref()
    }

    /// The pack's warning / regulatory body outlines and provisional proportions, when it
    /// carries them.
    pub fn roundels_triangles(&self) -> Option<&RoundelTriangleRules> {
        self.extra.roundels_triangles.as_ref()
    }
}
