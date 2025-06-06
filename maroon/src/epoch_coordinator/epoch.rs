use common::range_key::U64BlobIdClosedInterval;
use derive_more::Display;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Display, Serialize, Deserialize)]
#[display("Epoch {{ increments: {:?}, hash: 0x{:X} }}", increments, hash.iter().fold(0u128, |acc, &x| (acc << 8) | x as u128))]
pub struct Epoch {
  pub increments: Vec<U64BlobIdClosedInterval>,
  pub hash: [u8; 32],
}

impl Epoch {
  pub fn new(increments: Vec<U64BlobIdClosedInterval>, prev_hash: Option<[u8; 32]>) -> Epoch {
    let mut hasher = Sha256::new();

    // Include previous hash if it exists
    if let Some(prev) = prev_hash {
      hasher.update(&prev);
    }

    // Include current epoch data
    for interval in &increments {
      hasher.update(interval.start().0.to_le_bytes());
      hasher.update(interval.end().0.to_le_bytes());
    }

    let hash = hasher.finalize().into();

    Epoch { increments, hash }
  }
}
