use core::{cell::RefCell, ops::Range};

use embassy_sync::blocking_mutex::{Mutex as BlockingMutex, raw::CriticalSectionRawMutex};
use embedded_storage::nor_flash::NorFlash;
use embedded_storage::nor_flash::{ErrorType, ReadNorFlash};

pub mod storage;

type SynchronizedFlash<F> = BlockingMutex<CriticalSectionRawMutex, RefCell<F>>;

/// Holds synchronized flash, used to create partitions.
/// All partitions share the same physical flash and are access-serialized.
pub struct Flash<F: NorFlash + 'static> {
    flash: &'static SynchronizedFlash<F>,
}

impl<F: NorFlash> Flash<F> {
    pub fn new(flash: &'static SynchronizedFlash<F>) -> Self {
        Self { flash }
    }
    /// Creates new partition
    pub fn partition(&self, range: Range<u32>) -> FlashPartition<F> {
        FlashPartition::new(self.flash, range)
    }
}

/// A region of flash defined by `range`, addressed relative to itself (0..size),
/// not the chip's absolute position. Bounds are NOT checked.
pub struct FlashPartition<F: NorFlash + 'static> {
    flash: &'static SynchronizedFlash<F>,
    range: Range<u32>, // start = offset, end = offset + size
}

impl<F: NorFlash> FlashPartition<F> {
    pub const fn new(flash: &'static SynchronizedFlash<F>, range: Range<u32>) -> Self {
        Self { flash, range }
    }
    /// Start offest of partition relative to flash
    fn offset(&self) -> u32 {
        self.range.start
    }
    /// Parition size in butes
    fn size(&self) -> u32 {
        self.range.end - self.range.start
    }
}

impl<F: NorFlash> ErrorType for FlashPartition<F> {
    type Error = F::Error;
}

impl<F: NorFlash> ReadNorFlash for FlashPartition<F> {
    const READ_SIZE: usize = F::READ_SIZE;

    fn read(&mut self, off: u32, buf: &mut [u8]) -> Result<(), Self::Error> {
        let offset = self.offset();
        self.flash.lock(|cell| cell.borrow_mut().read(offset + off, buf))
    }

    fn capacity(&self) -> usize {
        self.size() as usize
    }
}

impl<F: NorFlash> NorFlash for FlashPartition<F> {
    const WRITE_SIZE: usize = F::WRITE_SIZE;
    const ERASE_SIZE: usize = F::ERASE_SIZE;

    fn erase(&mut self, from: u32, to: u32) -> Result<(), Self::Error> {
        let offset = self.offset();
        self.flash.lock(|cell| cell.borrow_mut().erase(offset + from, offset + to))
    }

    fn write(&mut self, off: u32, data: &[u8]) -> Result<(), Self::Error> {
        let offset = self.offset();
        self.flash.lock(|cell| cell.borrow_mut().write(offset + off, data))
    }
}

#[macro_export]
macro_rules! init_flash {
    ($flash_ty:ty, $flash:expr) => {{
        static CELL: static_cell::StaticCell<embassy_sync::blocking_mutex::Mutex<embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex, core::cell::RefCell<$flash_ty>>> =
            static_cell::StaticCell::new();
        let flash_ref = CELL.init_with(|| embassy_sync::blocking_mutex::Mutex::new(core::cell::RefCell::new($flash)));
        $crate::flash::Flash::<$flash_ty>::new(flash_ref)
    }};
}
