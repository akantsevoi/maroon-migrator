use derive_more::{Add, AddAssign, Display, Sub};
use serde::{Deserialize, Serialize};

// TODO: KeyRange and KeyOffset shouldn't be u64 since their combination fits into u64
//
#[derive(
  Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Display,
)]
pub struct KeyRange(pub u64);

#[derive(
  Serialize,
  Deserialize,
  AddAssign,
  Debug,
  Clone,
  Copy,
  PartialEq,
  Eq,
  Hash,
  PartialOrd,
  Ord,
  Add,
  Display,
  Sub,
)]
pub struct KeyOffset(pub u64);

/// Unique identifier for a transaction
#[derive(
  Serialize,
  Deserialize,
  Debug,
  Clone,
  Copy,
  PartialEq,
  Eq,
  Hash,
  PartialOrd,
  Ord,
  Add,
  AddAssign,
  Sub,
  Display,
)]
pub struct UniqueU64BlobId(pub u64);

///
const SINGLE_BLOB_SIZE: u64 = 1 << 30; // 1_073_741_824
const MAX_BLOCK_INDEX: u64 = (1 << (64 - 30)) - 1; // [0:17_179_869_184)

pub fn min_and_max_keys_for_range(range: KeyRange) -> (UniqueU64BlobId, UniqueU64BlobId) {
  if range.0 > MAX_BLOCK_INDEX {
    panic!("index can't be more than {}", MAX_BLOCK_INDEX);
  }

  (
    UniqueU64BlobId(range.0 * SINGLE_BLOB_SIZE),
    UniqueU64BlobId(range.0 * SINGLE_BLOB_SIZE + SINGLE_BLOB_SIZE - 1),
  )
}

pub fn range_from_unique_blob_id(global_key: UniqueU64BlobId) -> KeyRange {
  if global_key.0 > MAX_BLOCK_INDEX * SINGLE_BLOB_SIZE {
    panic!("out of range");
  }

  KeyRange(global_key.0 / SINGLE_BLOB_SIZE)
}

/// Converts full id (UniqueU64BlobId) into range and offset.
/// ```
/// use common::range_key::unique_blob_id_from_range_and_offset;
/// use common::range_key::UniqueU64BlobId;
/// use common::range_key::range_offset_from_unique_blob_id;
///
/// let id = UniqueU64BlobId(10);
/// let (range, offset) = range_offset_from_unique_blob_id(id);
/// let id_from = unique_blob_id_from_range_and_offset(range, offset);
/// assert_eq!(id, id_from);
///
/// ```
pub fn range_offset_from_unique_blob_id(global_key: UniqueU64BlobId) -> (KeyRange, KeyOffset) {
  let range = range_from_unique_blob_id(global_key);
  let offset = global_key.0 % SINGLE_BLOB_SIZE;
  (range, KeyOffset(offset))
}

pub fn unique_blob_id_from_range_and_offset(range: KeyRange, offset: KeyOffset) -> UniqueU64BlobId {
  UniqueU64BlobId(range.0 * SINGLE_BLOB_SIZE + offset.0)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_range_offset_from_unique_blob_id() {
    let tests = vec![
      (UniqueU64BlobId(0), KeyRange(0), KeyOffset(0)),
      (UniqueU64BlobId(1), KeyRange(0), KeyOffset(1)),
      (UniqueU64BlobId(1_073_741_824), KeyRange(1), KeyOffset(0)),
    ];

    for (key, ex_range, ex_offset) in tests {
      let (range, offset) = range_offset_from_unique_blob_id(key);
      assert_eq!(range, ex_range, "key: {}", key.0);
      assert_eq!(offset, ex_offset, "key: {}", key.0);
      assert_eq!(unique_blob_id_from_range_and_offset(range, offset), key);
    }
  }

  #[test]
  fn test_transaction_id_operation() {
    let tx1 = UniqueU64BlobId(10);
    let tx2 = UniqueU64BlobId(15);
    assert_eq!(UniqueU64BlobId(5), tx2 - tx1);
  }
}
