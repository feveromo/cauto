use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CacheEnvelope<T> {
    pub schema_version: u32,
    pub cauto_version: String,
    pub codex_version: String,
    pub codex_binary_fingerprint: String,
    pub codex_home_hash: String,
    pub profile: Option<String>,
    pub fetched_at: String,
    pub fetched_at_unix: u64,
    pub source: String,
    pub payload_sha256: String,
    pub catalog: T,
}

impl<T: Serialize> CacheEnvelope<T> {
    pub fn payload_digest(payload: &T) -> Result<String, serde_json::Error> {
        let bytes = serde_json::to_vec(payload)?;
        Ok(format!("{:x}", Sha256::digest(bytes)))
    }

    pub fn refresh_digest(&mut self) -> Result<(), serde_json::Error> {
        self.payload_sha256 = Self::payload_digest(&self.catalog)?;
        Ok(())
    }

    pub fn digest_is_valid(&self) -> Result<bool, serde_json::Error> {
        Ok(self.payload_sha256 == Self::payload_digest(&self.catalog)?)
    }
}

impl<T: DeserializeOwned> CacheEnvelope<T> {
    pub fn parse(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }
}
