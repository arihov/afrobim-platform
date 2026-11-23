//! The pixels of an image, decimated while they are read.
//!
//! An orthophoto from a drone survey is often tens of thousands of pixels square: decoding it
//! whole would take gigabytes. The image is read in pieces and only every `step`-th pixel along
//! both axes is kept (nearest neighbour), so memory is bounded by the output, not the file:
//!
//! * TIFF: strip by strip or tile by tile. A strip that holds no kept row is not decoded at all.
//!   When the file carries internal overviews (reduced-resolution images, as GDAL writes with
//!   `gdaladdo`), the smallest one that still meets the requested size is read instead.
//! * PNG: row by row, with 16-bit samples cut to 8 bits and palettes expanded by the decoder.
//! * JPEG and other formats: decoded whole by the `image` crate under an allocation limit, then
//!   reduced. JPEG orthophotos are rarely large enough for this to matter.
//!
//! 16-bit TIFF imagery (multispectral cameras, satellite scenes) is stretched to 8 bits between
//! the 2nd and 98th percentile of each band. Floating-point rasters (elevation models) are refused:
//! they are surfaces, not pictures.

use std::io::BufReader;
use std::path::Path;

/// What the caller asks for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DecodeOptions {
    /// The longest side of the result, pixels (the GPU texture limit, or a detail setting).
    pub max_side: u32,
    /// Pixels that are pure black (all colour bands zero) become transparent: the collar around
    /// an orthophoto mosaic.
    pub transparent_black: bool,
}

impl Default for DecodeOptions {
    fn default() -> Self {
        DecodeOptions { max_side: 4096, transparent_black: false }
    }
}

/// A decoded, decimated image: RGBA, 8 bits per sample, rows from the top.
#[derive(Debug, Clone, PartialEq)]
pub struct Decoded {
    pub width: u32,
    pub height: u32,
    /// Size of the full-resolution image in the file.
    pub src_width: u32,
    pub src_height: u32,
    pub rgba: Vec<u8>,
    /// What was done, for the window: the overview used, the stretch applied.
    pub notes: Vec<String>,
}

/// The step (kept every `step`-th pixel) that brings `w` by `h` within `max_side`.
pub fn step_for(w: u32, h: u32, max_side: u32) -> u32 {
    let m = w.max(h).max(1);
    m.div_ceil(max_side.max(1)).max(1)
}

/// Reads `path` and returns it decimated to `opts.max_side`.
pub fn decode(path: &Path, opts: DecodeOptions) -> Result<Decoded, String> {
    let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    let mut d = match ext.as_str() {
        "tif" | "tiff" => decode_tiff(path, opts.max_side)?,
        "png" => decode_png(path, opts.max_side)?,
        _ => decode_other(path, opts.max_side)?,
    };
    if opts.transparent_black {
        for px in d.rgba.chunks_exact_mut(4) {
            if px[0] == 0 && px[1] == 0 && px[2] == 0 {
                px[3] = 0;
            }
        }
    }
    Ok(d)
}

// ─── PNG ────────────────────────────────────────────────────────────────────

fn decode_png(path: &Path, max_side: u32) -> Result<Decoded, String> {
    let f = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut dec = png::Decoder::new(BufReader::new(f));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = dec.read_info().map_err(|e| format!("{} is not a PNG that can be read: {e}", path.display()))?;
    let (w, h) = (reader.info().width, reader.info().height);
    let (ct, _) = reader.output_color_type();
    let spp = match ct {
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Indexed => return Err("The PNG palette was not expanded.".into()),
    };
    if reader.info().interlaced {
        // Interlaced rows arrive in passes; decode whole (these are small in practice).
        return decode_other(path, max_side);
    }
    let step = step_for(w, h, max_side);
    let (ow, oh) = (w.div_ceil(step), h.div_ceil(step));
    let mut out = vec![0u8; ow as usize * oh as usize * 4];
    let mut y = 0u32;
    while let Some(row) = reader.next_row().map_err(|e| format!("{}: {e}", path.display()))? {
        if y % step == 0 {
            let oy = (y / step) as usize;
            let data = row.data();
            for ox in 0..ow as usize {
                let sx = ox * step as usize * spp;
                put(&mut out[(oy * ow as usize + ox) * 4..][..4], &data[sx..sx + spp], spp);
            }
        }
        y += 1;
    }
    let mut notes = Vec::new();
    if step > 1 {
        notes.push(format!("Shown at 1:{step} ({ow} x {oh} of {w} x {h} pixels)."));
    }
    Ok(Decoded { width: ow, height: oh, src_width: w, src_height: h, rgba: out, notes })
}

