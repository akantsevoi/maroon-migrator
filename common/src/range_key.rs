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
pub struct TransactionID(pub u64);

///
const SINGLE_BLOB_SIZE: u64 = 1 << 30; // 1_073_741_824
const MAX_BLOCK_INDEX: u64 = (1 << (64 - 30)) - 1; // [0:17_179_869_184)

pub fn min_and_max_keys_for_range(range: KeyRange) -> (TransactionID, TransactionID) {
  if range.0 > MAX_BLOCK_INDEX {
    panic!("index can't be more than {}", MAX_BLOCK_INDEX);
  }

  (
    TransactionID(range.0 * SINGLE_BLOB_SIZE),
    TransactionID(range.0 * SINGLE_BLOB_SIZE + SINGLE_BLOB_SIZE - 1),
  )
}

pub fn range_index_by_key(global_key: TransactionID) -> KeyRange {
  if global_key.0 > MAX_BLOCK_INDEX * SINGLE_BLOB_SIZE {
    panic!("out of range");
  }

  KeyRange(global_key.0 / SINGLE_BLOB_SIZE)
}

/// Converts full id (TransactionID) into range and offset.
/// ```
/// use common::range_key::key_from_range_and_offset;
/// use common::range_key::TransactionID;
/// use common::range_key::range_offset_from_key;
///
/// let id = TransactionID(10);
/// let (range, offset) = range_offset_from_key(id);
/// let id_from = key_from_range_and_offset(range, offset);
/// assert_eq!(id, id_from);
///
/// ```
pub fn range_offset_from_key(global_key: TransactionID) -> (KeyRange, KeyOffset) {
  let range = range_index_by_key(global_key);
  let offset = global_key.0 % SINGLE_BLOB_SIZE;
  (range, KeyOffset(offset))
}

pub fn key_from_range_and_offset(range: KeyRange, offset: KeyOffset) -> TransactionID {
  TransactionID(range.0 * SINGLE_BLOB_SIZE + offset.0)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_range_offset_from_key() {
    let tests = vec![
      (TransactionID(0), KeyRange(0), KeyOffset(0)),
      (TransactionID(1), KeyRange(0), KeyOffset(1)),
      (TransactionID(1_073_741_824), KeyRange(1), KeyOffset(0)),
    ];

    for (key, ex_range, ex_offset) in tests {
      let (range, offset) = range_offset_from_key(key);
      assert_eq!(range, ex_range, "key: {}", key.0);
      assert_eq!(offset, ex_offset, "key: {}", key.0);
      assert_eq!(key_from_range_and_offset(range, offset), key);
    }
  }

  #[test]
  fn test_transaction_id_operation() {
    let tx1 = TransactionID(10);
    let tx2 = TransactionID(15);
    assert_eq!(TransactionID(5), tx2 - tx1);
  }
}
