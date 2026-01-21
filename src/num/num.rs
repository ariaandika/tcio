const U64_MAX_BUF: &[u8; 20] = b"18446744073709551615";
const I64_MAX_BUF: &[u8; 20] = b"+9223372036854775807";
const U64_MAX_CH: usize = U64_MAX_BUF.len();
const I64_MAX_CH: usize = I64_MAX_BUF.len();

#[cfg(test)]
const I64_MIN_BUF: &[u8; 20] = b"-9223372036854775808";

/// Parse ASCII to unsigned integer.
///
/// Returns `None` if bytes is empty or its length more than maximum possible digit.
#[inline]
pub fn atou(bytes: &[u8]) -> Option<u64> {
    if (bytes.len() > U64_MAX_CH) | bytes.is_empty() {
        return None;
    }
    bytes
        .iter()
        .try_fold(0u64, |acc, next| match next.wrapping_sub(b'0') {
            b @ 0..=9 => acc.checked_mul(10).and_then(|n| n.checked_add(b as u64)),
            _ => None,
        })
}

/// Parse ASCII to unsigned integer, wrapping around at boundary integer.
///
/// Returns `None` if input contains non-ASCII digit.
///
/// Note that this function will returns `0` if the bytes is empty.
///
/// This funtion can be used when the input bytes length is known to be less than possible value
/// where it will overflow, thus parser can skips overflow checks resulting in better performance.
///
/// The maximum bytes length that may result in overflow is 20.
#[inline]
pub fn wrapping_atou(bytes: &[u8]) -> Option<u64> {
    bytes
        .iter()
        .try_fold(0u64, |acc, next| match next.wrapping_sub(b'0') {
            b @ 0..=9 => Some(acc.wrapping_mul(10).wrapping_add(b as u64)),
            _ => None,
        })
}

/// Parse ASCII to signed integer.
///
/// Returns `None` if bytes is empty, contains non-ASCII, non-sign prefix or resulting integer
/// overflowed.
pub fn atoi(bytes: &[u8]) -> Option<i64> {
    if bytes.len() > I64_MAX_CH {
        return None;
    }
    let (sign, bytes) = match bytes.split_first() {
        Some((b'+' | b'-', [])) | None => return None,
        // b'+' => -(-1)
        // b'-' => -(1)
        Some(sign @ (b'+' | b'-', rest)) => (-(*sign.0 as i8).wrapping_sub(0x2C), rest),
        Some(_) => (1, bytes),
    };
    bytes
        .iter()
        .try_fold(0i64, |acc, next| match next.wrapping_sub(b'0') as i8 {
            b @ 0..=9 => acc
                .checked_mul(10)
                .and_then(|n| n.checked_add((b * sign) as i64)),
            _ => None,
        })
}

#[test]
fn test_atou() {
    assert_eq!(Some(0), atou(b"0"));
    assert_eq!(Some(1), atou(b"1"));
    assert_eq!(Some(255), atou(b"255"));
    assert_eq!(Some(1024), atou(b"1024"));
    assert_eq!(Some(9999999999999999999), atou(b"9999999999999999999"));
    assert_eq!(Some(u64::MAX), atou(U64_MAX_BUF));
    assert_eq!(Some(15), atou(b"00000000000000000015"));
    assert_eq!(Some(6), atou(b"00000000000000000006"));

    assert!(atou(&[]).is_none());
    assert!(atou(&[0; 5]).is_none());
    assert!(atou(&[255; 5]).is_none());
    assert!(atou(&[0; U64_MAX_CH]).is_none());
    assert!(atou(&[255; U64_MAX_CH]).is_none());
    assert!(atou(b"-1").is_none());
    assert!(atou(b"foo").is_none());
    assert!(atou(b"184467440737095516155").is_none());
    assert!(atou(b"18446744073709551616").is_none());
    assert!(atou(b"28446744073709551615").is_none());
    assert!(atou(b"19446744073709551615").is_none());
    assert!(atou(b"18546744073709551615").is_none());
    assert!(atou(b"99999999999999999999").is_none());
}

