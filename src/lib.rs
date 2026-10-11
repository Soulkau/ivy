#![no_std]

pub use ivy_core::*;

pub use ivy_types as types;
pub use ivy_types::actor;

#[doc(hidden)]
pub mod __private {
    pub use ivy_core as core;
    pub use ivy_core::__private::*;
}

#[doc(hidden)]
pub use ivy_app_macros::__subscribe_internal;

pub use ivy_macros::*;

pub mod macros {

    #[macro_export]
    macro_rules! subscription {
        ($($input:tt)*) => {
            $crate::__subscribe_internal!($crate, $($input)*)
        };
    }
}
