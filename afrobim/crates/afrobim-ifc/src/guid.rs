//! Deterministic IFC GlobalIds.
//!
//! An `IfcGloballyUniqueId` is a 128-bit UUID written as 22 characters of a base-64 alphabet
//! (`0-9A-Za-z_$`); the first character carries 2 bits, the others 6 bits each. The UUID here is
//! an RFC 4122 version 5 (SHA-1, name-based) identifier, so the same name always gives the same
//! GlobalId and re-exporting a document does not change the identity of its objects.

/// Namespace of every AfroBIM name-based UUID (itself a random version 4 UUID, fixed forever).
const NAMESPACE: [u8; 16] =
    [0x6f, 0x1d, 0x8a, 0x52, 0x3c, 0x47, 0x4b, 0x9e, 0xa1, 0x0c, 0x5e, 0x2b, 0x7d, 0x94, 0x33, 0xc8];

const ALPHABET: &[u8; 64] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz_$";

/// SHA-1 (FIPS 180-4). Used only to derive identifiers, never for security.
fn sha1(data: &[u8]) -> [u8; 20] {
    let mut h: [u32; 5] = [0x6745_2301, 0xEFCD_AB89, 0x98BA_DCFE, 0x1032_5476, 0xC3D2_E1F0];
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&((data.len() as u64) * 8).to_be_bytes());
    for block in msg.chunks_exact(64) {
        let mut w = [0u32; 80];
        for (i, word) in block.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = h;
        for (i, wi) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | (!b & d), 0x5A82_7999),
                20..=39 => (b ^ c ^ d, 0x6ED9_EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1B_BCDC),
                _ => (b ^ c ^ d, 0xCA62_C1D6),
            };
            let t = a.rotate_left(5).wrapping_add(f).wrapping_add(e).wrapping_add(k).wrapping_add(*wi);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = t;
        }
        for (hv, v) in h.iter_mut().zip([a, b, c, d, e]) {
            *hv = hv.wrapping_add(v);
        }
    }
    let mut out = [0u8; 20];
    for (i, v) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&v.to_be_bytes());
    }
    out
}

/// The RFC 4122 version 5 UUID of `name` in the AfroBIM namespace.
pub(crate) fn uuid_v5(name: &str) -> [u8; 16] {
    let mut input = NAMESPACE.to_vec();
    input.extend_from_slice(name.as_bytes());
    let digest = sha1(&input);
    let mut uuid = [0u8; 16];
    uuid.copy_from_slice(&digest[..16]);
    uuid[6] = (uuid[6] & 0x0F) | 0x50; // version 5
    uuid[8] = (uuid[8] & 0x3F) | 0x80; // RFC 4122 variant
    uuid
}

/// The 22-character IFC form of a UUID.
pub(crate) fn compress(uuid: [u8; 16]) -> String {
    let n = u128::from_be_bytes(uuid);
    (0..22).map(|i| ALPHABET[((n >> (6 * (21 - i))) & 0x3F) as usize] as char).collect()
}

/// The deterministic IFC GlobalId for a name. The same name always gives the same id.
///
/// [`crate::write_ifc`] names its objects `"<project name>/<role>"`, for example
/// `"A104 signs/element/7"` for the element with id 7.
pub fn global_id(name: &str) -> String {
    compress(uuid_v5(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn sha1_matches_the_published_test_vectors() {
        assert_eq!(hex(&sha1(b"abc")), "a9993e364706816aba3e25717850c26c9cd0d89d");
        assert_eq!(hex(&sha1(b"")), "da39a3ee5e6b4b0d3255bfef95601890afd80709");
        assert_eq!(
            hex(&sha1(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq")),
            "84983e441c3bd26ebaae4aa1f95129e5e54670f1"
        );
    }

    #[test]
    fn compression_matches_the_ifc_encoding() {
        assert_eq!(compress([0; 16]), "0000000000000000000000");
        assert_eq!(compress([0xFF; 16]), "3$$$$$$$$$$$$$$$$$$$$$");
        let mut one = [0u8; 16];
        one[15] = 1;
        assert_eq!(compress(one), "0000000000000000000001");
    }

    #[test]
    fn a_uuid_is_version_5_with_the_rfc_variant() {
        let u = uuid_v5("x");
        assert_eq!(u[6] >> 4, 5);
        assert_eq!(u[8] >> 6, 0b10);
    }
}
