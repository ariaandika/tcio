use std::{
    cell::UnsafeCell,
    marker::PhantomData,
    ops::{Deref, DerefMut},
    ptr::NonNull,
    sync::atomic::{
        AtomicU8,
        Ordering::{AcqRel, Acquire, Release, SeqCst},
    },
};

/// Recallable thread-safe synchronization.
///
/// A [`Recall<T>`] is a struct that contains the `T` and track an [`Agent`]. User can create an
/// [`Agent`] which holds the full ownership of `T`. There is only 1 [`Agent`] are allowed to
/// exists for each [`Recall`]. This restriction allow [`Agent`] to have the full ownership of `T`
/// without any locking. Then later when an [`Agent`] is dropped, the ownership of `T` will be
/// "recalled" back to the original [`Recall`] where the [`Agent`] is created.
///
/// # Examples
///
/// ```no_run
///  # struct SharedResource;
///  # impl SharedResource { fn new() -> Self { Self } }
///  # struct Server;
///  # impl Server { fn new() -> Self { Self } }
///  # impl Iterator for Server {
///  #     type Item = Client;
///  #     fn next(&mut self) -> Option<Self::Item> { todo!() }
///  # }
///  # struct Client;
///  # impl Client {
///  #     fn send(&self, _: &'static str) { }
///  #     fn handle(self, _: Agent<SharedResource>) { }
///  # }
///  # use tcio::sync::Agent;
///  use tcio::sync::Recall;
///
///  let resource = Recall::new(SharedResource::new());
///  let server = Server::new();
///
///  for client in server {
///      let Some(agent) = resource.get_agent() else {
///          client.send("please try again later");
///          continue;
///      };
///
///      std::thread::spawn(move||{
///          client.handle(agent)
///      });
///  }
/// ```
///
/// [arcmut]: std::sync::Arc
#[repr(transparent)]
pub struct Recall<T: ?Sized> {
    inner: NonNull<Inner<T>>,
    _p: PhantomData<Inner<T>>
}

unsafe impl<T: ?Sized + Sync + Send> Send for Recall<T> {}
unsafe impl<T: ?Sized + Sync + Send> Sync for Recall<T> {}

impl<T: ?Sized> Recall<T> {
    #[inline]
    fn inner(&self) -> &Inner<T> {
        // SAFETY: as long as one of `Recall` and `Agent` is not dropped, `self.inner` will not be
        // deallocated
        unsafe { self.inner.as_ref() }
    }
}

impl<T> Recall<T> {
    /// Create new [`Recall`].
    #[inline]
    pub fn new(data: T) -> Self {
        Self {
            inner: Inner::new_ptr(data),
            _p: PhantomData,
        }
    }

    // ===== Delegation =====

    /// Returns `true` when there is active agent.
    #[inline]
    fn is_unique(&self) -> bool {
        self.inner().is_unique()
    }

    /// Returns `true` when there is active agent.
    #[inline]
    pub fn is_agent(&self) -> bool {
        !self.is_unique()
    }

    // ===== Getter =====

    /// Try to get shared reference of inner data.
    ///
    /// Returns [`None`] when there is active agent.
    #[inline]
    pub fn get_ref(&self) -> Option<&T> {
        if self.is_unique() {
            // SAFETY: `is_unique()` guarantees that pointer is unique, and as long as one of
            // `Recall` and `Agent` is not dropped, `self.inner` will not be deallocated
            Some(unsafe { &*self.inner.as_ref().data.get() })
        } else {
            None
        }
    }

    /// Try to get mutable reference of inner data.
    ///
    /// Returns [`None`] when there is active agent.
    #[inline]
    pub fn get_mut(&mut self) -> Option<&mut T> {
        if self.is_unique() {
            // SAFETY: `is_unique()` guarantees that pointer is unique, and as long as one of
            // `Recall` and `Agent` is not dropped, `self.inner` will not be deallocated
            Some(unsafe { self.inner.as_mut().data.get_mut() })
        } else {
            None
        }
    }

