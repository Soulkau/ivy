use core::marker::PhantomData;

use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};

use crate::codecs::{Codec, CodecError, Decode, Json};

///Trait alias for types that can be decoded inside of `Subscription`
pub trait Decodable<C: Codec>: for<'a> Decode<'a, C> + Send + 'static {}
///Blanket impl of any Codec<T> type that is Send
impl<T, C: Codec> Decodable<C> for T where T: for<'a> Decode<'a, C> + Send + 'static {}

/// Struct to poll messages for `topic`
///
/// Used by both: `MqttModule` to dispatch messages for `topic` and user to await messages by `next`
pub struct Subscription<T: 'static, C = Json>
where
    T: Decodable<C>,
    C: Codec,
{
    topic: &'static str,
    signal: &'static Signal<CriticalSectionRawMutex, T>,
    _codec: PhantomData<fn() -> C>,
}

impl<T, C> Clone for Subscription<T, C>
where
    T: Decodable<C>,
    C: Codec,
{
    fn clone(&self) -> Self {
        *self
    }
}

impl<T, C> Copy for Subscription<T, C>
where
    T: Decodable<C>,
    C: Codec,
{
}

impl<T, C> Subscription<T, C>
where
    T: Decodable<C>,
    C: Codec,
{
    pub const fn new(topic: &'static str, signal: &'static Signal<CriticalSectionRawMutex, T>) -> Self {
        Self { topic, signal, _codec: PhantomData }
    }

    /// Waits for the next message on this subscription.
    ///
    /// Only the most recent message is kept: if several arrive before
    /// this is polled, earlier ones are dropped.
    pub async fn next(&self) -> T {
        self.signal.wait().await
    }
}
/// Used by `MqttModule` to erase generics from subscriptions and dispatch messages
pub trait ErasedSubscription: Sync {
    ///Returns topic of subscription
    fn topic(&self) -> &'static str;
    ///Passes buf to subscription
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

    /// Dispatches message
    ///
    /// Note: if deserialization fails, signal is left untouched (old value stays)
    fn dispatch(&self, buf: &[u8]) -> Result<(), CodecError> {
        let value = <T as Decode<C>>::decode(buf)?;
        self.signal.signal(value);
        Ok(())
    }
}
