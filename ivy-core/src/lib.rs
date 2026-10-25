#![no_std]
#![feature(generic_const_exprs)]
#![feature(allocator_api)]
#![allow(incomplete_features)]

pub mod codecs;
pub mod flash;
pub mod logger;
pub mod mqtt;
pub mod wifi;

extern crate alloc;

#[doc(hidden)]
pub mod __private {
    pub use embassy_sync;
    pub use paste;
    pub use static_cell;
}

#[macro_export]
macro_rules! mk_static {
    ($t:ty, $val:expr) => {{
        use $crate::__private::static_cell::StaticCell;

        static CELL: StaticCell<$t> = StaticCell::new();
        CELL.init($val)
    }};
}
