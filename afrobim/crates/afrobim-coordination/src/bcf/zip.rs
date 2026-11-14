//! A minimal ZIP writer and reader (PKWARE APPNOTE 6.3.x subset) on top of `flate2`.
//!
//! The workspace has no `zip` crate, so this does what BCF needs: entries stored or deflated,
//! directory entries, forward slashes, CRC-32 checked on read. Not supported: ZIP64, encryption,
//! multi-disk archives, compression methods other than 0 (store) and 8 (deflate). Reading uses
//! the central directory only (so entries with a trailing data descriptor work).

use std::io::{Read, Write};

use flate2::read::DeflateDecoder;
use flate2::write::DeflateEncoder;
use flate2::{Compression, Crc};

/// One archive member.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZipEntry {
    /// Path with forward slashes. A trailing `/` marks a directory entry.
    pub name: String,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ZipError {
    /// Not a zip file, or damaged structure.
    Malformed(String),
    /// A feature this reader does not implement.
    Unsupported(String),
    /// CRC or size mismatch, or a bad deflate stream.
    Corrupt(String),
    TooLarge(String),
}

impl std::fmt::Display for ZipError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ZipError::Malformed(s) => write!(f, "malformed zip: {s}"),
            ZipError::Unsupported(s) => write!(f, "unsupported zip feature: {s}"),
            ZipError::Corrupt(s) => write!(f, "corrupt zip entry: {s}"),
            ZipError::TooLarge(s) => write!(f, "zip too large: {s}"),
        }
    }
}

impl std::error::Error for ZipError {}

fn crc32(data: &[u8]) -> u32 {
    let mut c = Crc::new();
    c.update(data);
    c.sum()
}

fn le16(b: &mut Vec<u8>, v: u16) {
    b.extend_from_slice(&v.to_le_bytes());
}
fn le32(b: &mut Vec<u8>, v: u32) {
    b.extend_from_slice(&v.to_le_bytes());
}

/// DOS date and time for 1980-01-01 00:00 (a fixed stamp keeps archives byte-reproducible).
const DOS_TIME: u16 = 0;
const DOS_DATE: u16 = (1 << 5) | 1;

/// Writes an archive. File data is deflated when that makes it smaller, otherwise stored.
/// Entry names must be valid UTF-8 (the language-encoding flag, bit 11, is set).
pub fn write_zip(entries: &[ZipEntry]) -> Result<Vec<u8>, ZipError> {
    if entries.len() >= 0xFFFF {
        return Err(ZipError::TooLarge("more than 65534 entries (ZIP64 not supported)".into()));
    }
    let mut out: Vec<u8> = Vec::new();
    let mut central: Vec<u8> = Vec::new();
    for e in entries {
        if e.data.len() >= u32::MAX as usize / 2 || out.len() >= u32::MAX as usize / 2 {
            return Err(ZipError::TooLarge(format!("{} (ZIP64 not supported)", e.name)));
        }
        let is_dir = e.name.ends_with('/');
        let crc = crc32(&e.data);
        let (method, body): (u16, Vec<u8>) = if is_dir || e.data.is_empty() {
            (0, e.data.clone())
        } else {
            let mut enc = DeflateEncoder::new(Vec::new(), Compression::default());
            enc.write_all(&e.data).map_err(|x| ZipError::Corrupt(x.to_string()))?;
            let packed = enc.finish().map_err(|x| ZipError::Corrupt(x.to_string()))?;
            if packed.len() < e.data.len() {
                (8, packed)
            } else {
                (0, e.data.clone())
            }
        };
        let offset = out.len() as u32;
        let name = e.name.as_bytes();
        let version: u16 = if method == 8 { 20 } else { 10 };
        // Local file header.
        le32(&mut out, 0x0403_4b50);
        le16(&mut out, version);
        le16(&mut out, 1 << 11);
        le16(&mut out, method);
        le16(&mut out, DOS_TIME);
        le16(&mut out, DOS_DATE);
        le32(&mut out, crc);
        le32(&mut out, body.len() as u32);
        le32(&mut out, e.data.len() as u32);
        le16(&mut out, name.len() as u16);
        le16(&mut out, 0);
        out.extend_from_slice(name);
        out.extend_from_slice(&body);
        // Central directory header.
        le32(&mut central, 0x0201_4b50);
        le16(&mut central, 20);
        le16(&mut central, version);
        le16(&mut central, 1 << 11);
        le16(&mut central, method);
        le16(&mut central, DOS_TIME);
        le16(&mut central, DOS_DATE);
        le32(&mut central, crc);
        le32(&mut central, body.len() as u32);
        le32(&mut central, e.data.len() as u32);
        le16(&mut central, name.len() as u16);
        le16(&mut central, 0);
        le16(&mut central, 0);
        le16(&mut central, 0);
        le16(&mut central, 0);
        le32(&mut central, if is_dir { 0x10 } else { 0 });
        le32(&mut central, offset);
        central.extend_from_slice(name);
    }
    let cd_offset = out.len() as u32;
    out.extend_from_slice(&central);
    le32(&mut out, 0x0605_4b50);
    le16(&mut out, 0);
    le16(&mut out, 0);
    le16(&mut out, entries.len() as u16);
    le16(&mut out, entries.len() as u16);
    le32(&mut out, central.len() as u32);
    le32(&mut out, cd_offset);
    le16(&mut out, 0);
    Ok(out)
}

