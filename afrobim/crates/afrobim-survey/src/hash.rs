// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! SHA-256 (FIPS 180-4) for import event file hashes. The round constants K are the first 32 bits of
//! the fractional parts of the cube roots of the first 64 primes and the initial hash values H are the
//! same for the square roots of the first 8 primes (FIPS 180-4 sections 4.2.2 and 5.3.3); they are
//! computed here instead of typed. Correctness is checked in the tests against the published test
//! vectors for "" and "abc".

fn primes(n: usize) -> Vec<u64> {
    let mut p = Vec::new();
    let mut c = 2u64;
    while p.len() < n {
        if p.iter().all(|q| c % q != 0) {
            p.push(c);
        }
        c += 1;
    }
    p
}

fn frac32(x: f64) -> u32 {
    ((x - x.floor()) * 4294967296.0).floor() as u32
}

pub fn sha256_hex(data: &[u8]) -> String {
    let pr = primes(64);
    let k: Vec<u32> = pr.iter().map(|&p| frac32((p as f64).cbrt())).collect();
    let mut h: Vec<u32> = pr[..8].iter().map(|&p| frac32((p as f64).sqrt())).collect();
    let mut msg = data.to_vec();
    let bitlen = (data.len() as u64) * 8;
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bitlen.to_be_bytes());
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([chunk[4 * i], chunk[4 * i + 1], chunk[4 * i + 2], chunk[4 * i + 3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let mut v = [h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]];
        for i in 0..64 {
            let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
            let ch = (v[4] & v[5]) ^ (!v[4] & v[6]);
            let t1 = v[7].wrapping_add(s1).wrapping_add(ch).wrapping_add(k[i]).wrapping_add(w[i]);
            let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
            let maj = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
            let t2 = s0.wrapping_add(maj);
            v[7] = v[6];
            v[6] = v[5];
            v[5] = v[4];
            v[4] = v[3].wrapping_add(t1);
            v[3] = v[2];
            v[2] = v[1];
            v[1] = v[0];
            v[0] = t1.wrapping_add(t2);
        }
        for i in 0..8 {
            h[i] = h[i].wrapping_add(v[i]);
        }
    }
    h.iter().map(|x| format!("{x:08x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn published_vectors() {
        // NIST FIPS 180-4 / CAVS example values
        assert_eq!(sha256_hex(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }
}
