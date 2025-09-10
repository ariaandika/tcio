macro_rules! impl_std_traits {
    (impl $($tt:tt)*) => {
        crate::macros::impl_std_traits_defaulted!(impl $($tt)*);
    };
    ($($tt:tt)*) => {
        crate::macros::impl_std_traits_standalone!($($tt)*);
    };
}

macro_rules! impl_std_traits_standalone {
    // fn drop
    {
        @drop fn $drop:ident$(<$($lf:lifetime),*>)?(&mut $a1:ident:$t1:ty) { $($e:expr);* }
        $($tt:tt)*
    } => {
        impl$(<$($lf)*>)? Drop for $t1 {
            #[inline] fn $drop(&mut $a1) { $($e);* }
        }
        crate::macros::impl_std_traits!($($tt)*);
    };

    // fn default
    {
        @default fn $default:ident$(<$($lf:lifetime),*>)?() -> $t1:ty { $($e:expr);* }
        $($tt:tt)*
    } => {
        impl$(<$($lf)*>)? Default for $t1 {
            #[inline] fn $default() -> Self { $($e);* }
        }
        crate::macros::impl_std_traits!($($tt)*);
    };

    // fn clone
    {
        @clone fn $clone:ident$(<$($lf:lifetime),*>)?(&$a1:ident:$t1:ty) $(-> Self)? {
            $($e:expr);*
        }
        $($tt:tt)*
    } => {
        impl$(<$($lf)*>)? Clone for $t1 {
            #[inline] fn $clone(&$a1) -> Self { $($e);* }
        }
        crate::macros::impl_std_traits!($($tt)*);
    };

    // fn deref
    {
        @deref fn $deref:ident$(<$($lf:lifetime),*>)?(&$a1:ident:$t1:ty) -> &$t2:ty {
            $($e:expr);*
        }
        $($tt:tt)*
    } => {
        impl$(<$($lf)*>)? std::ops::Deref for $t1 {
            type Target = $t2;
            #[inline] fn $deref(&$a1) -> &Self::Target { $($e);* }
        }
        impl$(<$($lf)*>)? AsRef<$t2> for $t1 {
            #[inline] fn as_ref(&$a1) -> &$t2 { $($e);* }
        }
        crate::macros::impl_std_traits!($($tt)*);
    };

    // fn deref_mut
    {
        @deref_mut fn $deref_mut:ident$(<$($lf:lifetime),*>)?(&mut $a1:ident:$t1:ty) -> &mut $t2:ty {
            $($e:expr);*
        }
        $($tt:tt)*
    } => {
        impl$(<$($lf)*>)? std::ops::DerefMut for $t1 {
            #[inline] fn $deref_mut(&mut $a1) -> &mut Self::Target { $($e);* }
        }
        impl$(<$($lf)*>)? AsMut<$t2> for $t1 {
            #[inline] fn as_mut(&mut $a1) -> &mut $t2 { $($e);* }
        }
        crate::macros::impl_std_traits!($($tt)*);
    };

    // fn from
    {
        @from fn $from:ident$(<$($lf:lifetime),*>)?($a2:ident:$t2:ty) -> $t1:ty {
            $($e:expr);*
        }
        $($tt:tt)*
    } => {
        impl$(<$($lf)*>)? From<$t2> for $t1 {
            #[inline] fn $from($a2: $t2) -> Self { $($e);* }
        }
        crate::macros::impl_std_traits!($($tt)*);
    };

    // fn fmt
    // fn fmt_debug
    (@fmt_debug fn $_:ident $($tt:tt)*) => {crate::macros::impl_std_traits!(@fmt fn fmt $($tt)*);};
    {
        @fmt fn $fmt:ident$(<$($lf:lifetime),*>)?(&$a1:ident:$t1:ty,$af:ident$(:$tf:ty)?) {
            $($e:expr);*
        }
        $($tt:tt)*
    } => {
        impl$(<$($lf)*>)? std::fmt::Debug for $t1 {
            #[inline] fn $fmt(&$a1, $af: &mut std::fmt::Formatter) -> std::fmt::Result { $($e);* }
        }
        crate::macros::impl_std_traits!($($tt)*);
    };

    // fn eq
    {
        @eq fn $eq:ident$(<$($lf:lifetime),*>)?(&$a1:ident:$t1:ty,&$a2:ident:$t2:ty) $(-> bool)? {
            $($e:expr);*
        }
        $($tt:tt)*
    } => {
        impl$(<$($lf)*>)* PartialEq<$t2> for $t1 {
            #[inline] fn $eq(&$a1, $a2: &$t2) -> bool { $($e);* }
        }
        crate::macros::impl_std_traits!($($tt)*);
    };

    // fn <unknown>
    {
        @$idk:ident fn $idk2:ident$(<$($lf:lifetime),*>)?($($tt2:tt)*) $(-> $r1:ty)? {
            $($e:expr);*
        }
        $($tt:tt)*
    } => {
        fn $idk2$(<$($lf),*>)?($($tt2)*) $(-> $r1)? {
            compile_error!(concat!("unknown std trait with method: ",stringify!($idk)));
            $($e:expr);*
        }
        crate::macros::impl_std_traits!($($tt)*);
    };


    // user input
    (fn $fn_id:ident $($tt:tt)*) => {
        crate::macros::impl_std_traits!(@$fn_id fn $fn_id $($tt)*);
    };
    (impl $fn_id:ident $($tt:tt)*) => {
        crate::macros::impl_std_traits!(@$fn_id fn $fn_id $($tt)*);
    };
    () => { };
}

