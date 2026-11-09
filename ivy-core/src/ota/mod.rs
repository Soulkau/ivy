use core::marker::PhantomData;

use alloc::borrow::Cow;
use embassy_time::{Duration, Timer};
use embedded_storage::nor_flash::{NorFlash, ReadNorFlash};
use heapless::Vec;
use ivy_macros::actor_handle;
use ivy_types::actor::Actor;
use upit::{DeviceSpecs, OtaReply, UpdateChunk, UpdateError, UpdateRequest, UpdateResponse, UpitProtocol, flash::UpdateWriter, protocols::offset::OffsetProtocolV1};

use crate::flash::FlashPartition;

pub mod client;

/// The wire adds a length prefix to every chunk, so not all of C is data.
const CHUNK_ENVELOPE: usize = 3;
/// How long to wait before rebooting, so the transport can publish the final reply.
const REBOOT_GRACE: Duration = Duration::from_millis(500);

/// One raw packet from the transport. Only the module decodes it.
pub struct OtaChunk<const S: usize> {
    pub data: Vec<u8, S>,
}

pub type OtaBytes = Vec<u8, 64>;

/// What `pass_chunk` gives back.
pub enum OtaResult {
    /// Chunk written, nothing to say.
    Continue,
    /// Protocol message in the middle of an update. Session stays open.
    Callback(OtaBytes),
    /// Update is over (done or failed). Send it and stop listening for chunks.
    End(OtaBytes),
}

/// What `ask` gives back.
pub enum OtaAnswer {
    /// Send it, then start listening for chunks.
    Accepted(OtaBytes),
    /// Send it, then go back to waiting for requests.
    Rejected(OtaBytes),
}

#[actor_handle(OtaHandle)]
pub trait OtaHandle<const C: usize> {
    async fn ask(&self, request: UpdateRequest) -> OtaAnswer;
    async fn pass_chunk(&self, chunk: OtaChunk<C>) -> OtaResult;
    async fn abort(&self);
}

pub enum OtaState<P: UpitProtocol> {
    Idle,
    Progress { writer: UpdateWriter<P>, request: UpdateRequest },
    Rebooting,
}

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
        Self {
            partition,
            platform,
            state: OtaState::Idle,
            protocol: PhantomData,
        }
    }

    fn try_begin<'b>(&mut self, req: UpdateRequest, hdr_buf: &'b mut [u8]) -> Result<UpdateResponse<'b>, UpdateError> {
        if !matches!(self.state, OtaState::Idle) {
            return Err(UpdateError::Busy);
        }
        if req.size as usize > self.partition.capacity() {
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
        };

        Ok(UpdateResponse {
            protocol_kind: UP::KIND,
            specs,
            headers: Cow::Borrowed(headers),
        })
    }

    fn begin(&mut self, req: UpdateRequest) -> OtaAnswer {
        let mut hdr_buf = [0u8; 32]; // protocol headers, msg borrows from this
        let mut out_buf = [0u8; 64]; // the encoded reply

        let msg = match self.try_begin(req, &mut hdr_buf) {
            Ok(r) => OtaReply::Accepted(r),
            Err(e) => OtaReply::Rejected(e),
        };
        let accepted = matches!(msg, OtaReply::Accepted(_));

        let Ok(used) = postcard::to_slice(&msg, &mut out_buf) else {
            self.state = OtaState::Idle; // try_begin already wiped and set Progress
            return OtaAnswer::Rejected(Vec::new()); //TODO: Return error instead of empty vec
        };
        let bytes = OtaBytes::from_slice(used).unwrap(); // out_buf is the same size as OtaBytes, can't fail

        if accepted { OtaAnswer::Accepted(bytes) } else { OtaAnswer::Rejected(bytes) }
    }

    fn on_chunk(&mut self, raw: OtaChunk<C>) -> OtaResult {
        let OtaState::Progress { writer, request } = &mut self.state else {
            return OtaResult::End(Vec::new()); // TODO: NoSession bytes
        };
        let size = request.size;

        let Ok(chunk) = postcard::from_bytes::<UpdateChunk>(&raw.data) else {
            return OtaResult::Continue; // garbage, the idle timeout catches a dead sender
        };

        let Ok(write_result) = writer.write(&mut self.partition, &chunk) else {
            self.state = OtaState::Idle; // flash error, session is dead
            return OtaResult::End(Vec::new()); // TODO: Flash error bytes
        };

        // None means the assembler refused the chunk and nothing was written
        let Some(end) = write_result else {
            return OtaResult::Continue;
        };

        if end == size {
            // TODO: verify hash against request.crc before this
            if self.platform.set_boot_target().is_err() {
                self.state = OtaState::Idle;
                return OtaResult::End(Vec::new()); // TODO: Platform error bytes
            }
            self.state = OtaState::Rebooting;
            return OtaResult::End(Vec::new()); // TODO: success bytes
        }

        OtaResult::Continue // TODO: assembler callback -> OtaResult::Callback(bytes)
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
            match inbox.next().await {
                OtaHandleCommand::Ask(c, req) => c.reply(self.begin(req)).await,
                OtaHandleCommand::PassChunk(c, chunk) => {
                    c.reply(self.on_chunk(chunk)).await;
                    if matches!(self.state, OtaState::Rebooting) {
                        Timer::after(REBOOT_GRACE).await; // let the transport publish the final reply
                        self.platform.reboot();
                    }
                }
                OtaHandleCommand::Abort(c) => {
                    self.state = OtaState::Idle;
                    c.reply(()).await
                }
            }
        }
    }
}

pub trait OtaPlatform {
    fn set_boot_target(&mut self) -> Result<(), ()>;
    fn reboot(&mut self) -> !;
}