#[test]
fn test_wrapping_atou() {
    assert_eq!(Some(0), wrapping_atou(b"0"));
    assert_eq!(Some(1), wrapping_atou(b"1"));
    assert_eq!(Some(255), wrapping_atou(b"255"));
    assert_eq!(Some(1024), wrapping_atou(b"1024"));
    assert_eq!(Some(9999999999999999999), wrapping_atou(b"9999999999999999999"));
    assert_eq!(Some(u64::MAX), wrapping_atou(U64_MAX_BUF));
    assert_eq!(Some(15), wrapping_atou(b"00000000000000000015"));
    assert_eq!(Some(6), wrapping_atou(b"00000000000000000006"));

    // special case
    assert_eq!(wrapping_atou(&[]), Some(0));

    // wrapped
    assert!(wrapping_atou(b"184467440737095516155").is_some());
    assert!(wrapping_atou(b"18446744073709551616").is_some());
    assert!(wrapping_atou(b"28446744073709551615").is_some());
    assert!(wrapping_atou(b"19446744073709551615").is_some());
    assert!(wrapping_atou(b"18546744073709551615").is_some());
    assert!(wrapping_atou(b"99999999999999999999").is_some());

    assert!(wrapping_atou(&[0; 5]).is_none());
    assert!(wrapping_atou(&[255; 5]).is_none());
    assert!(wrapping_atou(&[0; U64_MAX_CH]).is_none());
    assert!(wrapping_atou(&[255; U64_MAX_CH]).is_none());
    assert!(wrapping_atou(b"-1").is_none());
    assert!(wrapping_atou(b"foo").is_none());
}

#[test]
fn test_atoi() {
    assert_eq!(Some(0), atoi(b"0"));
    assert_eq!(Some(0), atoi(b"+0"));
    assert_eq!(Some(0), atoi(b"-0"));
    assert_eq!(Some(1), atoi(b"1"));
    assert_eq!(Some(1), atoi(b"+1"));
    assert_eq!(Some(-1), atoi(b"-1"));
    assert_eq!(Some(255), atoi(b"255"));
    assert_eq!(Some(255), atoi(b"+255"));
    assert_eq!(Some(-255), atoi(b"-255"));
    assert_eq!(Some(1024), atoi(b"1024"));
    assert_eq!(Some(1024), atoi(b"+1024"));
    assert_eq!(Some(-1024), atoi(b"-1024"));
    assert_eq!(Some(8), atoi(b"0000000000000000008"));
    assert_eq!(Some(999999999999999999), atoi(b"999999999999999999"));
    assert_eq!(Some(999999999999999999), atoi(b"+999999999999999999"));
    assert_eq!(Some(-999999999999999999), atoi(b"-999999999999999999"));
    assert_eq!(Some(i64::MAX), atoi(I64_MAX_BUF));
    assert_eq!(Some(i64::MIN), atoi(I64_MIN_BUF));
    assert_eq!(Some(-9223372036854775807), atoi(b"-9223372036854775807"));
    assert_eq!(Some(-15), atoi(b"-000000000000000015"));

    assert!(atoi(&[]).is_none());
    assert!(atoi(&[0; 5]).is_none());
    assert!(atoi(&[255; 5]).is_none());
    assert!(atoi(&[0; I64_MAX_CH]).is_none());
    assert!(atoi(&[255; I64_MAX_CH]).is_none());
    assert!(atoi(b"foo").is_none());
    assert!(atoi(b"-").is_none());
    assert!(atoi(b"+").is_none());
    assert!(atoi(b"92233720368547758077").is_none());
    assert!(atoi(b"9223372036854775808").is_none());
    assert!(atoi(b"9323372036854775807").is_none());
    assert!(atoi(b"9233372036854775807").is_none());
    assert!(atoi(b"9224372036854775807").is_none());
    assert!(atoi(b"9223472036854775807").is_none());
    assert!(atoi(b"9223382036854775807").is_none());
}