    /// Try to create new agent.
    ///
    /// Returns [`None`] when there is already active agent.
    #[inline]
    pub fn get_agent(&self) -> Option<Agent<T>> {
        if self.inner().lock() {
            Some(Agent {
                inner: self.inner,
                _p: PhantomData,
            })
        } else {
            None
        }
    }

    /// Returns the inner value if there is no agent active.
    ///
    /// Returns [`None`] when there is active agent.
    #[inline]
    pub fn into_inner(self) -> Option<T> {
        if self.is_unique() {
            // SAFETY: `is_unique()` guarantees that pointer is unique, and as long as one of
            // `Recall` and `Agent` is not dropped, `self.inner` will not be deallocated
            Some(*unsafe { Box::from_raw(self.inner().data.get()) })
        } else {
            None
        }
    }
}

impl<T: ?Sized> Drop for Recall<T> {
    fn drop(&mut self) {
        Inner::drop_recall(self.inner);
    }
}

// ===== Inner =====

struct Inner<T: ?Sized> {
    state: AtomicU8,
    data: UnsafeCell<T>,
}

const IS_RECALL: u8 = 0b0000_0001;
const IS_AGENT: u8  = 0b0000_0010;

unsafe impl<T: ?Sized + Sync + Send> Send for Inner<T> {}
unsafe impl<T: ?Sized + Sync + Send> Sync for Inner<T> {}

impl<T> Inner<T> {
    #[inline]
    fn new_ptr(data: T) -> NonNull<Self> {
        let me = Self {
            state: AtomicU8::new(IS_RECALL),
            data: UnsafeCell::new(data),
        };

        // SAFETY: concrete object pointer is not null
        unsafe { NonNull::new_unchecked(Box::into_raw(Box::new(me))) }
    }

    /// Returns `true` when successfuly acquire lock.
    #[inline]
    fn lock(&self) -> bool {
        self.state.fetch_or(IS_AGENT, AcqRel) == IS_RECALL
    }

    /// Returns `true` when current pointer is unique.
    #[inline]
    fn is_unique(&self) -> bool {
        self.state.load(Acquire) == IS_RECALL
    }
}

impl<T: ?Sized> Inner<T> {
    #[inline(never)]
    fn drop_slow(ptr: NonNull<Self>) {
        drop(unsafe { Box::from_raw(ptr.as_ptr()) });
    }

    fn drop_recall(ptr: NonNull<Self>) {
        const DROP: u8      = 0;
        const RECALL: u8    = IS_RECALL;
        const AGENT: u8     = IS_RECALL | IS_AGENT;

        let me = unsafe { ptr.as_ref() };
        match me.state.swap(DROP, SeqCst) {
            // an `Agent` alive, `drop_agent` will deallocate data
            AGENT => return,
            // no `Agent` alive, deallocate data
            RECALL => { }
            _ => unreachable!("invalid state on dropping `Recall`")
        }

        // This fence is needed to prevent reordering of use of the data and
        // deletion of the data. Because it is marked `Release`, the decreasing
        // of the reference count synchronizes with this `Acquire` fence. This
        // means that use of the data happens before decreasing the reference
        // count, which happens before this fence, which happens before the
        // deletion of the data.
        std::sync::atomic::fence(Acquire);

        Self::drop_slow(ptr);
    }

    fn drop_agent(ptr: NonNull<Self>) {
        const DROP: u8      = 0b0000_0000;
        const RECALL: u8    = 0b0000_0001;
        const AGENT: u8     = 0b0000_0011;

        let me = unsafe { ptr.as_ref() };
        match me.state.swap(RECALL, Release) {
            // `Recall` is still alive, `drop_recall` will deallocate data
            AGENT => return,
            // `Recall` is dropped, deallocate data
            DROP => { },
            RECALL => unreachable!("`Agent` alive but untracked by `Inner.agent`"),
            _ => unreachable!("invalid state on dropping `Agent`")
        }

        // This fence is needed to prevent reordering of use of the data and
        // deletion of the data. Because it is marked `Release`, the decreasing
        // of the reference count synchronizes with this `Acquire` fence. This
        // means that use of the data happens before decreasing the reference
        // count, which happens before this fence, which happens before the
        // deletion of the data.
        std::sync::atomic::fence(Acquire);

        Self::drop_slow(ptr);
    }
}

