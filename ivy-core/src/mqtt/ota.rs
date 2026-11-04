use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};

use crate::{
    codecs::{Decode, Postcard},
    mqtt::subscription::{ErasedSubscription, Subscription},
    ota::{OtaChunk, OtaHandle, OtaUpdateRequest},
};

pub struct ChunkStreamSubscription<const C: usize> {
    channel: &'static Channel<CriticalSectionRawMutex, OtaChunk<C>, 4>,
    chunk_topic: &'static str,
}

impl<const C: usize> ErasedSubscription for ChunkStreamSubscription<C> {
    fn topic(&self) -> &'static str {
        self.chunk_topic
    }

    fn dispatch(&self, buf: &[u8]) -> Result<(), crate::codecs::CodecError> {
        let chunk = <OtaChunk<C> as Decode<Postcard>>::decode(buf)?;
        self.channel.try_send(chunk).ok();
        Ok(())
    }
}

pub async fn ota_mqtt_task<const C: usize>(handle: OtaHandle<C>, request: Subscription<OtaUpdateRequest>, chunks: ChunkStreamSubscription<C>) -> ! {
    loop {
        let request = request.next().await;
        let go_on = handle.ask(request).await;
        if !go_on {
            continue;
        }

        while !handle.pass_chunk(chunks.channel.receive().await).await {
            // Do something meanwhile it is not finished idk
        }
    }
}
