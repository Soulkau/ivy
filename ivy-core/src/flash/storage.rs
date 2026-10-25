use const_panic::concat_panic;
use embassy_embedded_hal::adapter::BlockingAsync;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex as AsyncMutex;
use embedded_storage::nor_flash::NorFlash;
use sequential_storage::cache::NoCache;
use sequential_storage::map::{MapConfig, MapStorage};
use serde::{Deserialize, Serialize};

use crate::flash::FlashPartition;

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

    pub async fn get<D: for<'de> Deserialize<'de>>(&self, key: StorageKey) -> Option<D> {
        let mut guard = self.inner.lock().await;
        let inner = &mut *guard;
        let item_data = inner.storage.fetch_item(&mut inner.ser_buf, key.as_ref()).await.ok()??;
        let data: D = postcard::from_bytes(item_data).ok()?;
        Some(data)
    }

    pub async fn set<S: Serialize>(&self, key: StorageKey, value: &S) {
        let mut guard = self.inner.lock().await;
        let inner = &mut *guard;
        let serialized: &[u8] = postcard::to_slice(value, &mut inner.ser_buf).unwrap();
        let _ = inner.storage.store_item(&mut inner.work_buf, &key.as_ref(), &serialized).await;
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

pub struct StorageKey(pub(crate) u32);

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
