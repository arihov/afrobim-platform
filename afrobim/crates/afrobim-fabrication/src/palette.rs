//! The colours a sign resolves to, and names for colours. Port of C# `SignPalette.Resolve`
//! (afrobim-signs has the same resolution but keeps it private) and of the print palette
//! `SignColours.Rgb` the PDF writers use.

use afrobim_signs::pack::RulePack;
use afrobim_signs::{Colour, ColourScheme, FontFace, SignDefinition};

/// The concrete colours and lettering face of a sign.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    pub ground: Colour,
    pub lettering: Colour,
    pub arm: Colour,
    pub face: FontFace,
}

/// Resolution order: an explicit scheme; otherwise the scheme of the sign's catalog entry;
/// otherwise the explicit background with Transport Heavy in black on white and Transport
/// Medium in white on anything else.
pub fn resolve(sign: &SignDefinition, pack: &RulePack) -> Palette {
    let scheme = scheme_name(sign.scheme).or_else(|| {
        let key = sign.catalog_key.as_deref().filter(|k| !k.is_empty())?;
        pack.catalog.as_ref()?.find(key)?.scheme.as_deref()
    });
    if let Some(entry) = scheme.and_then(|s| pack.palettes.as_ref()?.find(s)) {
        return Palette { ground: entry.ground, lettering: entry.lettering, arm: entry.arm, face: entry.face };
    }
    let white = sign.background == Colour::White;
    let ink = if white { Colour::Black } else { Colour::White };
    Palette {
        ground: sign.background,
        lettering: ink,
        arm: ink,
        face: if white { FontFace::TransportHeavy } else { FontFace::TransportMedium },
    }
}

fn scheme_name(scheme: ColourScheme) -> Option<&'static str> {
    match scheme {
        ColourScheme::Primary => Some("primary"),
        ColourScheme::Secondary => Some("secondary"),
        ColourScheme::Information => Some("information"),
        ColourScheme::Tourist => Some("tourist"),
        ColourScheme::Temporary => Some("temporary"),
        ColourScheme::Motorway => Some("motorway"),
        ColourScheme::Auto => None,
    }
}

/// The colour's name as the C# enum prints it.
pub fn colour_name(c: Colour) -> &'static str {
    match c {
        Colour::White => "White",
        Colour::Black => "Black",
        Colour::Green => "Green",
        Colour::Blue => "Blue",
        Colour::Brown => "Brown",
        Colour::Yellow => "Yellow",
        Colour::Red => "Red",
        Colour::Grey => "Grey",
    }
}

/// The colour's position in the C# enum, which is the order colours sort in.
pub fn colour_index(c: Colour) -> u8 {
    match c {
        Colour::White => 0,
        Colour::Black => 1,
        Colour::Green => 2,
        Colour::Blue => 3,
        Colour::Brown => 4,
        Colour::Yellow => 5,
        Colour::Red => 6,
        Colour::Grey => 7,
    }
}

/// Print colours for the eight sign colours (C# `SignColours.Rgb`, the desktop app's base
/// display palette). These are the values the PDFs are filled with; they are not specification
/// values.
pub fn print_rgb(c: Colour) -> [u8; 3] {
    match c {
        Colour::White => [0xFF, 0xFF, 0xFF],
        Colour::Black => [0x00, 0x00, 0x00],
        Colour::Green => [0x02, 0x5C, 0x2E],
        Colour::Blue => [0x00, 0x5D, 0xA6],
        Colour::Brown => [0x6B, 0x42, 0x1F],
        Colour::Yellow => [0xFF, 0xC8, 0x00],
        Colour::Red => [0xC8, 0x10, 0x2E],
        Colour::Grey => [0x8C, 0x8C, 0x8C],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_sort_in_enum_order() {
        let all = [Colour::White, Colour::Black, Colour::Green, Colour::Blue, Colour::Brown, Colour::Yellow, Colour::Red, Colour::Grey];
        for (i, c) in all.iter().enumerate() {
            assert_eq!(colour_index(*c) as usize, i);
            assert_eq!(format!("{c:?}"), colour_name(*c));
        }
    }
}
