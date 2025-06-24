//! Futures utilities.
use std::{pin::Pin, task::ready};

// ===== Map =====

/// Map a [`Future`] output.
///
/// # Example
///
/// ```
/// # async fn app() {
/// use tcio::futures::map;
/// let fut = std::future::ready(112);
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

    #[inline]
    fn poll(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        // SAFETY: self is pinned
        // no `Drop`, nor manual `Unpin` implementation.
        let (f, map) = unsafe {
            let me = self.get_unchecked_mut();
            (Pin::new_unchecked(&mut me.f), &mut me.map)
        };
        f.poll(cx).map(map.take().expect("poll after complete"))
    }
}

// ===== Then =====

/// Map a [`Future`] output into another [`Future`].
///
/// # Example
///
/// ```
/// # async fn app() {
/// use tcio::futures::then;
/// let fut = std::future::ready(112);
/// let result = then(fut, |e| async move { e.to_string() }).await;
/// assert_eq!(&result[..], "112");
/// # }
/// # assert!(matches!(
/// #     std::pin::pin!(app())
/// #         .poll(&mut std::task::Context::from_waker(std::task::Waker::noop())),
/// #     std::task::Poll::Ready(())
/// # ));
/// ```
#[inline]
pub fn then<F, M, F2>(f: F, map: M) -> Then<F, M, F2>
where
    F: Future,
    M: FnOnce(F::Output) -> F2,
    F2: Future,
{
    Then { phase: Phase::F1(f), map: Some(map) }
}

/// Future returned by [`then`].
#[derive(Debug)]
pub struct Then<F, M, F2> {
    phase: Phase<F, F2>,
    map: Option<M>,
}

#[derive(Debug)]
enum Phase<F, F2> {
    F1(F),
    F2(F2),
}

impl<F, M, F2> Future for Then<F, M, F2>
where
    F: Future,
    M: FnOnce(F::Output) -> F2,
    F2: Future,
{
    type Output = F2::Output;

    #[inline]
    fn poll(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        // SAFETY: self is pinned
        // no `Drop`, nor manual `Unpin` implementation.
        let me = unsafe { self.as_mut().get_unchecked_mut() };

        match &mut me.phase {
            Phase::F1(f) => {
                // SAFETY: self is pinned
                // no `Drop`, nor manual `Unpin` implementation.
                let f = unsafe { Pin::new_unchecked(f) };
                let ok = ready!(f.poll(cx));
                let ok = me.map.take().expect("poll after complete")(ok);
                me.phase = Phase::F2(ok);
                self.poll(cx)
            }
            // SAFETY: self is pinned
            // no `Drop`, nor manual `Unpin` implementation.
            Phase::F2(f) => unsafe { Pin::new_unchecked(f) }.poll(cx),
        }
    }
}