/// One pixel of `spp` 8-bit samples into RGBA.
fn put(dst: &mut [u8], s: &[u8], spp: usize) {
    match spp {
        1 => dst.copy_from_slice(&[s[0], s[0], s[0], 255]),
        2 => dst.copy_from_slice(&[s[0], s[0], s[0], s[1]]),
        3 => dst.copy_from_slice(&[s[0], s[1], s[2], 255]),
        _ => dst.copy_from_slice(&[s[0], s[1], s[2], s[3]]),
    }
}

// ─── JPEG and the rest ──────────────────────────────────────────────────────

fn decode_other(path: &Path, max_side: u32) -> Result<Decoded, String> {
    let mut reader = image::ImageReader::open(path).map_err(|e| format!("{}: {e}", path.display()))?.with_guessed_format().map_err(|e| format!("{}: {e}", path.display()))?;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(1536 * 1024 * 1024);
    reader.limits(limits);
    let img = reader.decode().map_err(|e| format!("{} could not be decoded: {e}", path.display()))?;
    let (w, h) = (img.width(), img.height());
    let step = step_for(w, h, max_side);
    let rgba = if step > 1 { img.thumbnail(w.div_ceil(step), h.div_ceil(step)).to_rgba8() } else { img.to_rgba8() };
    let mut notes = Vec::new();
    if step > 1 {
        notes.push(format!("Shown reduced ({} x {} of {w} x {h} pixels).", rgba.width(), rgba.height()));
    }
    Ok(Decoded { width: rgba.width(), height: rgba.height(), src_width: w, src_height: h, rgba: rgba.into_raw(), notes })
}

// ─── TIFF ───────────────────────────────────────────────────────────────────

type TiffDec = tiff::decoder::Decoder<BufReader<std::fs::File>>;

fn open_tiff(path: &Path) -> Result<TiffDec, String> {
    let f = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut limits = tiff::decoder::Limits::default();
    // One strip or tile at a time; generous for wide strips, still bounded.
    limits.decoding_buffer_size = 512 * 1024 * 1024;
    tiff::decoder::Decoder::new(BufReader::new(f)).map(|d| d.with_limits(limits)).map_err(|e| format!("{} is not a TIFF that can be read: {e}", path.display()))
}

/// Samples per pixel and bits per sample of the current image, or why it is not imagery.
fn layout(dec: &mut TiffDec) -> Result<(usize, u8), String> {
    use tiff::ColorType as C;
    let ct = dec.colortype().map_err(|e| e.to_string())?;
    let (spp, bits) = match ct {
        C::Gray(b) => (1, b),
        C::GrayA(b) => (2, b),
        C::RGB(b) => (3, b),
        C::RGBA(b) => (4, b),
        C::Multiband { bit_depth, num_samples } if num_samples >= 1 => (num_samples as usize, bit_depth),
        C::Palette(_) => return Err("Palette (indexed colour) TIFFs are not read yet. Convert to RGB (gdal_translate -expand rgb).".into()),
        C::YCbCr(_) => return Err("YCbCr (JPEG-compressed) TIFFs are not read yet. Convert to RGB (gdal_translate -co PHOTOMETRIC=RGB).".into()),
        other => return Err(format!("A TIFF of colour type {other:?} is not imagery AfroBIM can show.")),
    };
    if bits != 8 && bits != 16 {
        return Err(format!("{bits}-bit samples are not imagery (an elevation model is loaded as a surface). AfroBIM shows 8 and 16-bit images."));
    }
    let planar = dec.find_tag_unsigned::<u16>(tiff::tags::Tag::PlanarConfiguration).ok().flatten().unwrap_or(1);
    if planar == 2 && spp > 1 {
        return Err("Band-separate (planar) TIFFs are not read yet. Convert to pixel interleave (gdal_translate -co INTERLEAVE=PIXEL).".into());
    }
    Ok((spp, bits))
}

