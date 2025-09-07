use super::{Bytes, BytesMut, Cursor};

/// Integration for [`Cursor`] with other buffer types.
///
/// This struct created by calling `.cursor()` on specific types, such as [`Bytes::cursor`].
#[derive(Debug)]
pub struct CursorBuf<T> {
    // static lifetime for workaround with self referencing struct, implicitly `cursor` have shared
    // reference to `buf`
    cursor: Cursor<'static>,
    // Note that we must not expose mutable reference to the underlying buffer because `cursor` is
    // implicitly have shared reference to buffer.
    buf: T,
}

macro_rules! delegate_cursor {
    {
        impl<$($lf:lifetime),*> $ty2:ty; $($tt:tt)*
    } => {
        impl<'a,$($lf),*> CursorBuf<&'a mut $ty2> {
            #[inline]
            pub(crate) const fn shared_mut(bytes: &'a mut $ty2) -> Self {
                Self {
                    cursor: Cursor::new_unbound(bytes$($tt)*),
                    buf: bytes,
                }
            }
        }

        impl<'a,$($lf),*> From<&'a mut $ty2> for CursorBuf<&'a mut $ty2> {
            #[inline]
            fn from(value: &'a mut $ty2) -> Self {
                Self::shared_mut(value)
            }
        }

        impl<'a,$($lf),*> std::ops::Deref for CursorBuf<&'a mut $ty2> {
            type Target = Cursor<'a>;

            #[inline]
            fn deref(&self) -> &Self::Target {
                &self.cursor
            }
        }

        impl<'a,$($lf),*> std::ops::DerefMut for CursorBuf<&'a mut $ty2> {
            #[inline]
            fn deref_mut(&mut self) -> &mut Self::Target {
                // SAFETY: static lifetime for workaround with self referencing struct, implicitly
                // `cursor` have shared reference to `buf` which equal to the generic type
                unsafe {
                    std::mem::transmute::<&mut Cursor<'static>, &mut Self::Target>(
                        &mut self.cursor
                    )
                }
            }
        }
    };
    {
        impl $ty2:ty; $($tt:tt)*
    } => {
        delegate_cursor!(impl<> $ty2; $($tt)*);
    };
}

macro_rules! delegate_bytes {
    {
        impl $ty2:ty;
    } => {
        impl<'a> CursorBuf<&'a mut $ty2> {
            // ===== Delegate method from Bytes/BytesMut =====

            /// [`advance()`][Buf::advance] the contained buffer based on current cursor.
            ///
            /// The underlying [`Cursor`] is reset reflecting the advanced buffer.
            #[inline]
            pub fn advance_buf(&mut self) {
                // SAFETY: `cursor.steps()` is less than or equal to bytes length
                unsafe {
                    <$ty2>::advance_unchecked(&mut self.buf, self.cursor.steps());
                }
                self.cursor = Cursor::new_unbound(self.buf.as_slice());
            }

            /// Split the contained buffer based on current cursor.
            ///
            /// The underlying [`Cursor`] then will be at the start of the buffer.
            #[inline]
            pub fn split_to(&mut self) -> $ty2 {
                let bytes = self.buf.split_to(self.cursor.steps());
                self.cursor = Cursor::new_unbound(self.buf.as_slice());
                bytes
            }

            /// Split the contained buffer based on current cursor.
            ///
            /// The underlying [`Cursor`] then will be at the end of the buffer.
            #[inline]
            pub fn split_off(&mut self) -> $ty2 {
                let bytes = self.buf.split_off(self.cursor.steps());
                self.cursor = Cursor::from_end_unbound(self.buf.as_slice());
                bytes
            }

            /// Truncate the contained buffer.
            ///
            /// The underlying [`Cursor`] then will be at the end of the buffer.
            #[inline]
            pub fn truncate_buf(&mut self) {
                self.buf.truncate(self.cursor.steps());
                self.cursor = Cursor::from_end_unbound(self.buf.as_slice());
            }
        }
    };
}

// ===== impl =====

delegate_cursor! {
    impl Bytes;
    .as_slice()
}

delegate_bytes! {
    impl Bytes;
}

delegate_cursor! {
    impl BytesMut;
    .as_slice()
}

delegate_bytes! {
    impl BytesMut;
}

delegate_cursor! {
    impl<'b> &'b [u8];
}

impl<'a, 'b> CursorBuf<&'a mut &'b [u8]> {
    /// Create [`CursorBuf`] from mutable shared buffer.
    #[inline]
    pub const fn from_slice(bytes: &'a mut &'b [u8]) -> Self {
        CursorBuf {
            cursor: Cursor::new_unbound(bytes),
            buf: bytes,
        }
    }

    /// [`advance()`][Buf::advance] the contained buffer based on current cursor.
    ///
    /// The underlying [`Cursor`] is reset reflecting the advanced buffer.
    #[inline]
    pub const fn advance_buf(&mut self) {
        *self.buf = self.cursor.as_slice();
        self.cursor = Cursor::new_unbound(self.buf);
    }
}

impl<T> CursorBuf<T> {
    /// Consume the `CursorBuf`, returning the contained buffer.
    #[inline]
    pub fn into_inner(self) -> T {
        self.buf
    }
}
