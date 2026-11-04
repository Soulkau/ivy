use core::range::Range;

use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use embedded_storage::nor_flash::NorFlash;
use heapless::Vec;
use ivy_macros::actor_handle;
use serde::{Deserialize, Serialize};

use crate::flash::FlashPartition;

#[derive(Serialize, Deserialize)]
pub struct OtaChunk<const S: usize> {
    pub offset: u32,
    pub data_len: usize,
    pub data: Vec<u8, S>,
}

#[derive(Deserialize, Serialize)]
pub struct OtaUpdateRequest {
    size: u32,
}

pub type OtaChannel<const S: usize> = Channel<CriticalSectionRawMutex, OtaChunk<S>, 4>;

#[actor_handle(OtaHandle)]
pub trait OtaHandle<const S: usize> {
    async fn ask(&self, request: OtaUpdateRequest) -> bool;
    async fn pass_chunk(&self, chunk: OtaChunk<S>) -> bool;
}

pub struct OtaModule<const S: usize, const Q: usize, F, P>
where
    F: NorFlash + 'static,
    P: OtaPlatform,
{
    partition: FlashPartition<F>,
    platform: P,
}

impl<const S: usize, const Q: usize, F, P> OtaModule<S, Q, F, P>
where
    F: NorFlash + 'static,
    P: OtaPlatform,
{
    pub fn new(partition: FlashPartition<F>, platform: P) -> Self {
        Self { partition, platform }
    }
}

pub trait OtaPlatform {
    fn set_boot_target(&mut self) -> Result<(), ()>;
    fn reboot(&mut self) -> !;
}