fn rd16(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}
fn rd32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

/// Entries larger than this when unpacked are refused (a guard against decompression bombs).
pub const MAX_ENTRY_BYTES: u64 = 512 * 1024 * 1024;

/// Reads every entry of an archive. Names are returned with `\` turned into `/` (some Windows
/// tools write backslashes, which the BCF documentation asks writers not to do).
pub fn read_zip(bytes: &[u8]) -> Result<Vec<ZipEntry>, ZipError> {
    // End of central directory: search backwards (the comment may be up to 65535 bytes).
    let min = bytes.len().saturating_sub(22 + 65535);
    let eocd = (min..=bytes.len().saturating_sub(22))
        .rev()
        .find(|&i| rd32(bytes, i) == Some(0x0605_4b50))
        .ok_or_else(|| ZipError::Malformed("no end of central directory record".into()))?;
    let disk = rd16(bytes, eocd + 4).unwrap_or(0);
    let cd_disk = rd16(bytes, eocd + 6).unwrap_or(0);
    if disk != 0 || cd_disk != 0 {
        return Err(ZipError::Unsupported("multi-disk archive".into()));
    }
    let count = rd16(bytes, eocd + 10).ok_or_else(|| ZipError::Malformed("short EOCD".into()))? as usize;
    let cd_size = rd32(bytes, eocd + 12).unwrap_or(0) as usize;
    let cd_off = rd32(bytes, eocd + 16).unwrap_or(0) as usize;
    if count == 0xFFFF || cd_size == 0xFFFF_FFFF || cd_off == 0xFFFF_FFFF {
        return Err(ZipError::Unsupported("ZIP64".into()));
    }
    let mut entries = Vec::with_capacity(count);
    let mut p = cd_off;
    for _ in 0..count {
        if rd32(bytes, p) != Some(0x0201_4b50) {
            return Err(ZipError::Malformed("bad central directory header".into()));
        }
        let flags = rd16(bytes, p + 8).unwrap_or(0);
        let method = rd16(bytes, p + 10).unwrap_or(0);
        let crc = rd32(bytes, p + 16).unwrap_or(0);
        let csize = rd32(bytes, p + 20).unwrap_or(0) as usize;
        let usize_ = rd32(bytes, p + 24).unwrap_or(0) as u64;
        let nlen = rd16(bytes, p + 28).unwrap_or(0) as usize;
        let xlen = rd16(bytes, p + 30).unwrap_or(0) as usize;
        let clen = rd16(bytes, p + 32).unwrap_or(0) as usize;
        let lho = rd32(bytes, p + 42).unwrap_or(0) as usize;
        let name_bytes = bytes.get(p + 46..p + 46 + nlen).ok_or_else(|| ZipError::Malformed("short name".into()))?;
        let name = String::from_utf8_lossy(name_bytes).replace('\\', "/");
        p += 46 + nlen + xlen + clen;
        if flags & 1 != 0 {
            return Err(ZipError::Unsupported(format!("{name}: encrypted entry")));
        }
        if usize_ > MAX_ENTRY_BYTES {
            return Err(ZipError::TooLarge(format!("{name}: {usize_} bytes unpacked")));
        }
        if rd32(bytes, lho) != Some(0x0403_4b50) {
            return Err(ZipError::Malformed(format!("{name}: bad local header")));
        }
        let lname = rd16(bytes, lho + 26).unwrap_or(0) as usize;
        let lextra = rd16(bytes, lho + 28).unwrap_or(0) as usize;
        let start = lho + 30 + lname + lextra;
        let raw = bytes.get(start..start + csize).ok_or_else(|| ZipError::Malformed(format!("{name}: data runs past the end")))?;
        let data = match method {
            0 => raw.to_vec(),
            8 => {
                let mut out = Vec::with_capacity(usize_ as usize);
                DeflateDecoder::new(raw).take(MAX_ENTRY_BYTES + 1).read_to_end(&mut out).map_err(|e| ZipError::Corrupt(format!("{name}: {e}")))?;
                out
            }
            m => return Err(ZipError::Unsupported(format!("{name}: compression method {m}"))),
        };
        if data.len() as u64 != usize_ || crc32(&data) != crc {
            return Err(ZipError::Corrupt(format!("{name}: size or CRC-32 mismatch")));
        }
        entries.push(ZipEntry { name, data });
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<ZipEntry> {
        vec![
            ZipEntry { name: "bcf.version".into(), data: b"<Version/>".to_vec() },
            ZipEntry { name: "abc/".into(), data: vec![] },
            ZipEntry { name: "abc/markup.bcf".into(), data: "hello ".repeat(500).into_bytes() },
            ZipEntry { name: "abc/snapshot.png".into(), data: (0..=255u8).collect() },
            ZipEntry { name: "abc/empty.txt".into(), data: vec![] },
            ZipEntry { name: "abc/unicode-\u{e9}.txt".into(), data: "caf\u{e9}".as_bytes().to_vec() },
        ]
    }

    #[test]
    fn round_trip_keeps_names_and_bytes() {
        let z = write_zip(&sample()).unwrap();
        assert_eq!(read_zip(&z).unwrap(), sample());
        // The repetitive entry deflates, the incompressible one is stored.
        assert!(z.len() < sample().iter().map(|e| e.data.len()).sum::<usize>());
    }

    #[test]
    fn output_is_reproducible() {
        assert_eq!(write_zip(&sample()).unwrap(), write_zip(&sample()).unwrap());
    }

    #[test]
    fn crc_mismatch_and_garbage_are_errors() {
        let mut z = write_zip(&sample()).unwrap();
        // Flip a byte inside the first entry's data (after the 30 + 11 byte local header).
        z[30 + 11 + 2] ^= 0xFF;
        assert!(matches!(read_zip(&z), Err(ZipError::Corrupt(_))));
        assert!(matches!(read_zip(b"not a zip at all, just text"), Err(ZipError::Malformed(_))));
        assert!(matches!(read_zip(&[]), Err(ZipError::Malformed(_))));
        let z = write_zip(&sample()).unwrap();
        assert!(read_zip(&z[..z.len() - 10]).is_err());
    }

    #[test]
    fn backslashes_in_names_are_normalised() {
        let z = write_zip(&[ZipEntry { name: "a\\b.txt".into(), data: b"x".to_vec() }]).unwrap();
        assert_eq!(read_zip(&z).unwrap()[0].name, "a/b.txt");
    }
}