impl<T: ?Sized + std::fmt::Debug> std::fmt::Debug for Recall<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_tuple("Recall").finish()
    }
}

// ===== Agent =====

/// An agent of [`Recall`].
///
/// Access inner data through implementation of [`Deref`] and [`DerefMut`].
pub struct Agent<T: ?Sized> {
    inner: NonNull<Inner<T>>,
    _p: PhantomData<Inner<T>>
}

unsafe impl<T: ?Sized + Sync + Send> Send for Agent<T> {}
unsafe impl<T: ?Sized + Sync + Send> Sync for Agent<T> {}

impl<T: ?Sized> Agent<T> {
    #[inline]
    fn inner(&self) -> &Inner<T> {
        // SAFETY: as long as one of `Recall` and `Agent` is not dropped, `self.inner` will not be
        // deallocated
        unsafe { self.inner.as_ref() }
    }
}

impl<T: ?Sized> Deref for Agent<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        // SAFETY: `Agent` owns the unique ownership of pointer
        unsafe { &*self.inner().data.get() }
    }
}

impl<T: ?Sized> DerefMut for Agent<T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY: `Agent` owns the unique ownership of pointer
        unsafe { &mut *self.inner().data.get() }
    }
}

impl<T: ?Sized> Drop for Agent<T> {
    #[inline]
    fn drop(&mut self) {
        Inner::drop_agent(self.inner);
    }
}

impl<T: ?Sized + std::fmt::Debug> std::fmt::Debug for Agent<T> {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        T::fmt(self, f)
    }
}

#[test]
fn test_recall() {
    let content = String::new();
    let mut recall = Recall::new(content);

    recall.get_mut().unwrap().push_str("69");

    let mut agent = recall.get_agent().unwrap();

    assert!(recall.is_agent());
    assert!(recall.get_mut().is_none());

    agent.push_str("420");

    drop(agent);

    assert!(!recall.is_agent());
    assert_eq!(recall.get_mut().unwrap(), "69420");
}

#[test]
fn test_recall_thread() {
    use std::sync::mpsc::sync_channel;

    let content = String::new();
    let mut recall = Recall::new(content);
    let (t1, r1) = sync_channel(1);
    let (t2, r2) = sync_channel(1);

    recall.get_mut().unwrap().push_str("69");

    let agent = recall.get_agent().unwrap();

    std::thread::spawn(move ||{
        let mut agent = agent;
        let tx = t2;
        let rx = r1;

        rx.recv().unwrap();

        agent.push_str("420");

        drop(agent);

        tx.send(()).unwrap();
    });

    assert!(recall.is_agent());
    assert!(recall.get_mut().is_none());

    t1.send(()).unwrap();
    r2.recv().unwrap();

    assert!(!recall.is_agent());
    assert_eq!(recall.get_mut().unwrap(), "69420");
}

#[test]
fn test_recall_drop_early() {
    use std::sync::mpsc::sync_channel;
    use std::sync::atomic::AtomicBool;

    static DROPPED: AtomicBool = AtomicBool::new(false);

    struct Dropship;

    impl Drop for Dropship {
        fn drop(&mut self) {
            DROPPED.store(true, SeqCst);
        }
    }

    let content = Dropship;
    let mut recall = Recall::new(content);
    let (t1, r1) = sync_channel(1);
    let (t2, r2) = sync_channel(1);

    recall.get_mut().unwrap();

    let mut agent = recall.get_agent().unwrap();

    std::thread::spawn(move ||{
        let recall = recall;
        let tx = t2;
        let rx = r1;

        rx.recv().unwrap();

        drop(recall);

        tx.send(()).unwrap();
    });

    agent.deref_mut();

    t1.send(()).unwrap();
    r2.recv().unwrap();

    drop(agent);

    assert!(DROPPED.load(Acquire));
}
