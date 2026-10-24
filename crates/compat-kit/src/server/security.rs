use super::Request;
use crate::files::{Result, io};
use std::{fs::File, io::Read};

pub(super) struct LaunchSecurity {
    address: String,
    token: [u8; 64],
}
impl LaunchSecurity {
    pub(super) fn new(address: &str) -> Result<Self> {
        let mut random = [0_u8; 32];
        File::open("/dev/urandom")
            .and_then(|mut file| file.read_exact(&mut random))
            .map_err(io)?;
        let mut token = [0; 64];
        let hex = b"0123456789abcdef";
        for (index, byte) in random.iter().enumerate() {
            token[index * 2] = hex[usize::from(byte >> 4)];
            token[index * 2 + 1] = hex[usize::from(byte & 15)];
        }
        Ok(Self {
            address: address.into(),
            token,
        })
    }
    pub(super) fn token(&self) -> &str {
        // The token is encoded exclusively from ASCII hex digits at construction.
        std::str::from_utf8(&self.token).expect("ASCII launch token")
    }
    pub(super) fn trusts_peer(&self, req: &Request) -> bool {
        let Some(host) = req.headers.get("host") else {
            return false;
        };
        if host != &self.address && host != &self.address.replace("127.0.0.1", "localhost") {
            return false;
        }
        req.headers
            .get("origin")
            .is_none_or(|origin| origin == &format!("http://{host}"))
    }
    pub(super) fn authorizes_mutation(&self, req: &Request) -> bool {
        let Some(value) = req.headers.get("x-compat-token") else {
            return false;
        };
        let Ok(candidate) = <&[u8; 64]>::try_from(value.as_bytes()) else {
            return false;
        };
        // Compare every byte for fixed-length candidates without a prefix-dependent exit.
        candidate
            .iter()
            .zip(&self.token)
            .fold(0_u8, |difference, (a, b)| difference | (a ^ b))
            == 0
    }
}
