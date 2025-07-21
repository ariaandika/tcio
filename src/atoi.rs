const U64_MAX_BUF: &[u8; 20] = b"18446744073709551615";
const U64_MAX_CH: usize = U64_MAX_BUF.len();

/// Parse ascii to unsigned integer.
pub const fn atou(text: &[u8]) -> Option<u64> {
    if text.len() > U64_MAX_CH || text.is_empty() {
        return None;
    }

    if let Some(chunk) = text.first_chunk::<U64_MAX_CH>() {
        return atou_max(chunk);
    }

    let ptr = text.as_ptr();
    let max = text.len();
    let mut o = 0u64;
    let mut i = 0;

    while i < max {
        unsafe {
            // SAFETY: i < text.len()
            let n = *ptr.add(i);
            if n < b'0' || n > b'9' {
                return None;
            }

            // SAFETY: `text.len()` is less than digit count of u64::MAX,
            // thus it cannot overflow
            o = o
                .unchecked_mul(10)
                .unchecked_add(u8::unchecked_sub(n, 48) as u64);

            // SAFETY: i < text.len()
            i = i.unchecked_add(1);
        }
    }

    Some(o)
}

const fn atou_max(text: &[u8; U64_MAX_CH]) -> Option<u64> {
    const U64_MAX_B_PTR: *const u8 = U64_MAX_BUF.as_ptr();

    // SAFETY: the first value of `ptr` is never get read
    let p1 = text.as_ptr();
    let mut o = 0u64;
    let mut i = 0;

    while i < U64_MAX_CH {
        unsafe {
            // SAFETY: i < U64_MAX_CH
            let n = *p1.add(i);

            if n < b'0' || n > *U64_MAX_B_PTR.add(i) {
                return None;
            }

            // SAFETY: multiply will only happens U64_MAX_CH-nth time, thus cannot overflow
            o = o
                .unchecked_mul(10)
                .unchecked_add(u8::unchecked_sub(n, 48) as u64);

            // SAFETY: i <U64_MAX_CH
            i = i.unchecked_add(1);
        }
    }

    Some(o)
}

const I64_MAX_BUF: &[u8; 19] = b"9223372036854775807";
const I64_MAX_CH: usize = I64_MAX_BUF.len();
const I64_MAX_B_PTR: *const u8 = I64_MAX_BUF.as_ptr();

const I64_MIN_BUF: &[u8] = b"-9223372036854775808";

/// Parse ascii to signed integer.
pub const fn atoi(text: &[u8]) -> Option<i64> {
    // this optimistic check will prevent diverge for the check below because the last digit for
    // i64::MIN is higher than i64::MAX
    if let I64_MIN_BUF = text {
        return Some(i64::MIN);
    }

    let (text, sign) = match text.first() {
        Some(&b @ (b'+' | b'-')) => (
            // SAFETY: text length is at least 1,
            // thus `text.len().unchecked_sub(1)` returns at least 0
            unsafe {
                std::slice::from_raw_parts(text.as_ptr().add(1), text.len().unchecked_sub(1))
            },
            if b == b'+' { 1 } else { -1 },
        ),
        Some(_) => (text, 1),
        None => return None,
    };

    if text.len() > I64_MAX_CH || text.is_empty() {
        return None;
    }

    let ptr = text.as_ptr();
    let max = text.len();
    let is_max = max == I64_MAX_CH;
    let mut o = 0i64;
    let mut i = 0;

    while i < max {
        unsafe {
            // SAFETY: i < text.len()
            let n = *ptr.add(i);

            //                              SAFETY: i < I64_MAX_CH
            if n < b'0' || n > (if is_max { *I64_MAX_B_PTR.add(i) } else { b'9' }) {
                return None;
            }

            // SAFETY: `text.len()` is less than digit count of i64::MAX, thus it cannot overflow
            // SAFETY `unchecked_sub`: n < b'0'
            o = o
                .unchecked_mul(10)
                .unchecked_add(u8::unchecked_sub(n, b'0') as i64);

            // SAFETY: i < text.len()
            i = i.unchecked_add(1);
        }
    }

    // SAFETY: `sign` is either 1 or -1
    Some(unsafe { o.unchecked_mul(sign) })
}

#[test]
fn test_atoi() {
    // atou

    assert_eq!(Some(0), atou(b"0"));
    assert_eq!(Some(1), atou(b"1"));
    assert_eq!(Some(255), atou(b"255"));
    assert_eq!(Some(1024), atou(b"1024"));
    assert_eq!(Some(9999999999999999999), atou(b"9999999999999999999"));

    assert!(atou(&[]).is_none());
    assert!(atou(&[0; 5]).is_none());
    assert!(atou(&[255; 5]).is_none());
    assert!(atou(b"-1").is_none());
    assert!(atou(b"foo").is_none());
    assert!(atou(b"184467440737095516155").is_none());

    // atou_max

    assert_eq!(Some(u64::MAX), atou(U64_MAX_BUF));
    assert_eq!(Some(15), atou(b"00000000000000000015"));

    assert!(atou(&[0; U64_MAX_CH]).is_none());
    assert!(atou(&[255; U64_MAX_CH]).is_none());
    assert!(atou(b"00000000000000000006").is_none());
    assert!(atou(b"18446744073709551616").is_none());
    assert!(atou(b"28446744073709551615").is_none());
    assert!(atou(b"19446744073709551615").is_none());
    assert!(atou(b"18546744073709551615").is_none());
    assert!(atou(b"99999999999999999999").is_none());

    // atoi

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
    assert_eq!(Some(999999999999999999), atoi(b"999999999999999999"));
    assert_eq!(Some(999999999999999999), atoi(b"+999999999999999999"));
    assert_eq!(Some(-999999999999999999), atoi(b"-999999999999999999"));

    assert!(atoi(&[]).is_none());
    assert!(atoi(&[0; 5]).is_none());
    assert!(atoi(&[255; 5]).is_none());
    assert!(atoi(b"foo").is_none());
    assert!(atoi(b"-").is_none());
    assert!(atoi(b"+").is_none());
    assert!(atoi(b"92233720368547758077").is_none());

    // atoi_max

    assert_eq!(Some(i64::MAX), atoi(I64_MAX_BUF));
    assert_eq!(Some(i64::MIN), atoi(I64_MIN_BUF));
    assert_eq!(Some(-9223372036854775807), atoi(b"-9223372036854775807"));
    assert_eq!(Some(-15), atoi(b"-000000000000000015"));

    assert!(atoi(&[0; 20]).is_none());
    assert!(atoi(&[255; 20]).is_none());
    assert!(atoi(b"0000000000000000008").is_none());
    assert!(atoi(b"9223372036854775808").is_none());
    assert!(atoi(b"9323372036854775807").is_none());
    assert!(atoi(b"9233372036854775807").is_none());
    assert!(atoi(b"9224372036854775807").is_none());
    assert!(atoi(b"9223472036854775807").is_none());
    assert!(atoi(b"9223382036854775807").is_none());
}