macro_rules! impl_std_traits_defaulted {
    // fn drop
    {
        @{$(<$($lf:lifetime),*>)? $me:ty}
        @drop fn $drop:ident(&mut $a1:ident) { $($e:expr);* }
        $($tt:tt)*
    } => {
        impl$(<$($lf)*>)? Drop for $me {
            #[inline] fn $drop(&mut $a1) { $($e);* }
        }
        crate::macros::impl_std_traits_defaulted!(@{$(<$($lf),*>)? $me} $($tt)*);
    };


    // fn default
    {
        @{$(<$($lf:lifetime),*>)? $me:ty}
        @default fn $default:ident() { $($e:expr);* }
        $($tt:tt)*
    } => {
        crate::macros::impl_std_traits_defaulted!(
            @{$(<$($lf),*>)? $me}
            @default fn $default() -> Self { $($e);* }
            $($tt)*
        );
    };
    {
        @{$(<$($lf:lifetime),*>)? $me:ty}
        @default fn $default:ident() -> $Self:ty { $($e:expr);* }
        $($tt:tt)*
    } => {
        impl$(<$($lf)*>)? Default for $me {
            #[inline] fn $default() -> $Self { $($e);* }
        }
        crate::macros::impl_std_traits_defaulted!(@{$(<$($lf),*>)? $me} $($tt)*);
    };


    // fn clone
    {
        @{$(<$($lf:lifetime),*>)? $me:ty}
        @clone fn $clone:ident(&$a1:ident) { $($e:expr);* }
        $($tt:tt)*
    } => {
        crate::macros::impl_std_traits_defaulted!(
            @{$(<$($lf),*>)? $me}
            @clone fn $clone(&$a1) -> Self { $($e);* }
            $($tt)*
        );
    };
    {
        @{$(<$($lf:lifetime),*>)? $me:ty}
        @clone fn $clone:ident(&$a1:ident) -> $Self:ty { $($e:expr);* }
        $($tt:tt)*
    } => {
        impl$(<$($lf)*>)? Clone for $me {
            #[inline] fn $clone(&$a1) -> $Self { $($e);* }
        }
        crate::macros::impl_std_traits_defaulted!(@{$(<$($lf),*>)? $me} $($tt)*);
    };


    // fn deref
    {
        @{$(<$($lf:lifetime),*>)? $me:ty}
        @deref fn $deref:ident(&$a1:ident) -> &$t2:ty { $($e:expr);* }
        $($tt:tt)*
    } => {
        impl$(<$($lf)*>)? std::ops::Deref for $me {
            type Target = $t2;
            #[inline] fn $deref(&$a1) -> &$t2 { $($e);* }
        }
        crate::macros::impl_std_traits_defaulted!(@{$(<$($lf),*>)? $me} $($tt)*);
    };
    // fn deref_mut
    {
        @{$(<$($lf:lifetime),*>)? $me:ty}
        @deref_mut fn $deref_mut:ident(&mut $a1:ident) -> &mut $t2:ty { $($e:expr);* }
        $($tt:tt)*
    } => {
        impl$(<$($lf)*>)? std::ops::DerefMut for $me {
            #[inline] fn $deref_mut(&mut $a1) -> &mut $t2 { $($e);* }
        }
        crate::macros::impl_std_traits_defaulted!(@{$(<$($lf),*>)? $me} $($tt)*);
    };


    // fn from
    {
        @{$(<$($lf:lifetime),*>)? $me:ty}
        @from fn $from:ident($a2:ident:$t2:ty) { $($e:expr);* }
        $($tt:tt)*
    } => {
        crate::macros::impl_std_traits_defaulted!(
            @{$(<$($lf),*>)? $me}
            @from fn $from($a2:$t2) -> Self { $($e);* }
            $($tt)*
        );
    };
    {
        @{$(<$($lf:lifetime),*>)? $me:ty}
        @from fn $from:ident($a2:ident:$t2:ty) -> $Self:ty { $($e:expr);* }
        $($tt:tt)*
    } => {
        impl$(<$($lf)*>)? From<$t2> for $me {
            #[inline] fn $from($a2: $t2) -> $Self { $($e);* }
        }
        crate::macros::impl_std_traits_defaulted!(@{$(<$($lf),*>)? $me} $($tt)*);
    };


    // fn eq
    {
        @{$(<$($lf:lifetime),*>)? $me:ty}
        @eq fn $eq:ident(&$a1:ident, &$a2:ident:$t2:ty) { $($e:expr);* }
        $($tt:tt)*
    } => {
        crate::macros::impl_std_traits_defaulted!(
            @{$(<$($lf),*>)? $me}
            @eq fn $eq(&$a1, &$a2:$t2) -> bool { $($e);* }
            $($tt)*
        );
    };
    {
        @{$(<$($lf:lifetime),*>)? $me:ty}
        @eq fn $eq:ident(&$a1:ident, &$a2:ident:$t2:ty) -> $Self:ty { $($e:expr);* }
        $($tt:tt)*
    } => {
        impl$(<$($lf)*>)? PartialEq<$t2> for $me {
            #[inline] fn $eq(&$a1, $a2: &$t2) -> $Self { $($e);* }
        }
        crate::macros::impl_std_traits_defaulted!(@{$(<$($lf),*>)? $me} $($tt)*);
    };


    // fn fmt
    {
        @{$(<$($lf:lifetime),*>)? $me:ty}
        @fmt fn $fmt:ident(&$a1:ident:$t1:ty,$af:ident$(:$tf:ty)?) {
            $($e:expr);*
        }
        $($tt:tt)*
    } => {
        impl$(<$($lf)*>)? std::fmt::Debug for $t1 {
            #[inline] fn $fmt(&$a1, $af: &mut std::fmt::Formatter) -> std::fmt::Result { $($e);* }
        }
        crate::macros::impl_std_traits_defaulted!(@{$(<$($lf),*>)? $me} $($tt)*);
    };


    // fn <unknown>
    {
        @{$(<$($lf:lifetime),*>)? $me:ty}
        @$idk:ident fn $idk2:ident($($tt2:tt)*) $(-> $r1:ty)? {
            $($e:expr);*
        }
        $($tt:tt)*
    } => {
        fn $idk2$(<$($lf),*>)?($($tt2)*) $(-> $r1)? {
            compile_error!(concat!("unknown std trait with method: ",stringify!($idk)));
            $($e:expr);*
        }
        crate::macros::impl_std_traits_defaulted!(@{$(<$($lf),*>)? $me} $($tt)*);
    };


    // user input
    (@{$(<$($lf:lifetime),*>)? $me:ty} fn $fn_id:ident $($tt:tt)*) => {
        crate::macros::impl_std_traits_defaulted!(
            @{$(<$($lf),*>)? $me} @$fn_id fn $fn_id $($tt)*
        );
    };
    (@{$(<$($lf:lifetime),*>)? $me:ty}) => {
        // end recursion
    };

    // first user input
    (impl $(<$($lf:lifetime),*>$(,)?)? $me:ty; $($tt:tt)*) => {
        crate::macros::impl_std_traits_defaulted!(
            @{$(<$($lf),*>)? $me} $($tt)*
        );
    };
    () => { };
}

pub(crate) use {impl_std_traits, impl_std_traits_standalone, impl_std_traits_defaulted};

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
