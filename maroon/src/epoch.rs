use common::range_key::UniqueU64BlobId;
use derive_more::Display;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Display)]
#[display("Epoch {{ increments: {:?}, hash: 0x{:X} }}", increments, hash.iter().fold(0u128, |acc, &x| (acc << 8) | x as u128))]
pub struct Epoch {
  pub increments: Vec<(UniqueU64BlobId, UniqueU64BlobId)>,
  pub hash: [u8; 32],
}

impl Epoch {
  pub fn new(
    increments: Vec<(UniqueU64BlobId, UniqueU64BlobId)>,
    prev_hash: Option<[u8; 32]>,
  ) -> Epoch {
    let mut hasher = Sha256::new();

    // Include previous hash if it exists
    if let Some(prev) = prev_hash {
      hasher.update(&prev);
    }

    // Include current epoch data
    for (start, end) in &increments {
      hasher.update(start.0.to_le_bytes());
      hasher.update(end.0.to_le_bytes());
    }

    let hash = hasher.finalize().into();

    Epoch { increments, hash }
  }
}
