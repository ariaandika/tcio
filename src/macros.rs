
macro_rules! deref {
    (|&mut $meid:ident: $me:ty| -> $target:ty { $e:expr } $($tt:tt)*) => {
        impl std::ops::DerefMut for $me {
            #[inline]
            fn deref_mut(&mut $meid) -> &mut Self::Target {
                $e
            }
        }

        impl AsMut<$target> for $me {
            #[inline]
            fn as_mut(&mut $meid) -> &mut $target {
                $e
            }
        }
    };
    (|&$meid:ident: $me:ty| -> $target:ty { $e:expr } $($tt:tt)*) => {
        impl std::ops::Deref for $me {
            type Target = $target;

            #[inline]
            fn deref(&$meid) -> &Self::Target {
                $e
            }
        }

        impl AsRef<$target> for $me {
            #[inline]
            fn as_ref(&$meid) -> &$target {
                $e
            }
        }

        crate::macros::deref!($($tt)*);
    };
    () => { }
}

macro_rules! partial_eq {
    ($(<$($lf:tt)*>)*|&$a1:ident:$t1:ty,$a2:ident:$t2:ty| { $e:expr } $($tt:tt)*) => {
        impl$(<$($lf)*>)* PartialEq<$t2> for $t1 {
            fn eq(&$a1, $a2: &$t2) -> bool {
                $e
            }
        }
        crate::macros::partial_eq!($($tt)*);
    };
    () => { };
}

macro_rules! from {
    ($(<$($lf:lifetime),*>)?|$a1:ident:$t1:ty| -> $me:ty { $e:expr } $($tt:tt)*) => {
        impl$(<$($lf)*>)? From<$t1> for $me {
            #[inline]
            fn from($a1: $t1) -> Self {
                $e
            }
        }
        crate::macros::from!($($tt)*);
    };
    () => { };
}

pub(crate) use {deref, partial_eq, from};

