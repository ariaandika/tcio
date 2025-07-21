use std::{pin::Pin, task::Poll};

/// Map a [`Future`] output.
///
/// # Example
///
/// ```
/// # async fn app() {
/// use tcio::futures::map;
/// let fut = async { 112 };
/// let result = map(fut, |e| e.to_string()).await;
/// assert_eq!(&result[..], "112");
/// # }
/// # assert!(matches!(
/// #     std::pin::pin!(app())
/// #         .poll(&mut std::task::Context::from_waker(std::task::Waker::noop())),
/// #     std::task::Poll::Ready(())
/// # ));
/// ```
#[inline]
pub fn map<F, M, O>(f: F, map: M) -> Map<F, M>
where
    F: Future,
    M: FnOnce(F::Output) -> O,
{
    Map { f, map: Some(map) }
}

/// Future returned by [`map`].
#[derive(Debug)]
pub struct Map<F, M> {
    f: F,
    map: Option<M>,
}

impl<F, M, O> Future for Map<F, M>
where
    F: Future,
    M: FnOnce(F::Output) -> O,
{
    type Output = O;

    fn poll(self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        // SAFETY: self is pinned
        // no `Drop`, nor manual `Unpin` implementation.
        let (f, map) = unsafe {
            let me = self.get_unchecked_mut();
            (Pin::new_unchecked(&mut me.f), &mut me.map)
        };
        match f.poll(cx) {
            Poll::Ready(ok) => Poll::Ready(map.take().expect("poll after complete")(ok)),
            Poll::Pending => Poll::Pending,
        }
    }
}
