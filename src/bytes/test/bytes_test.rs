use crate::bytes::{Buf, Bytes, BytesMut};

const DATA: &[u8] = &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];

type Cx = (Bytes, &'static [u8]);

// ===== Constructor =====

fn statics() -> Cx {
    (Bytes::from_static(DATA), DATA)
}

fn owned() -> Cx {
    (Bytes::copy_from_slice(DATA), DATA)
}

fn shared() -> Cx {
    let bytes = Bytes::copy_from_slice(DATA);
    let _ = bytes.clone();
    (bytes, DATA)
}

fn from_vec() -> Cx {
    (Bytes::from(DATA.to_vec()), DATA)
}

fn from_vec_excess() -> Cx {
    let mut vec = DATA.to_vec();
    vec.truncate(12);
    (Bytes::from(vec), &DATA[..12])
}

// ===== Destructor =====

fn dropped((bytes, expect): Cx) {
    assert_eq!(bytes.as_slice(), expect);
    drop(bytes);
}

fn into_vec((bytes, expect): Cx) {
    let vec = Vec::from(bytes);
    assert_eq!(vec.as_slice(), expect);
}

fn into_mut((bytes, expect): Cx) {
    let buf = BytesMut::from(bytes);
    assert_eq!(buf.as_slice(), expect);
}

// ===== Behavior =====

fn advancing((mut bytes, expect): Cx) -> Cx {
    bytes.advance(2);
    assert_eq!(bytes.as_slice(), &expect[2..]);
    (bytes, &expect[2..])
}

fn advancing_full((mut bytes, _): Cx) -> Cx {
    bytes.advance(bytes.len());
    assert_eq!(bytes.as_slice(), &[][..]);
    (bytes, &[][..])
}

fn truncating((mut bytes, expect): Cx) -> Cx {
    bytes.truncate(bytes.len() - 2);
    assert_eq!(bytes.as_slice(), &expect[..expect.len() - 2]);
    (bytes, &expect[..expect.len() - 2])
}

fn truncating_empty((mut bytes, _): Cx) -> Cx {
    bytes.truncate(0);
    assert_eq!(bytes.as_slice(), &[][..]);
    (bytes, &[][..])
}

fn splitting_off((mut bytes, expect): Cx) -> Cx {
    let split = bytes.split_off(6);
    assert_eq!(bytes.as_slice(), &expect[..6]);
    assert_eq!(split.as_slice(), &expect[6..]);
    (bytes, &expect[..6])
}

fn splitting_to((mut bytes, expect): Cx) -> Cx {
    let split = bytes.split_to(2);
    assert_eq!(bytes.as_slice(), &expect[2..]);
    assert_eq!(split.as_slice(), &expect[..2]);
    (bytes, &expect[2..])
}

// ===== Test =====

macro_rules! behavior {
    (@G1 $b1:ident; $c:ident,$d:ident) => {
        $d($b1($c()));
        $d(advancing_full($b1($c())));
        $d(truncating_empty($b1($c())));
    };
    (@G2 $b1:ident; $b2:ident; $c:ident,$d:ident) => {
        $d($b2($b1($c())));
        $d(advancing_full($b2($b1($c()))));
        $d(truncating_empty($b2($b1($c()))));
    };

    (@B1 $b1:ident; $c:ident,$d:ident) => {
        behavior!(@G1 $b1; $c,$d);
    };
    (@B1 $b1:ident, $($br1:ident),*; $c:ident,$d:ident) => {
        behavior!(@G1 $b1; $c,$d);
        behavior!(@B1 $($br1),*; $c,$d);
    };

    (@B2
     $b1:ident; $b2:ident; $($all:ident),*;
     $c:ident,$d:ident
    ) => {
        behavior!(@G2 $b1; $b2; $c,$d);
    };
    (@B2
     $b1:ident; $b2:ident, $($br2:ident),*; $($all:ident),*;
     $c:ident,$d:ident
    ) => {
        behavior!(@G2 $b1; $b2; $c,$d);
        behavior!(@B2 $b1; $($br2),*; $($all),*; $c,$d);
    };
    (@B2
     $b1:ident, $($br1:ident),*; $b2:ident; $($all:ident),*;
     $c:ident,$d:ident
    ) => {
        behavior!(@G2 $b1; $b2; $c,$d);
        behavior!(@B2 $($br1),*; $($all),*; $($all),*; $c,$d);
    };
    (@B2
     $b1:ident, $($br1:ident),*; $b2:ident, $($br2:ident),*; $($all:ident),*;
     $c:ident,$d:ident
    ) => {
        behavior!(@G2 $b1; $b2; $c,$d);
        behavior!(@B2 $b1, $($br1),*; $($br2),*; $($all),*; $c,$d);
    };

    ($($b:ident),*; $c:ident,$d:ident) => {
        behavior!(@B1 $($b),*; $c,$d);
        behavior!(@B2 $($b),*;$($b),*;$($b),*; $c,$d);
    };
    ($ctor:ident, $dtor:ident) => {
        behavior!(
            advancing, truncating, splitting_off, splitting_to;
            $ctor, $dtor
        );
    };
}

// ===== Statics =====

#[test]
fn test_statics_dropped() {
    behavior!(statics, dropped);
}

#[test]
fn test_statics_into_vec() {
    behavior!(statics, into_vec);
}

#[test]
fn test_statics_into_mut() {
    behavior!(statics, into_mut);
}

// ===== Owned =====

#[test]
fn test_owned_dropped() {
    behavior!(owned, dropped);
}

#[test]
fn test_owned_into_vec() {
    behavior!(owned, into_vec);
}

#[test]
fn test_owned_into_mut() {
    behavior!(owned, into_mut);
}

// ===== Shared =====

#[test]
fn test_shared_dropped() {
    behavior!(shared, dropped);
}

#[test]
fn test_shared_into_vec() {
    behavior!(shared, into_vec);
}

#[test]
fn test_shared_into_mut() {
    behavior!(shared, into_mut);
}

// ===== From<Vec<u8>> =====

#[test]
fn test_from_vec_dropped() {
    behavior!(from_vec, dropped);
}

#[test]
fn test_from_vec_into_vec() {
    behavior!(from_vec, into_vec);
}

#[test]
fn test_from_vec_into_mut() {
    behavior!(from_vec, into_mut);
}

// ===== From<Vec<u8>> Excess =====

#[test]
fn test_from_vec_excess_dropped() {
    behavior!(from_vec_excess, dropped);
}

#[test]
fn test_from_vec_excess_into_vec() {
    behavior!(from_vec_excess, into_vec);
}

#[test]
fn test_from_vec_excess_into_mut() {
    behavior!(from_vec_excess, into_mut);
}

