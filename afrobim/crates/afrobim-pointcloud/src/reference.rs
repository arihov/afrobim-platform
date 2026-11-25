// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! `PointCloudReference`: what a document stores about a point cloud, without embedding points.
//!
//! It holds the source path, the SHA-256 of the source, the header facts, the coordinate system
//! text, the filters and the display settings. Opening the cloud again goes through
//! [`PointCloudReference::open`], which checks the hash and uses the cache file beside the source.

use crate::cache::{open_indexed, sha256_file, OpenedIndex};
use crate::color::{ClassPalette, ColorMode, ColorRamp, RgbScale};
use crate::error::Result;
use crate::filter::PointFilter;
use crate::header::{CrsInfo, HeaderInfo};
use crate::index::IndexOptions;
use crate::source::{read_header, SourceOptions};
use crate::text::TextFormat;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// How a cloud is drawn. All numbers are the user's choice.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DisplaySettings {
    pub visible: bool,
    pub color_mode: ColorMode,
    /// Point size in screen pixels.
    pub point_size: f32,
    /// Most points drawn per view (the level-of-detail budget).
    pub max_points_per_view: usize,
}

impl DisplaySettings {
    /// Visible, coloured by the file's own colour where it has some (16-bit assumed unless the
    /// values stay within 255), 2 pixel points, 1 000 000 points per view. A starting point only.
    pub fn starting() -> DisplaySettings {
        DisplaySettings { visible: true, color_mode: ColorMode::Rgb { scale: RgbScale::Auto }, point_size: 2.0, max_points_per_view: 1_000_000 }
    }

    /// Elevation colouring over a range the caller supplies.
    pub fn by_elevation(min: f64, max: f64) -> DisplaySettings {
        DisplaySettings { color_mode: ColorMode::Elevation { min, max, ramp: ColorRamp::rainbow() }, ..DisplaySettings::starting() }
    }

    /// Class colouring with the crate's cosmetic palette.
    pub fn by_class() -> DisplaySettings {
        DisplaySettings { color_mode: ColorMode::Class { palette: ClassPalette::standard() }, ..DisplaySettings::starting() }
    }
}

/// Result of comparing the file on disk with the reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceCheck {
    Unchanged,
    /// The file exists but its hash differs from the stored one.
    Changed,
    Missing,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PointCloudReference {
    pub name: String,
    pub source_path: String,
    /// Lower-case hex SHA-256 of the source when the reference was made.
    pub source_hash: String,
    pub header: HeaderInfo,
    /// The coordinate system in effect: the file's, or the one the user set when the file has none.
    pub crs: CrsInfo,
    /// Text files: the column mapping used.
    #[serde(default)]
    pub text_format: Option<TextFormat>,
    #[serde(default = "yes")]
    pub e57_apply_pose: bool,
    #[serde(default)]
    pub filter: PointFilter,
    pub display: DisplaySettings,
}

fn yes() -> bool {
    true
}

impl PointCloudReference {
    /// Reads the header and hashes the file. No points are loaded (text files: one pass).
    pub fn from_file(name: &str, path: &Path, opts: &SourceOptions) -> Result<PointCloudReference> {
        let header = read_header(path, opts)?;
        Ok(PointCloudReference {
            name: name.to_string(),
            source_path: path.to_string_lossy().into_owned(),
            source_hash: sha256_file(path)?,
            crs: header.crs.clone(),
            header,
            text_format: opts.text.clone(),
            e57_apply_pose: opts.e57_apply_pose,
            filter: PointFilter::default(),
            display: DisplaySettings::starting(),
        })
    }

    pub fn source_options(&self) -> SourceOptions {
        SourceOptions { text: self.text_format.clone(), e57_apply_pose: self.e57_apply_pose }
    }

    /// Hashes the file at `source_path` and compares.
    pub fn check_source(&self) -> Result<SourceCheck> {
        let p = Path::new(&self.source_path);
        if !p.exists() {
            return Ok(SourceCheck::Missing);
        }
        Ok(if sha256_file(p)? == self.source_hash { SourceCheck::Unchanged } else { SourceCheck::Changed })
    }

    /// Opens the index (through the cache file). Fails if the file changed since the reference was made.
    pub fn open(&self, idx: &IndexOptions) -> Result<OpenedIndex> {
        let opened = open_indexed(Path::new(&self.source_path), &self.source_options(), idx)?;
        if opened.source_hash != self.source_hash {
            return Err(crate::error::PointCloudError::Invalid(format!("{} changed since the reference was made (hash differs); make a new reference", self.source_path)));
        }
        Ok(opened)
    }

    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string(self)?)
    }

    pub fn from_json(text: &str) -> Result<PointCloudReference> {
        Ok(serde_json::from_str(text)?)
    }
}
