use core::marker::PhantomData;

use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};

use crate::{codecs::Json, mqtt::MqttError};
use crate::codecs::{Codec, CodecError, Decode};

///Trait alias for types that can be decoded inside of MqttSubscription
pub trait Decodable<C>: for<'a> Decode<'a, C> + Send + 'static {}
///Blanket impl of any Codec<T> type that is Send
impl<T, C> Decodable<C> for T where T: for<'a> Decode<'a, C> + Send + 'static {}

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
    fn dispatch(&self, buf: &[u8]) -> Result<(), MqttError>;
}

impl<T, C> ErasedSubscription for Subscription<T, C>
where
    T: DeserializeOwned + Send + Sync + 'static,
{
    fn topic(&self) -> &'static str {
        self.topic
    }

    fn dispatch(&self, buf: &[u8]) -> Result<(), MqttError> {
        let (value, _) = serde_json_core::from_slice(buf).map_err(MqttError::Decode)?;
        let value = <T as Decode<C>>::decode(buf)?;
        self.signal.signal(value);
        Ok(())
    }
}

/// Macro to pre-declares subscriptions, that will be managed by mqtt durning app lifetime.
#[macro_export]
macro_rules! declare_subcriptions {
    ( $( $name:ident => $topic:literal : $payload:ty ),+ $(,)? ) => {
        {
            $crate::paste::paste! {
                $(
                    static [<$name:upper _SIGNAL>]: ::embassy_sync::signal::Signal<
                        ::embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex,
                        $payload,
                    > = ::embassy_sync::signal::Signal::new();
                )+
                pub struct SubscriberReg {
                    $( pub $name: $crate::mqtt::TypedHandle<$payload>, )+
                }
                impl SubscriberReg {
                    fn new() -> Self {
                        Self {
                            $( $name: $crate::mqtt::TypedHandle::new(&[<$name:upper _SIGNAL>]), )+
                        }
                    }
                }
            }
            // everything below is OUTSIDE paste!, so $crate:: stays intact
            static SUBSCRIBER_REG: ::static_cell::StaticCell<SubscriberReg> =
                ::static_cell::StaticCell::new();
            let reg: &'static SubscriberReg = SUBSCRIBER_REG.init(SubscriberReg::new());
            let handles: [(&'static str, &'static dyn $crate::mqtt::ErasedHandle); $crate::count!($($name)+)] = [
                $( ($topic, &reg.$name as &'static dyn $crate::mqtt::ErasedHandle), )+
            ];
            let subs = $crate::paste::paste! {
                ( $( $crate::mqtt::Subscription::new(&[<$name:upper _SIGNAL>]), )+ )
            };
            (handles, subs)
        }
    };
}
#[macro_export]
macro_rules! count {
    () => { 0 };
    ($_head:ident $($tail:ident)*) => { 1 + count!($($tail)*) };
}
