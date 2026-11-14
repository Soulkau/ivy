use core::marker::PhantomData;

use alloc::borrow::Cow;
use embassy_futures::select::{Either, select};
use embassy_time::{Duration, Instant, Timer};
use embedded_storage::nor_flash::{NorFlash, ReadNorFlash};
use heapless::Vec;
use ivy_macros::actor_handle;
use ivy_types::actor::Actor;
use upit::{DeviceSpecs, OtaReply, UpdateChunk, UpdateError, UpdateRequest, UpdateResponse, UpitProtocol, flash::UpdateWriter, protocols::offset::OffsetProtocolV1};

use crate::flash::FlashPartition;

pub mod client;
pub mod mqtt;

/// The wire adds a length prefix to every chunk, so not all of C is data.
const CHUNK_ENVELOPE: usize = 3;

const IDLE_TIMEOUT: Duration = Duration::from_secs(30);

/// One raw packet from the transport. Only the module decodes it.
pub struct OtaChunk<const S: usize> {
    pub data: Vec<u8, S>,
}

pub type OtaBytes = Vec<u8, 64>;

pub enum OtaEvent<const C: usize> {
    Request(UpdateRequest),
    Chunk(OtaChunk<C>),
}

pub enum OtaResult {
    RequestResult(OtaBytes),
    Callback(OtaBytes),
    Nothing,
}

#[actor_handle(OtaHandle)]
pub trait OtaHandle<const C: usize> {
    async fn event(&self, event: OtaEvent<C>) -> OtaResult;
    async fn abort(&self);
}

pub enum OtaState<P: UpitProtocol> {
    Idle,
    Progress {
        writer: UpdateWriter<P>,
        request: UpdateRequest,
        last_activity: Instant,
    },
    Rebooting,
}

//TODO: reboot after update is complete
pub struct OtaModule<const C: usize, F, P, UP = OffsetProtocolV1>
where
    UP: UpitProtocol,
    F: NorFlash + 'static,
    P: OtaPlatform + 'static,
{
    protocol: PhantomData<UP>,
    partition: FlashPartition<F>,
    state: OtaState<UP>,
    platform: P,
}

