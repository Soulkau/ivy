use embassy_futures::select::{Either, select};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use heapless::Vec;
use upit::UpdateRequest;

use crate::{
    codecs::{CodecError, Postcard},
    mqtt::{
        RawMqttHandle,
        subscription::{ErasedSubscription, Subscription},
    },
    ota::{
        OtaChunk, OtaEvent, OtaHandle,
        client::{OtaClient, OtaTransport},
    },
};

const REPLY_TOPIC: &str = "ota/reply";
const CALLBACK_TOPIC: &str = "ota/callbacks";

#[derive(Clone, Copy)]
pub struct ChunkStreamSubscription<const C: usize> {
    channel: &'static Channel<CriticalSectionRawMutex, OtaChunk<C>, 4>,
    chunk_topic: &'static str,
}

impl<const C: usize> ChunkStreamSubscription<C> {
    pub fn new(chunk_topic: &'static str, channel: &'static Channel<CriticalSectionRawMutex, OtaChunk<C>, 4>) -> Self {
        Self { channel, chunk_topic }
    }

    pub async fn next_chunk(&self) -> OtaChunk<C> {
        self.channel.receive().await
    }

    pub fn drain(&self) {
        while self.channel.try_receive().is_ok() {}
    }
}

impl<const C: usize> ErasedSubscription for ChunkStreamSubscription<C> {
    fn topic(&self) -> &'static str {
        self.chunk_topic
    }

    fn dispatch(&self, buf: &[u8]) -> Result<(), CodecError> {
        let Ok(data) = Vec::from_slice(buf) else {
            tracing::warn!(tag = "mqtt/ota", "chunk packet bigger than C, dropped");
            return Ok(());
        };
        if self.channel.try_send(OtaChunk { data }).is_err() {
            tracing::warn!(tag = "mqtt/ota", "chunk channel full, chunk dropped");
        }
        Ok(())
    }
}

pub struct MqttOta<const C: usize> {
    request: Subscription<UpdateRequest, Postcard>,
    chunks: ChunkStreamSubscription<C>,
    mqtt: RawMqttHandle,
}

//TODO: handle mqtt errors
impl<const C: usize> OtaTransport<C> for MqttOta<C> {
    async fn next_event(&mut self) -> crate::ota::OtaEvent<C> {
        match select(self.request.next(), self.chunks.next_chunk()).await {
            Either::First(request) => OtaEvent::Request(request),
            Either::Second(chunk) => OtaEvent::Chunk(chunk),
        }
    }

    async fn send_reply(&mut self, bytes: &[u8]) {
        unsafe {
            self.mqtt.publish(REPLY_TOPIC, bytes).await;
        }
    }

    async fn send_callback(&mut self, bytes: &[u8]) {
        unsafe {
            self.mqtt.publish(CALLBACK_TOPIC, bytes).await;
        }
    }
}

pub async fn ota_mqtt_task<const C: usize>(handle: OtaHandle<C>, request: Subscription<UpdateRequest, Postcard>, chunks: ChunkStreamSubscription<C>, mqtt_handle: RawMqttHandle) -> ! {
    let client = OtaClient::new(handle);
    let mut transport = MqttOta { request, chunks, mqtt: mqtt_handle };
    client.run(&mut transport).await
}
