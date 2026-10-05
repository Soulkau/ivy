use serde::{Deserialize, Serialize};

pub struct CodecError {}

pub trait Encode<C> {
    fn encode(&self, buf: &mut [u8]) -> Result<usize, CodecError>;
}

pub trait Decode<'a, C>: Sized {
    fn decode(buf: &'a [u8]) -> Result<Self, CodecError>;
}

pub struct Json;

pub struct Postcard;

// Json
impl<T> Encode<Json> for T
where
    T: Serialize,
{
    fn encode(&self, buf: &mut [u8]) -> Result<usize, CodecError> {
        serde_json_core::to_slice(self, buf).map_err(|_| CodecError {})
    }
}

impl<'a, T> Decode<'a, Json> for T
where
    T: Deserialize<'a>,
{
    fn decode(buf: &'a [u8]) -> Result<Self, CodecError> {
        serde_json_core::from_slice(buf).map(|(value, _)| value).map_err(|_| CodecError {})
    }
}

// Postcard
impl<T> Encode<Postcard> for T
where
    T: Serialize,
{
    fn encode(&self, buf: &mut [u8]) -> Result<usize, CodecError> {
        postcard::to_slice(self, buf).map(|encoded| encoded.len()).map_err(|_| CodecError {})
    }
}

impl<'a, T> Decode<'a, Postcard> for T
where
    T: Deserialize<'a>,
{
    fn decode(buf: &'a [u8]) -> Result<Self, CodecError> {
        postcard::from_bytes(buf).map_err(|_| CodecError {})
    }
}
