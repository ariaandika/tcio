
fn main() {
    app1();
    app2();
    app3();
    app4();
}

/// [`String`] can be converted to `parts` (ptr,len),
/// send between thread, then create [`str`] from it
fn app1() {
    let mut s = String::from("clap");

    let slice = unsafe {
        let ptr = s.as_ptr();
        let len = s.len();
        let slice = std::slice::from_raw_parts::<'static>(ptr, len);
        std::str::from_utf8_unchecked(slice)
    };

    s.insert(0, 'g');

    assert_eq!(&slice[0..3],"gcl");
    assert_eq!(slice,"gcla");
}

/// [`String`] can be converted to `parts` (ptr,len,cap),
/// send between thread, then recreate [`String`] from it
///
/// using [`String::leak`] before [`str::as_mut_ptr`] to acquire ptr,
/// otherwise 2 [`String`] will own the same pointer thus causing
/// double free runtime error
///
/// memory will not deallocate because no `drop` was called
fn app2() {
    struct StringParts {
        ptr: usize,
        len: usize,
        cap: usize,
    }

    impl StringParts {
        fn new(s: String) -> Self {
            Self { len: s.len(), cap: s.capacity(), ptr: s.leak().as_mut_ptr() as usize }
        }
        fn into_string(self) -> String {
            unsafe { String::from_raw_parts(self.ptr as *mut u8, self.len, self.cap) }
        }
    }

    let s = String::from("clap");
    let parts = StringParts::new(s);
    let (tx,rx) = std::sync::mpsc::sync_channel::<StringParts>(0);

    std::thread::spawn(move||{
        let parts = parts;
        let mut s = parts.into_string();

        assert_eq!(&s[..],"clap");
        s.push_str("deez");

        let parts = StringParts::new(s);
        let _ = tx.send(parts);
    });

    let parts = rx.recv().unwrap();
    let s = parts.into_string();
    assert_eq!(&s[..],"clapdeez");
}

/// one can also use pointer
fn app3() {
    let mut s = String::from("clap");

    let ptr = &mut s as *mut String as usize;
    let (tx,rx) = std::sync::mpsc::sync_channel::<()>(0);

    std::thread::spawn(move||unsafe{
        let ptr = ptr as *mut String;
        let val = &mut *ptr;
        assert_eq!(&val[..], "clap");
        val.push_str("deez");
        let _ = tx.send(());
    });

    let _ = rx.recv().unwrap();

    unsafe {
        let ptr = ptr as *mut String;
        let val = &mut *ptr;
        assert_eq!(&val[..],"clapdeez");
    }
}

/// slice reference also contains it len
fn app4() {
    let mut s1 = Vec::from(b"nice");
    let s2 = unsafe { &*{ &s1[..] as *const [u8] } };

    // notable behaviour
    {
        s1.push(b's');
        assert_eq!(s2.len(),4);
    }

    let mut iter = s2.iter();

    assert!(matches!(iter.next(),Some(b'n')));
    assert!(matches!(iter.next(),Some(b'i')));
    assert!(matches!(iter.next(),Some(b'c')));
    assert!(matches!(iter.next(),Some(b'e')));
    assert!(matches!(iter.next(),None));
}

