use crate::ota::{OtaEvent, OtaHandle, OtaResult};

#[allow(async_fn_in_trait)]
pub trait OtaTransport<const C: usize> {
    async fn next_event(&mut self) -> OtaEvent<C>;
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
        loop {
            match self.handle.event(t.next_event().await).await {
                OtaResult::RequestResult(b) => t.send_reply(&b).await,
                OtaResult::Callback(b) => t.send_callback(&b).await,
                OtaResult::Nothing => {}
            }
        }
    }
}