impl<const C: usize, F, P, UP> OtaModule<C, F, P, UP>
where
    UP: UpitProtocol,
    F: NorFlash + 'static,
    P: OtaPlatform,
    [u8; FlashPartition::<F>::WRITE_SIZE]: Sized,
{
    /// Fails the build if C can't work as a chunk size.
    const CHECK: () = assert!(C > CHUNK_ENVELOPE && C - CHUNK_ENVELOPE <= u16::MAX as usize);

    pub fn new(partition: FlashPartition<F>, platform: P) -> Self {
        let () = Self::CHECK;
        tracing::debug!(tag = "ota", "creating ota module. chunk size {}, mtu {}, write size {}", C, C - CHUNK_ENVELOPE, F::WRITE_SIZE);
        Self {
            partition,
            platform,
            state: OtaState::Idle,
            protocol: PhantomData,
        }
    }

    fn try_begin<'b>(&mut self, req: UpdateRequest, hdr_buf: &'b mut [u8]) -> Result<UpdateResponse<'b>, UpdateError> {
        tracing::debug!(tag = "ota", "trying to begin ota update. hash {:?}", req.crc.0);
        if !matches!(self.state, OtaState::Idle) {
            tracing::info!(tag = "ota", "ota is busy, rejected update request");
            return Err(UpdateError::Busy);
        }
        if req.size as usize > self.partition.capacity() {
            tracing::warn!(tag = "ota", "update too big, rejected. requested size {}, partition capacity {}", req.size, self.partition.capacity());
            return Err(UpdateError::TooBig);
        }

        let specs = DeviceSpecs {
            mtu: (C - CHUNK_ENVELOPE) as u16,
            alignment: F::WRITE_SIZE as u16,
        };

        // Negotiate before wiping, no point erasing flash for an update we refuse.
        let (assembler, headers) = UP::accept(&req, &specs)?;
        let headers = postcard::to_slice(&headers, hdr_buf).map_err(|_| UpdateError::Platform)?;

        self.partition.wipe().map_err(|_| UpdateError::Flash)?;

        self.state = OtaState::Progress {
            writer: UpdateWriter::new(assembler),
            request: req,
            last_activity: Instant::now(),
        };

        tracing::info!(tag = "ota", "update session started, waiting for chunks");

        Ok(UpdateResponse {
            protocol_kind: UP::KIND,
            specs,
            headers: Cow::Borrowed(headers),
        })
    }

    fn begin(&mut self, req: UpdateRequest) -> OtaResult {
        tracing::debug!(tag = "ota", "handling update request. size {}", req.size);

        let mut hdr_buf = [0u8; 32]; // protocol headers, msg borrows from this
        let mut out_buf = [0u8; 64]; // the encoded reply

        let msg = match self.try_begin(req, &mut hdr_buf) {
            Ok(r) => {
                tracing::debug!(tag = "ota", "update request accepted");
                OtaReply::Accepted(r)
            }
            Err(e) => {
                tracing::debug!(tag = "ota", "update request rejected");
                OtaReply::Rejected(e)
            }
        };

        let Ok(used) = postcard::to_slice(&msg, &mut out_buf) else {
            tracing::error!(tag = "ota", "failed to encode reply, resetting to idle");
            self.state = OtaState::Idle; // try_begin already wiped and set Progress
            return OtaResult::RequestResult(Vec::new()); //TODO: Return error instead of empty vec
        };
        let bytes = OtaBytes::from_slice(used).unwrap(); // out_buf is the same size as OtaBytes, can't fail

        tracing::debug!(tag = "ota", "sending request reply, {} bytes", bytes.len());

        OtaResult::RequestResult(bytes)
    }

    //TODO: refactor the last chunks detection logic
    fn handle_chunk(&mut self, raw: OtaChunk<C>) -> OtaResult {
        tracing::trace!(tag = "ota", "got raw chunk, {} bytes", raw.data.len());

        let OtaState::Progress { writer, request, last_activity } = &mut self.state else {
            tracing::warn!(tag = "ota", "got chunk without an active session, ignoring");
            return OtaResult::Callback(Vec::new()); // TODO: NoSession bytes
        };

        *last_activity = Instant::now();
        let size = request.size;

        let Ok(chunk) = postcard::from_bytes::<UpdateChunk>(&raw.data) else {
            tracing::warn!(tag = "ota", "failed to decode chunk, {} bytes dropped", raw.data.len());
            return OtaResult::Nothing;
        };

        let Ok(write_result) = writer.write(&mut self.partition, &chunk) else {
            tracing::error!(tag = "ota", "failed to write chunk, dropping session");
            self.state = OtaState::Idle;
            return OtaResult::Callback(Vec::new()); // TODO: Flash error bytes
        };

        let Some(end) = write_result else {
            tracing::trace!(tag = "ota", "chunk accepted, nothing flushed to flash yet");
            return OtaResult::Nothing;
        };

        tracing::debug!(tag = "ota", "chunk written, progress {}/{} bytes", end, size);

        if end == size {
            tracing::info!(tag = "ota", "all {} bytes received", size);

            let (matches, hash) = writer.verify(&mut self.partition, &request).unwrap();

            if !matches {
                tracing::info!(tag = "ota", "update hashes mismatch, request {} written {}", request.hash, hash);
                return OtaResult::Nothing;
            }

            tracing::info!("hashes match. written {} request {}", hash, request.hash);

            // TODO: verify hash against request.crc before this
            if self.platform.set_boot_target().is_err() {
                tracing::error!(tag = "ota", "failed to set boot target, dropping session");
                self.state = OtaState::Idle;
                return OtaResult::Nothing; // TODO: Platform error bytes
            }
            tracing::info!(tag = "ota", "boot target set, update complete, ready to reboot");
            self.state = OtaState::Rebooting;
            return OtaResult::Callback(Vec::new()); // TODO: success bytes
        }

        OtaResult::Nothing // TODO: assembler callback -> OtaResult::Callback(bytes)
    }
}

impl<const C: usize, F, P, UP> Actor for OtaModule<C, F, P, UP>
where
    UP: UpitProtocol,
    F: NorFlash + 'static,
    P: OtaPlatform,
    [u8; FlashPartition::<F>::WRITE_SIZE]: Sized,
{
    type Handle = OtaHandle<C>;

    async fn act(&mut self, mut inbox: ivy_types::actor::Inbox<<Self::Handle as ivy_types::actor::ActorHandle>::Cmd>) -> ! {
        loop {
            let deadline = match self.state {
                OtaState::Progress { last_activity, .. } => last_activity + IDLE_TIMEOUT,
                _ => Instant::MAX,
            };
            tracing::debug!(tag = "ota", "waiting for next ota event");
            let cmd = match select(inbox.next(), Timer::at(deadline)).await {
                Either::First(cmd) => cmd,
                Either::Second(_) => {
                    tracing::warn!(tag = "ota", "idle timeout, dropping session");
                    self.state = OtaState::Idle;
                    continue;
                }
            };

            match cmd {
                OtaHandleCommand::Event(c, event) => {
                    let reply = match event {
                        OtaEvent::Chunk(chunk) => {
                            tracing::trace!(tag = "ota", "got chunk event");
                            self.handle_chunk(chunk)
                        }
                        OtaEvent::Request(request) => {
                            tracing::debug!(tag = "ota", "got request event");
                            self.begin(request)
                        }
                    };

                    tracing::trace!(tag = "ota", "replying to event");
                    c.reply(reply).await;
                }
                OtaHandleCommand::Abort(c) => {
                    tracing::info!(tag = "ota", "abort requested, dropping session");
                    //TODO: implement aborting
                    self.state = OtaState::Idle;
                    c.ack().await;
                }
            }
        }
    }
}

//TODO: Finish platform implementation
pub trait OtaPlatform {
    fn set_boot_target(&mut self) -> Result<(), ()>;
    fn reboot(&mut self) -> !;
}