/// The images in the file that are the full image or a reduced version of it: index and size.
/// Masks (NewSubfileType bit 2) are left out.
fn pyramid(path: &Path) -> Result<Vec<(usize, u32, u32)>, String> {
    let mut dec = open_tiff(path)?;
    let mut out = Vec::new();
    let mut i = 0usize;
    loop {
        let sub = dec.find_tag_unsigned::<u32>(tiff::tags::Tag::NewSubfileType).ok().flatten().unwrap_or(0);
        if sub & 4 == 0 {
            if let Ok((w, h)) = dec.dimensions() {
                out.push((i, w, h));
            }
        }
        if !dec.more_images() || i > 64 {
            break;
        }
        if dec.next_image().is_err() {
            break;
        }
        i += 1;
    }
    Ok(out)
}

fn decode_tiff(path: &Path, max_side: u32) -> Result<Decoded, String> {
    let levels = pyramid(path)?;
    let &(_, src_w, src_h) = levels.first().ok_or("The TIFF holds no image.")?;
    // The overviews keep the aspect of the full image; anything else is a different picture.
    let aspect = src_w as f64 / src_h.max(1) as f64;
    let usable: Vec<&(usize, u32, u32)> = levels.iter().filter(|(_, w, h)| ((*w as f64 / (*h).max(1) as f64) - aspect).abs() < 0.02 * aspect.max(1.0)).collect();
    // The smallest level that still meets max_side, else the largest.
    let chosen = usable.iter().filter(|(_, w, h)| (*w).max(*h) >= max_side).min_by_key(|(_, w, _)| *w).or_else(|| usable.iter().max_by_key(|(_, w, _)| *w)).copied().copied().unwrap_or(levels[0]);
    let (index, w, h) = chosen;
    let mut dec = open_tiff(path)?;
    if index > 0 {
        dec.seek_to_image(index).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    let (spp, bits) = layout(&mut dec)?;
    let step = step_for(w, h, max_side);
    let (ow, oh) = (w.div_ceil(step) as usize, h.div_ceil(step) as usize);
    let (cw, ch) = dec.chunk_dimensions();
    let (cw, ch) = (cw.max(1), ch.max(1));
    let across = w.div_ceil(cw);
    let count = across * h.div_ceil(ch);
    // 8-bit samples go straight to RGBA; 16-bit are kept and stretched at the end.
    let mut out8 = if bits == 8 { vec![0u8; ow * oh * 4] } else { Vec::new() };
    let mut out16 = if bits == 16 { vec![0u16; ow * oh * 4] } else { Vec::new() };
    let bands = spp.min(4);
    for i in 0..count {
        let (x0, y0) = ((i % across) * cw, (i / across) * ch);
        let (dw, dh) = (cw.min(w - x0), ch.min(h - y0));
        // Kept rows and columns that fall in this chunk.
        let oy0 = y0.div_ceil(step);
        let ox0 = x0.div_ceil(step);
        if oy0 * step >= y0 + dh || ox0 * step >= x0 + dw {
            continue;
        }
        let chunk = dec.read_chunk(i).map_err(|e| format!("{}: piece {i}: {e}", path.display()))?;
        // The decoder's row length is the chunk's data width (tiles are padded to the full tile width).
        let (rw, _) = dec.chunk_data_dimensions(i);
        let row_len = if dec.get_chunk_type() == tiff::decoder::ChunkType::Tile { cw as usize } else { rw as usize } * spp;
        let mut oy = oy0;
        while oy * step < y0 + dh && (oy as usize) < oh {
            let ly = (oy * step - y0) as usize;
            let mut ox = ox0;
            while ox * step < x0 + dw && (ox as usize) < ow {
                let lx = (ox * step - x0) as usize;
                let at = ly * row_len + lx * spp;
                let o = (oy as usize * ow + ox as usize) * 4;
                match &chunk {
                    tiff::decoder::DecodingResult::U8(v) => {
                        if at + spp <= v.len() {
                            put(&mut out8[o..o + 4], &v[at..at + bands], bands);
                        }
                    }
                    tiff::decoder::DecodingResult::U16(v) => {
                        if at + spp <= v.len() {
                            let s = &v[at..at + bands];
                            let px = match bands {
                                1 => [s[0], s[0], s[0], u16::MAX],
                                2 => [s[0], s[0], s[0], s[1]],
                                3 => [s[0], s[1], s[2], u16::MAX],
                                _ => [s[0], s[1], s[2], s[3]],
                            };
                            out16[o..o + 4].copy_from_slice(&px);
                        }
                    }
                    _ => return Err("Floating-point samples are not imagery.".into()),
                }
                ox += 1;
            }
            oy += 1;
        }
    }
    let mut notes = Vec::new();
    if index > 0 {
        notes.push(format!("Read from the file's overview {index} ({w} x {h} pixels)."));
    }
    if step > 1 {
        notes.push(format!("Shown at 1:{step} ({ow} x {oh} of {w} x {h} pixels)."));
    }
    if spp > 4 {
        notes.push(format!("The file has {spp} bands; the first three are shown as red, green and blue."));
    }
    let rgba = if bits == 16 {
        notes.push("16-bit samples stretched to 8 bits between the 2nd and 98th percentile of each band.".into());
        stretch16(&out16)
    } else {
        out8
    };
    Ok(Decoded { width: ow as u32, height: oh as u32, src_width: src_w, src_height: src_h, rgba, notes })
}

/// 16-bit RGBA to 8-bit, each colour band stretched between its 2nd and 98th percentile; alpha
/// is cut to 8 bits.
pub fn stretch16(px: &[u16]) -> Vec<u8> {
    let n = px.len() / 4;
    let mut out = vec![0u8; px.len()];
    for band in 0..3 {
        let mut hist = vec![0u32; 65536];
        for i in 0..n {
            hist[px[i * 4 + band] as usize] += 1;
        }
        let pct = |p: f64| -> u16 {
            let target = (n as f64 * p) as u64;
            let mut acc = 0u64;
            for (v, c) in hist.iter().enumerate() {
                acc += *c as u64;
                if acc > target {
                    return v as u16;
                }
            }
            u16::MAX
        };
        let (lo, hi) = (pct(0.02), pct(0.98));
        let span = (hi.saturating_sub(lo)).max(1) as f64;
        for i in 0..n {
            let v = px[i * 4 + band].saturating_sub(lo) as f64 / span;
            out[i * 4 + band] = (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        }
    }
    for i in 0..n {
        out[i * 4 + 3] = (px[i * 4 + 3] >> 8) as u8;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("afrobim-raster-{}-{name}", std::process::id()));
        let _ = std::fs::create_dir_all(&d);
        d
    }

    /// A w by h RGB image whose pixel (x, y) is (x, y, 7), modulo 256.
    fn pattern(w: u32, h: u32) -> Vec<u8> {
        (0..h).flat_map(|y| (0..w).flat_map(move |x| [(x % 256) as u8, (y % 256) as u8, 7])).collect()
    }

    #[test]
    fn the_step_brings_the_longest_side_within_the_limit() {
        assert_eq!(step_for(100, 50, 4096), 1);
        assert_eq!(step_for(8192, 100, 4096), 2);
        assert_eq!(step_for(8193, 100, 4096), 3);
        assert_eq!(step_for(0, 0, 0), 1);
    }

    #[test]
    fn a_png_is_decimated_row_by_row_to_the_nearest_kept_pixel() {
        let p = tmp("png").join("a.png");
        image::save_buffer(&p, &pattern(300, 200), 300, 200, image::ExtendedColorType::Rgb8).unwrap();
        let d = decode(&p, DecodeOptions { max_side: 100, transparent_black: false }).unwrap();
        assert_eq!((d.width, d.height, d.src_width, d.src_height), (100, 67, 300, 200));
        // Output pixel (10, 20) is source pixel (30, 60).
        let o = (20 * 100 + 10) * 4;
        assert_eq!(&d.rgba[o..o + 4], &[30, 60, 7, 255]);
        let full = decode(&p, DecodeOptions::default()).unwrap();
        assert_eq!((full.width, full.height), (300, 200));
        assert!(full.notes.is_empty());
    }

    #[test]
    fn black_collars_can_be_made_transparent() {
        let p = tmp("black").join("b.png");
        let mut px = pattern(4, 4);
        px[0..3].copy_from_slice(&[0, 0, 0]);
        image::save_buffer(&p, &px, 4, 4, image::ExtendedColorType::Rgb8).unwrap();
        let d = decode(&p, DecodeOptions { max_side: 64, transparent_black: true }).unwrap();
        assert_eq!(d.rgba[3], 0);
        assert_eq!(d.rgba[7], 255);
    }

    #[test]
    fn a_stripped_tiff_skips_strips_with_no_kept_row_and_samples_the_right_pixels() {
        use tiff::encoder::{colortype, TiffEncoder};
        let p = tmp("tif").join("s.tif");
        let (w, h) = (260u32, 130u32);
        let data = pattern(w, h);
        {
            let mut enc = TiffEncoder::new(std::fs::File::create(&p).unwrap()).unwrap();
            let mut img = enc.new_image::<colortype::RGB8>(w, h).unwrap();
            img.rows_per_strip(3).unwrap();
            img.write_data(&data).unwrap();
        }
        let d = decode(&p, DecodeOptions { max_side: 65, transparent_black: false }).unwrap();
        assert_eq!((d.width, d.height), (65, 33));
        let o = (10 * 65 + 20) * 4;
        assert_eq!(&d.rgba[o..o + 4], &[80, 40, 7, 255]);
        // The last kept pixel of the last row.
        let o = (32 * 65 + 64) * 4;
        assert_eq!(&d.rgba[o..o + 4], &[(256 % 256) as u8, 128, 7, 255]);
    }

    #[test]
    fn a_16_bit_tiff_is_stretched() {
        use tiff::encoder::{colortype, TiffEncoder};
        let p = tmp("tif16").join("g.tif");
        let data: Vec<u16> = (0..100u16).map(|i| 1000 + i * 10).collect();
        {
            let mut enc = TiffEncoder::new(std::fs::File::create(&p).unwrap()).unwrap();
            enc.write_image::<colortype::Gray16>(10, 10, &data).unwrap();
        }
        let d = decode(&p, DecodeOptions::default()).unwrap();
        assert_eq!(d.rgba[0], 0);
        assert_eq!(d.rgba[99 * 4], 255);
        assert!(d.notes.iter().any(|n| n.contains("stretched")));
    }

    #[test]
    fn a_floating_point_tiff_is_refused_as_not_imagery() {
        use tiff::encoder::{colortype, TiffEncoder};
        let p = tmp("tiff32").join("dem.tif");
        {
            let mut enc = TiffEncoder::new(std::fs::File::create(&p).unwrap()).unwrap();
            enc.write_image::<colortype::Gray32Float>(2, 2, &[1.0f32, 2.0, 3.0, 4.0]).unwrap();
        }
        let e = decode(&p, DecodeOptions::default()).unwrap_err();
        assert!(e.contains("not imagery"), "{e}");
    }
}
