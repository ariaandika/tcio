
const fn ascii_to_int(b: u8) -> Option<usize> {
    match b {
        b'0' => Some(0),
        b'1' => Some(1),
        b'2' => Some(2),
        b'3' => Some(3),
        b'4' => Some(4),
        b'5' => Some(5),
        b'6' => Some(6),
        b'7' => Some(7),
        b'8' => Some(8),
        b'9' => Some(9),
        _ => None,
    }
}

/// `usize::MAX.to_string().len()`
const USIZE_MAX_CH: usize = b"18446744073709551615".len();

/// Parse ascii to integer.
pub const fn atoi(text: &[u8]) -> Option<usize> {
    if let Some(chunk) = text.first_chunk::<USIZE_MAX_CH>() {
        return if text.len() > USIZE_MAX_CH {
            None
        } else {
            atoi_max(chunk)
        };
    }

    let ptr = text.as_ptr();
    let max = text.len();
    let mut v = 0usize;
    let mut i = 0;
    loop {
        if i == max {
            break
        }
        // SAFETY: i < max (text.len())
        let b = unsafe { *ptr.add(i) };
        let Some(n) = ascii_to_int(b) else {
            return None;
        };
        // SAFETY: text.len() < USIZE_MAX_CH,
        // thus cannot overflow
        unsafe {
            v = v.unchecked_mul(10).unchecked_add(n);
            i = i.unchecked_add(1);
        }
    }
    Some(v)
}

const fn atoi_max(text: &[u8; USIZE_MAX_CH]) -> Option<usize> {
    let mut o = 0;
    let ptr = text.as_ptr();

    macro_rules! check {
        (2@ $i:expr,$p:expr => $m:literal) => {
            // SAFETY: exact size is known and max bounds is checked
            unsafe {
                let v = *ptr.add($i);
                let Some(n) = ascii_to_int(v) else {
                    return None
                };
                if n > $m {
                    return None;
                }
                o = usize::unchecked_add(o, usize::unchecked_mul(n, $p));
            }
        };

        (1@ $i:expr,$p:expr => $n:literal $($m:literal)*) => {
            check!(2@ $i,$p => $n);
            // recursive
            check!(1@ $i + 1,$p / 10 => $($m)*);
        };
        (1@ $i:expr,$p:expr =>) => {
            // base case
        };

        ($n:literal $($m:literal)*) => {
            check!(1@ 0, usize::pow(10, USIZE_MAX_CH as u32 - 1) => $n $($m)*);
        };
    }

    // 18446744073709551615
    check!(1 8 4 4 6 7 4 4 0 7 3 7 0 9 5 5 1 6 1 5);

    Some(o)
}

#[test]
fn test_atoi() {
    assert_eq!(Some(0), atoi(b"0"));
    assert_eq!(Some(1), atoi(b"1"));
    assert_eq!(Some(255), atoi(b"255"));
    assert_eq!(Some(1024), atoi(b"1024"));

    assert_eq!(Some(9999999999999999999), atoi(b"9999999999999999999"));
    assert_eq!(Some(18446744073709551615), atoi(b"18446744073709551615"));
    assert_eq!(Some(15), atoi(b"00000000000000000015"));

    assert!(atoi(b"00000000000000000006").is_none());
    assert!(atoi(b"18446744073709551616").is_none());
    assert!(atoi(b"28446744073709551615").is_none());
    assert!(atoi(b"19446744073709551615").is_none());
    assert!(atoi(b"18546744073709551615").is_none());
    assert!(atoi(b"99999999999999999999").is_none());
    assert!(atoi(b"184467440737095516155").is_none());

    assert!(atoi(b"-1").is_none());
    assert!(atoi(b"foo").is_none());
}

