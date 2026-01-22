
macro_rules! method {
    ($name:ident, $ty:ident::$f:ident, $doc:literal) => {
        #[doc = concat!("Get `", stringify!($ty), "` in ", $doc)]
        fn $name(&mut self) -> Option<$ty> {
            const SIZE: usize = size_of::<$ty>();
            self.next_chunk::<SIZE>().map($ty::$f)
        }
    };
}

/// An extension trait for reading raw bytes.
///
/// This trait can be thought as an [`Iterator`] over bytes.
pub trait BytesExt {
    /// Take `N` length chunk and advance the bytes by `N`.
    fn next_chunk<const N: usize>(&mut self) -> Option<[u8; N]>;

    method!(get_u8, u8::from_be_bytes, "big endian");
    method!(get_u8_le, u8::from_le_bytes, "little endian");
    method!(get_u8_ne, u8::from_ne_bytes, "native endian");
    method!(get_i8, i8::from_be_bytes, "big endian");
    method!(get_i8_le, i8::from_le_bytes, "little endian");
    method!(get_i8_ne, i8::from_ne_bytes, "native endian");

    method!(get_u16, u16::from_be_bytes, "big endian");
    method!(get_u16_le, u16::from_le_bytes, "little endian");
    method!(get_u16_ne, u16::from_ne_bytes, "native endian");
    method!(get_i16, i16::from_be_bytes, "big endian");
    method!(get_i16_le, i16::from_le_bytes, "little endian");
    method!(get_i16_ne, i16::from_ne_bytes, "native endian");

    method!(get_u32, u32::from_be_bytes, "big endian");
    method!(get_u32_le, u32::from_le_bytes, "little endian");
    method!(get_u32_ne, u32::from_ne_bytes, "native endian");
    method!(get_i32, i32::from_be_bytes, "big endian");
    method!(get_i32_le, i32::from_le_bytes, "little endian");
    method!(get_i32_ne, i32::from_ne_bytes, "native endian");

    method!(get_u64, u64::from_be_bytes, "big endian");
    method!(get_u64_le, u64::from_le_bytes, "little endian");
    method!(get_u64_ne, u64::from_ne_bytes, "native endian");
    method!(get_i64, i64::from_be_bytes, "big endian");
    method!(get_i64_le, i64::from_le_bytes, "little endian");
    method!(get_i64_ne, i64::from_ne_bytes, "native endian");

    method!(get_u128, u128::from_be_bytes, "big endian");
    method!(get_u128_le, u128::from_le_bytes, "little endian");
    method!(get_u128_ne, u128::from_ne_bytes, "native endian");
    method!(get_i128, i128::from_be_bytes, "big endian");
    method!(get_i128_le, i128::from_le_bytes, "little endian");
    method!(get_i128_ne, i128::from_ne_bytes, "native endian");
}

impl BytesExt for &[u8] {
    fn next_chunk<const N: usize>(&mut self) -> Option<[u8; N]> {
        let (&lead, rest) = self.split_first_chunk::<N>()?;
        *self = rest;
        Some(lead)
    }

    fn get_u8(&mut self) -> Option<u8> {
        let (&lead, rest) = self.split_first()?;
        *self = rest;
        Some(lead)
    }
}

impl BytesExt for crate::bytes::Bytes {
    fn next_chunk<const N: usize>(&mut self) -> Option<[u8; N]> {
        let &lead = self.first_chunk::<N>()?;
        unsafe { self.advance_unchecked(N) };
        Some(lead)
    }

    fn get_u8(&mut self) -> Option<u8> {
        let &lead = self.first()?;
        unsafe { self.advance_unchecked(1) };
        Some(lead)
    }
}

impl BytesExt for crate::bytes::BytesMut {
    fn next_chunk<const N: usize>(&mut self) -> Option<[u8; N]> {
        let &lead = self.first_chunk::<N>()?;
        unsafe { self.advance_unchecked(N) };
        Some(lead)
    }

    fn get_u8(&mut self) -> Option<u8> {
        let &lead = self.first()?;
        unsafe { self.advance_unchecked(1) };
        Some(lead)
    }
}
