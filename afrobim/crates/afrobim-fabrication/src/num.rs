// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Number formatting as the C# reference writes it.
//!
//! The C# code formats with custom patterns such as `"0.####"` and `"0.000"` in the invariant
//! culture. .NET takes a `double` to 15 significant digits first and then rounds half away from
//! zero to the pattern's decimals, so 1790.78125 becomes `1790.7813` and 2614.8437499999995
//! becomes `2614.8438`. Rounding the binary value directly would give `...12` and `...37`.

/// `v` with at least `min_decimals` and at most `max_decimals` decimals: the C# pattern
/// `"0." + "0" * min + "#" * (max - min)`.
pub fn fixed(v: f64, min_decimals: usize, max_decimals: usize) -> String {
    debug_assert!(min_decimals <= max_decimals);
    if !v.is_finite() {
        return if v.is_nan() { "NaN".to_string() } else if v > 0.0 { "Infinity".to_string() } else { "-Infinity".to_string() };
    }
    let (mut digits, mut int_len): (Vec<u8>, i32) = if v == 0.0 {
        (Vec::new(), 0)
    } else {
        // 15 significant digits: "d.dddddddddddddde±x".
        let sci = format!("{:.14e}", v.abs());
        let (mantissa, exp) = sci.split_once('e').expect("scientific format has an exponent");
        let exp: i32 = exp.parse().expect("exponent is an integer");
        (mantissa.bytes().filter(u8::is_ascii_digit).map(|b| b - b'0').collect(), exp + 1)
    };
    // `int_len` is the number of digits before the decimal point (≤ 0 for small values).
    if int_len < 0 {
        let pad = (-int_len) as usize;
        digits.splice(0..0, std::iter::repeat_n(0, pad));
        int_len = 0;
    }
    let keep = int_len as usize + max_decimals;
    while digits.len() < keep + 1 {
        digits.push(0);
    }
    let round_up = digits[keep] >= 5;
    digits.truncate(keep);
    if round_up {
        let mut i = digits.len();
        loop {
            if i == 0 {
                digits.insert(0, 1);
                int_len += 1;
                break;
            }
            i -= 1;
            if digits[i] == 9 {
                digits[i] = 0;
            } else {
                digits[i] += 1;
                break;
            }
        }
    }
    let split = int_len as usize;
    let mut out = String::new();
    // .NET Core 3.0 and later keep the sign of a negative value that rounds to zero ("-0").
    if v < 0.0 {
        out.push('-');
    }
    if split == 0 {
        out.push('0');
    }
    out.extend(digits[..split].iter().map(|d| (d + b'0') as char));
    let mut frac: Vec<u8> = digits[split..].to_vec();
    while frac.len() > min_decimals && frac.last() == Some(&0) {
        frac.pop();
    }
    if !frac.is_empty() {
        out.push('.');
        out.extend(frac.iter().map(|d| (d + b'0') as char));
    }
    out
}

/// The C# `"0.####"` pattern, the one every DXF coordinate uses.
pub fn four(v: f64) -> String {
    fixed(v, 0, 4)
}

/// Length of a string in UTF-16 code units, which is what C# `string.Length` counts.
pub fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}

/// The first `n` UTF-16 code units of `s` (C# `s[..n]`).
pub fn utf16_prefix(s: &str, n: usize) -> String {
    let units: Vec<u16> = s.encode_utf16().take(n).collect();
    String::from_utf16_lossy(&units)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn four_decimals_without_trailing_zeros() {
        assert_eq!(four(0.0), "0");
        assert_eq!(four(12.5), "12.5");
        assert_eq!(four(8.12345678), "8.1235");
        assert_eq!(four(-7.25), "-7.25");
        assert_eq!(four(100.00004), "100");
        assert_eq!(four(1790.78125), "1790.7813");
        assert_eq!(four(0.99996), "1");
        assert_eq!(four(9.99995), "10");
        assert_eq!(four(-0.00001), "-0");
        assert_eq!(four(2614.8437499999995), "2614.8438");
        assert_eq!(four(0.00005), "0.0001");
        assert_eq!(four(0.00004), "0");
        assert_eq!(four(123456.0), "123456");
    }

    #[test]
    fn fixed_decimals_are_padded() {
        assert_eq!(fixed(6.0468, 3, 3), "6.047");
        assert_eq!(fixed(0.5625, 3, 3), "0.563");
        assert_eq!(fixed(10.2, 3, 3), "10.200");
        assert_eq!(fixed(0.0, 3, 3), "0.000");
        assert_eq!(fixed(2303.75, 0, 1), "2303.8");
        assert_eq!(fixed(2300.0, 0, 1), "2300");
        assert_eq!(fixed(749.96, 0, 0), "750");
        assert_eq!(fixed(0.4, 0, 0), "0");
        assert_eq!(fixed(31.25, 0, 3), "31.25");
        assert_eq!(fixed(12.34, 1, 1), "12.3");
        assert_eq!(fixed(0.05, 1, 1), "0.1");
    }

    #[test]
    fn utf16_helpers_count_code_units() {
        assert_eq!(utf16_len("abc"), 3);
        assert_eq!(utf16_len("é½"), 2);
        assert_eq!(utf16_len("😀"), 2);
        assert_eq!(utf16_prefix("abcdef", 3), "abc");
    }
}
