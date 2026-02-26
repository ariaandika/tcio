use crate::bytes::{Buf, Bytes, BytesMut};

const DATA: &[u8] = &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];

type Cx<'a> = (BytesMut, &'a [u8]);

// ===== Constructor =====

fn owned() -> Cx<'static> {
    (BytesMut::copy_from_slice(DATA), DATA)
}

fn shared() -> Cx<'static> {
    let mut value = BytesMut::copy_from_slice(DATA);
    (value.split_off(2), &DATA[2..])
}

fn from_vec() -> Cx<'static> {
    (BytesMut::from(DATA.to_vec()), DATA)
}

fn from_vec_excess() -> Cx<'static> {
    let mut vec = DATA.to_vec();
    vec.truncate(12);
    (BytesMut::from(vec), &DATA[..12])
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

fn into_shared((bytes, expect): Cx) {
    let buf = Bytes::from(bytes);
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
    behavior!(owned, into_shared);
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
    behavior!(shared, into_shared);
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
    behavior!(from_vec, into_shared);
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
    behavior!(from_vec_excess, into_shared);
}

// ===== Allocation =====

#[test]
fn test_reserve() {
    let mut bytes = BytesMut::copy_from_slice(DATA);
    let base_ptr = bytes.as_ptr();
    let base_cap = bytes.capacity();

    // Offset Reclaim
    bytes.advance(12);
    assert_eq!(bytes.capacity(), base_cap - 12);
    bytes.reserve(12);
    // this ensure reclaim, if this would have reallocate, the capacity will be more excessive
    assert_eq!(bytes.capacity(), base_cap);
    bytes.as_slice();


    bytes.extend_from_slice(&DATA[..12]);
    assert_eq!(bytes.capacity(), base_cap);

    // Reallocate Owned
    bytes.reserve(4);
    assert!(bytes.capacity() > base_cap);
    let base_cap = bytes.capacity();
    bytes.as_slice();


    while let rem = bytes.capacity() - bytes.len() && rem != 0 {
        bytes.extend_from_slice(DATA.get(..rem).unwrap_or(DATA));
    }

    // Tail Reclaim
    bytes.split_off(bytes.len() - 4);
    assert_eq!(bytes.capacity(), base_cap - 4);
    bytes.reserve(4);
    assert_eq!(bytes.capacity(), base_cap);


    bytes.extend_from_slice(&DATA[..4]);
    // Reallocate Exclusive
    bytes.reserve(4);
    assert!(bytes.capacity() > base_cap);
    let base_cap = bytes.capacity();
    bytes.as_slice();


    while let rem = bytes.capacity() - bytes.len() && rem != 0 {
        bytes.extend_from_slice(DATA.get(..rem).unwrap_or(DATA));
    }
    assert_eq!(bytes.len(), bytes.capacity());

    // Reallocate Shared
    let vec = bytes.to_vec();
    let split = bytes.split_off(bytes.len() - 4);
    bytes.reserve(4);
    assert_ne!(bytes.as_ptr(), base_ptr);
    assert!(bytes.capacity() > base_cap);
    drop(split);
    assert_eq!(bytes.as_slice(), &vec[..vec.len() - 4]);
}
