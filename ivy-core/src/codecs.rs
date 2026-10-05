trait Encode<C> {
    type Error;

    fn encode(&self, buf: &mut [u8]) -> Result<usize, Self::Error>;
}

trait Decode<C>: Sized {
    type Error;

    fn decode(buf: &[u8]) -> Result<Self, Self::Error>;
}


