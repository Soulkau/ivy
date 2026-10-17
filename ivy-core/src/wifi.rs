use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct WifiCredentials {
    pub ssid: &'static str,
    pub password: &'static str,
}
