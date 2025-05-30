use crate::range_key::UniqueU64BlobId;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Transaction {
  pub id: UniqueU64BlobId,
  pub status: TxStatus,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(tag = "type", content = "data")]
pub enum TxStatus {
  Created,
  Pending,
  Confirmed,
  Finished,
  Rejected,
}
