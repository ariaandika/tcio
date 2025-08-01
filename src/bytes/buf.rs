
pub trait Buf {
    fn remaining(&self) -> usize;

    fn chunk(&self) -> &[u8];

    fn advance(&mut self, cnt: usize);

    fn has_remaining(&self) -> bool {
        self.remaining() > 0
    }

    fn copy_to_bytes(&mut self, len: usize) -> super::Bytes {
        if self.remaining() < len {
            panic!(
                "cannot get `{len}` bytes, only `{}` is remaining",
                self.remaining()
            )
        }

        let mut b = super::BytesMut::with_capacity(len);
        b.extend_from_slice(&self.chunk()[..len]);
        b.freeze()
    }
}

impl Buf for &[u8] {
    #[inline]
    fn remaining(&self) -> usize {
        self.len()
    }

    #[inline]
    fn chunk(&self) -> &[u8] {
        self
    }

    #[inline]
    fn advance(&mut self, cnt: usize) {
        if self.len() < cnt {
            panic!(
                "cannot get `{cnt}` bytes, only `{}` is remaining",
                self.len()
            )
        }

        *self = &self[cnt..];
    }
}

impl<T: Buf> Buf for &mut T {
    #[inline]
    fn remaining(&self) -> usize {
        T::remaining(self)
    }

    #[inline]
    fn chunk(&self) -> &[u8] {
        T::chunk(self)
    }

    #[inline]
    fn advance(&mut self, cnt: usize) {
        T::advance(self, cnt);
    }
}

impl<T: Buf> Buf for Box<T> {
    #[inline]
    fn remaining(&self) -> usize {
        T::remaining(self)
    }

    #[inline]
    fn chunk(&self) -> &[u8] {
        T::chunk(self)
    }

    #[inline]
    fn advance(&mut self, cnt: usize) {
        T::advance(self, cnt);
    }
}
