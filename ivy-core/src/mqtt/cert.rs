use embedded_tls::{Aes128GcmSha256, Certificate, CryptoProvider, CryptoRngCore, TlsClock, TlsVerifier, pki::CertVerifier};

pub struct NoClock;
impl TlsClock for NoClock {
    fn now() -> Option<u64> {
        None
    }
}

pub struct MqttTlsProvider<Rng> {
    rng: Rng,
    verifier: CertVerifier<'static, Aes128GcmSha256, NoClock, 4096>,
}

impl<Rng: CryptoRngCore> MqttTlsProvider<Rng> {
    pub fn new(rng: Rng, crt: &'static [u8]) -> Self {
        Self {
            rng,
            verifier: CertVerifier::new(Certificate::X509(crt)),
        }
    }
}

impl<Rng: CryptoRngCore> CryptoProvider for MqttTlsProvider<Rng> {
    type CipherSuite = Aes128GcmSha256;
    type Signature = &'static [u8];

    fn rng(&mut self) -> impl CryptoRngCore {
        &mut self.rng
    }

    fn verifier(&mut self) -> Result<&mut impl TlsVerifier<Self::CipherSuite>, embedded_tls::TlsError> {
        Ok(&mut self.verifier)
    }
}
