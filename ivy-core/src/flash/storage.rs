use core::fmt::Display;

use const_panic::concat_panic;
use embassy_embedded_hal::adapter::BlockingAsync;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex as AsyncMutex;
use embedded_storage::nor_flash::NorFlash;
use sequential_storage::cache::NoCache;
use sequential_storage::map::{MapConfig, MapStorage};

use crate::codecs::{CodecError, Decode, Encode, Postcard};
use crate::flash::FlashPartition;

#[derive(Debug, thiserror::Error)]
pub enum StorageError<E: core::fmt::Debug> {
    /// Flash or sequential-storage failure
    #[error("flash error: {0:?}")]
    Flash(#[from] sequential_storage::Error<E>),
    /// Value didn't fit in the buffer or couldn't be serialized
    #[error("failed to encode value: {0}")]
    Encode(CodecError),
    /// Bytes exist but don't match the type (corrupt, or written by a different version)
    #[error("failed to decode value: {0}")]
    Decode(CodecError),
}

pub struct StorageInner<F: NorFlash + 'static> {
    storage: MapStorage<u32, BlockingAsync<FlashPartition<F>>, NoCache>,
    ser_buf: [u8; 256],
    work_buf: [u8; 256],
}

pub type Inner<F> = AsyncMutex<CriticalSectionRawMutex, StorageInner<F>>;

pub struct FlashStorage<F: NorFlash + 'static> {
    pub inner: &'static Inner<F>,
}

impl<F: NorFlash> FlashStorage<F> {
    #[doc(hidden)]
    pub fn build(partition: FlashPartition<F>) -> Inner<F> {
        let map_config = MapConfig::new(0..partition.size()); // relative, not absolute
        AsyncMutex::new(StorageInner {
            storage: MapStorage::new(BlockingAsync::new(partition), map_config, NoCache::new()),
            ser_buf: [0u8; 256],
            work_buf: [0u8; 256],
        })
    }

    pub fn from_static(inner: &'static Inner<F>) -> Self {
        Self { inner }
    }

    pub async fn get<D>(&self, key: StorageKey) -> Result<Option<D>, StorageError<F::Error>>
    where
        D: for<'a> Decode<'a, Postcard>,
    {
        let mut guard = self.inner.lock().await;
        let inner = &mut *guard;

        let maybe_bytes = inner.storage.fetch_item(&mut inner.ser_buf, key.as_ref()).await?;
        let Some(bytes) = maybe_bytes else {
            return Ok(None);
        };

        let value = <D as Decode<Postcard>>::decode(bytes).map_err(StorageError::Decode)?;
        Ok(Some(value))
    }

    pub async fn set<S>(&self, key: StorageKey, value: &S) -> Result<(), StorageError<F::Error>>
    where
        S: Encode<Postcard>,
    {
        let mut guard = self.inner.lock().await;
        let inner = &mut *guard;

        let len = <S as Encode<Postcard>>::encode(value, &mut inner.ser_buf).map_err(StorageError::Encode)?;
        let serialized: &[u8] = &inner.ser_buf[..len];
        inner.storage.store_item(&mut inner.work_buf, key.as_ref(), &serialized).await?;
        Ok(())
    }
}

#[macro_export]
macro_rules! init_storage {
    ($flash_ty:ty, $partition:expr) => {{
        static CELL: static_cell::StaticCell<$crate::flash::Inner<$flash_ty>> = static_cell::StaticCell::new();
        let storage_ref = CELL.init_with(|| $crate::flash::FlashStorage::<$flash_ty>::build($partition));
        $crate::flash::FlashStorage::from_static(storage_ref)
    }};
}

#[derive(Debug)]
pub struct StorageKey(pub(crate) u32);

impl Display for StorageKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

pub const MIN_KEY_VALUE: u32 = 100;

impl StorageKey {
    pub const fn new(key: u32) -> Self {
        if key < MIN_KEY_VALUE {
            concat_panic!("This key is reserved for system use: ", key);
        }
        Self(key)
    }

    pub(crate) const fn metadata_key() -> Self {
        Self(1)
    }
}

impl AsRef<u32> for StorageKey {
    fn as_ref(&self) -> &u32 {
        &self.0
    }
}
