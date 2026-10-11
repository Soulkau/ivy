use core::marker::PhantomData;

use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};

use crate::codecs::{Codec, CodecError, Decode};

///Trait alias for types that can be decoded inside of `Subscription`
pub trait Decodable<C: Codec>: for<'a> Decode<'a, C> + Send + 'static {}
///Blanket impl of any Codec<T> type that is Send
impl<T, C: Codec> Decodable<C> for T where T: for<'a> Decode<'a, C> + Send + 'static {}

#[derive(Clone, Copy)]
pub struct Subscription<T: 'static, C>
where
    T: Decodable<C>,
    C: Codec,
{
    topic: &'static str,
    signal: &'static Signal<CriticalSectionRawMutex, T>,
    _codec: PhantomData<fn() -> C>,
}

impl<T, C> Subscription<T, C>
where
    T: Decodable<C>,
    C: Codec,
{
    pub const fn new(topic: &'static str, signal: &'static Signal<CriticalSectionRawMutex, T>) -> Self {
        Self { topic, signal, _codec: PhantomData }
    }

    pub async fn next(&self) -> T {
        self.signal.wait().await
    }
}

pub trait ErasedSubscription: Sync {
    fn topic(&self) -> &'static str;
    fn dispatch(&self, buf: &[u8]) -> Result<(), CodecError>;
}

impl<T, C> ErasedSubscription for Subscription<T, C>
where
    T: Decodable<C>,
    C: Codec,
{
    fn topic(&self) -> &'static str {
        self.topic
    }

    fn dispatch(&self, buf: &[u8]) -> Result<(), CodecError> {
        let value = <T as Decode<C>>::decode(buf)?;
        self.signal.signal(value);
        Ok(())
    }
}
