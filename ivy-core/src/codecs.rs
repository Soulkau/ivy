use core::fmt::Write;
use serde::{Deserialize, Serialize};

// Add variants if they are useful for controlflow
#[derive(Debug, thiserror::Error)]
pub enum CodecError {
    #[error("codec output buffer is full")]
    BufferFull,

    #[error("{0}")]
    Displayed(heapless::String<128>),
}

impl CodecError {
    fn displayed<E: core::fmt::Display>(error: E) -> Self {
        let mut message = heapless::String::<128>::new();
        let _ = write!(&mut message, "{error}");
        Self::Displayed(message)
    }
}

pub trait Encode<C: Codec> {
    fn encode(&self, buf: &mut [u8]) -> Result<usize, CodecError>;
}

pub trait Decode<'a, C: Codec>: Sized {
    fn decode(buf: &'a [u8]) -> Result<Self, CodecError>;
}
/// Just a marker trait for codecs.
pub trait Codec: 'static {}

/// Json codec using [`serde_json_core`].
pub struct Json;

impl Codec for Json {}

impl<T> Encode<Json> for T
where
    T: Serialize,
{
    fn encode(&self, buf: &mut [u8]) -> Result<usize, CodecError> {
        serde_json_core::to_slice(self, buf).map_err(CodecError::from)
    }
}

impl<'a, T> Decode<'a, Json> for T
where
    T: Deserialize<'a>,
{
    fn decode(buf: &'a [u8]) -> Result<Self, CodecError> {
        serde_json_core::from_slice(buf).map(|(value, _)| value).map_err(CodecError::from)
    }
}

impl From<serde_json_core::de::Error> for CodecError {
    fn from(error: serde_json_core::de::Error) -> Self {
        Self::displayed(error)
    }
}
impl From<serde_json_core::ser::Error> for CodecError {
    fn from(error: serde_json_core::ser::Error) -> Self {
        match error {
            serde_json_core::ser::Error::BufferFull => Self::BufferFull,
            _ => Self::displayed(error),
        }
    }
}

/// Postcard codec using [`postcard`].
pub struct Postcard;

impl Codec for Postcard {}

impl<T> Encode<Postcard> for T
where
    T: Serialize,
{
    fn encode(&self, buf: &mut [u8]) -> Result<usize, CodecError> {
        postcard::to_slice(self, buf).map(|encoded| encoded.len()).map_err(CodecError::from)
    }
}

impl<'a, T> Decode<'a, Postcard> for T
where
    T: Deserialize<'a>,
{
    fn decode(buf: &'a [u8]) -> Result<Self, CodecError> {
        postcard::from_bytes(buf).map_err(CodecError::from)
    }
}

impl From<postcard::Error> for CodecError {
    fn from(error: postcard::Error) -> Self {
        match error {
            postcard::Error::SerializeBufferFull => Self::BufferFull,
            _ => Self::displayed(error),
        }
    }
}
