const USIZE_MAX_BUF: &[u8; 20] = b"18446744073709551615";
const USIZE_MAX_CH: usize = USIZE_MAX_BUF.len();

/// Parse ascii to unsigned integer.
pub const fn atou(text: &[u8]) -> Option<usize> {
    if text.len() > USIZE_MAX_CH || text.is_empty() {
        return None;
    }

    if let Some(chunk) = text.first_chunk::<USIZE_MAX_CH>() {
        return atou_max(chunk);
    }

    let ptr = text.as_ptr();
    let max = text.len();
    let mut o = 0usize;
    let mut i = 0;

    while i < max {
        unsafe {
            // SAFETY: i < text.len()
            let n = *ptr.add(i);
            if n < b'0' || n > b'9' {
                return None;
            }

            // SAFETY: `text.len()` is less than digit count of usize::MAX,
            // thus it cannot overflow
            o = o
                .unchecked_mul(10)
                .unchecked_add(u8::unchecked_sub(n, 48) as usize);

            // SAFETY: i < text.len()
            i = i.unchecked_add(1);
        }
    }

    Some(o)
}

const fn atou_max(text: &[u8; USIZE_MAX_CH]) -> Option<usize> {
    const USIZE_MAX_B_PTR: *const u8 = USIZE_MAX_BUF.as_ptr();

    // SAFETY: the first value of `ptr` is never get read
    let p1 = text.as_ptr();
    let mut o = 0usize;
    let mut i = 0;

    while i < USIZE_MAX_CH {
        unsafe {
            // SAFETY: i < USIZE_MAX_CH
            let n = *p1.add(i);

            if n < b'0' || n > *USIZE_MAX_B_PTR.add(i) {
                return None;
            }

            // SAFETY: multiply will only happens USIZE_MAX_CH-nth time, thus cannot overflow
            o = o
                .unchecked_mul(10)
                .unchecked_add(u8::unchecked_sub(n, 48) as usize);

            // SAFETY: i < USIZE_MAX_CH
            i = i.unchecked_add(1);
        }
    }

    Some(o)
}

#[test]
fn test_atoi() {
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

    // atoi_max

    assert_eq!(Some(18446744073709551615), atou(b"18446744073709551615"));
    assert_eq!(Some(15), atou(b"00000000000000000015"));

    assert!(atou(&[0; 20]).is_none());
    assert!(atou(&[255; 20]).is_none());
    assert!(atou(b"00000000000000000006").is_none());
    assert!(atou(b"18446744073709551616").is_none());
    assert!(atou(b"28446744073709551615").is_none());
    assert!(atou(b"19446744073709551615").is_none());
    assert!(atou(b"18546744073709551615").is_none());
    assert!(atou(b"99999999999999999999").is_none());
}
