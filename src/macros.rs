macro_rules! partial_eq {
    (
        @impl <$($lf:lifetime),*> $me:ty;
        fn $fn_id:ident($slef:ident, $other:ident:$other_ty:ty) { $($e:expr);* }
        $($tt:tt)*
    ) => {
        impl PartialEq<$other_ty> for $me {
            #[inline]
            fn $fn_id(&$slef, $other:&$other_ty) -> bool {
                $($e);*
            }
        }
        crate::macros::partial_eq!(@impl <$($lf),*> $me; $($tt)*);
    };
    (@impl <$($lf:lifetime),*> $me:ty;) => { }; // base case

    // user input
    (
        impl $(<$($lf:lifetime),*>)? $me:ty;
        $($tt:tt)*
    ) => {
        crate::macros::partial_eq!(@impl <$($($lf),*)?> $me; $($tt)*);
    };
}

macro_rules! from {
    (
        @impl <$($lf:lifetime),*> $me:ty;
        fn $fn_id:ident($value:ident:$value_ty:ty) { $($e:expr);* }
        $($tt:tt)*
    ) => {
        impl From<$value_ty> for $me {
            #[inline]
            fn $fn_id($value:$value_ty) -> Self {
                $($e);*
            }
        }
        crate::macros::from!(@impl <$($lf),*> $me; $($tt)*);
    };
    (@impl <$($lf:lifetime),*> $me:ty;) => { }; // base case

    // user input
    (
        impl $(<$($lf:lifetime),*>)? $me:ty;
        $($tt:tt)*
    ) => {
        crate::macros::from!(@impl <$($($lf),*)?> $me; $($tt)*);
    };
}

pub(crate) use {partial_eq, from};
