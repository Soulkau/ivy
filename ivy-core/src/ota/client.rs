use embassy_time::{Duration, with_timeout};
use upit::UpdateRequest;

use crate::ota::{OtaAnswer, OtaChunk, OtaHandle, OtaResult};

const IDLE_TIMEOUT: Duration = Duration::from_secs(30);

#[allow(async_fn_in_trait)]
pub trait OtaTransport<const C: usize> {
    async fn next_request(&mut self) -> UpdateRequest;
    async fn next_chunk(&mut self) -> OtaChunk<C>;
    async fn send_reply(&mut self, bytes: &[u8]);
    async fn send_callback(&mut self, bytes: &[u8]);
}

pub struct OtaClient<const C: usize> {
    handle: OtaHandle<C>,
}

impl<const C: usize> OtaClient<C> {
    pub fn new(handle: OtaHandle<C>) -> Self {
        tracing::debug!(tag = "ota/client", "creating ota client");

        Self { handle }
    }

    pub async fn run<T: OtaTransport<C>>(&self, t: &mut T) -> ! {
        tracing::info!(tag = "ota/client", "ota client started");

        loop {
            tracing::debug!(tag = "ota/client", "waiting for ota update request");

            let request = t.next_request().await;

            tracing::debug!(tag = "ota/client", "received ota update request");

            match self.handle.ask(request).await {
                OtaAnswer::Rejected(b) => {
                    tracing::warn!(tag = "ota/client", "ota update request rejected");

                    t.send_reply(&b).await;

                    tracing::debug!(tag = "ota/client", "sent ota rejection reply");

                    continue;
                }

                OtaAnswer::Accepted(b) => {
                    tracing::info!(tag = "ota/client", "ota update request accepted");

                    t.send_reply(&b).await;

                    tracing::debug!(tag = "ota/client", "sent ota acceptance reply");
                }
            }

            tracing::debug!(tag = "ota/client", "waiting for ota chunks");
            let mut count = 0u32;
            loop {
                let Ok(chunk) = with_timeout(IDLE_TIMEOUT, t.next_chunk()).await else {
                    tracing::warn!(tag = "ota/client", "ota chunk receive timed out, aborting update");

                    self.handle.abort().await;

                    tracing::info!(tag = "ota/client", "ota update aborted after idle timeout");

                    break;
                };
                count += 1;

                tracing::debug!(tag = "ota/client", "received ota chunk number {}", count);

                match self.handle.pass_chunk(chunk).await {
                    OtaResult::Continue => {
                        tracing::debug!(tag = "ota/client", "ota chunk processed, waiting for next chunk");
                    }

                    OtaResult::Callback(b) => {
                        tracing::info!(tag = "ota/client", "ota callback requested");

                        t.send_callback(&b).await;

                        tracing::debug!(tag = "ota/client", "sent ota callback");
                    }

                    OtaResult::End(b) => {
                        tracing::info!(tag = "ota/client", "ota update completed");

                        t.send_reply(&b).await;

                        tracing::debug!(tag = "ota/client", "sent ota completion reply");

                        break;
                    }
                }
            }

            tracing::debug!(tag = "ota/client", "ota session ended, waiting for next request");
        }
    }
}
