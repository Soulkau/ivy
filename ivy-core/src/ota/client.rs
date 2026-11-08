use embassy_time::{Duration, with_timeout};
use upit::UpdateRequest;

use crate::ota::{OtaAnswer, OtaChunk, OtaHandle, OtaResult};

const IDLE_TIMEOUT: Duration = Duration::from_secs(30);

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
        Self { handle }
    }

    pub async fn run<T: OtaTransport<C>>(&self, t: &mut T) -> ! {
        loop {
            match self.handle.ask(t.next_request().await).await {
                OtaAnswer::Rejected(b) => {
                    t.send_reply(&b).await;
                    continue;
                }
                OtaAnswer::Accepted(b) => t.send_reply(&b).await,
            }

            loop {
                let Ok(chunk) = with_timeout(IDLE_TIMEOUT, t.next_chunk()).await else {
                    self.handle.abort().await;
                    break;
                };

                match self.handle.pass_chunk(chunk).await {
                    OtaResult::Continue => {}
                    OtaResult::Callback(b) => t.send_callback(&b).await,
                    OtaResult::End(b) => {
                        t.send_reply(&b).await;
                        break;
                    }
                }
            }
        }
    }
}
