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

fn truncating((mut bytes, expect): Cx) -> Cx {
    bytes.truncate_off(2);
    assert_eq!(bytes.as_slice(), &expect[..expect.len() - 2]);
    (bytes, &expect[..expect.len() - 2])
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
    (@G $t:ident; $u:ident; $c:ident,$d:ident) => {
        $d($u($t($c())));
    };

    (@C
     $t1:ident; $u1:ident; $($all:ident),*;
     $c:ident,$d:ident
    ) => {
        behavior!(@G $t1; $u1; $c,$d);
    };
    (@C
     $t1:ident; $u1:ident, $($u2:ident),*; $($all:ident),*;
     $c:ident,$d:ident
    ) => {
        behavior!(@G $t1; $u1; $c,$d);
        behavior!(@C $t1; $($u2),*; $($all),*; $c,$d);
    };
    (@C
     $t1:ident, $($t2:ident),*; $u1:ident; $($all:ident),*;
     $c:ident,$d:ident
    ) => {
        behavior!(@G $t1; $u1; $c,$d);
        behavior!(@C $($t2),*; $($all),*; $($all),*; $c,$d);
    };
    (@C
     $t1:ident, $($t2:ident),*; $u1:ident, $($u2:ident),*; $($all:ident),*;
     $c:ident,$d:ident
    ) => {
        behavior!(@G $t1; $u1; $c,$d);
        behavior!(@C $t1, $($t2),*; $($u2),*; $($all),*; $c,$d);
    };

    ($($t:ident),*; $c:ident,$d:ident) => {
        behavior!(@C $($t),*;$($t),*;$($t),*; $c,$d);
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
