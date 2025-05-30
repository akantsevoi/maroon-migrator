use common::range_key::UniqueU64BlobId;
use sha2::digest::generic_array::sequence;

use crate::epoch::Epoch;

pub trait Linearizer {
  fn new_epoch(&mut self, epoch: Epoch);
}

struct LogLineriazer {
  sequence: Vec<UniqueU64BlobId>,
}

impl Linearizer for LogLineriazer {
  fn new_epoch(&mut self, mut epoch: Epoch) {
    epoch.increments.sort_by_key(|pair| pair.0);
    let new_elements_count = epoch.increments.iter().sum()
    self
      .sequence
      .resize(epoch.increments.len(), UniqueU64BlobId(0));

    for (left, right) in &epoch.increments {}

    println!("new epoch: {epoch}");
  }
}

struct AppImitation<L: Linearizer> {
  linearizer: L,
}

impl<L: Linearizer> AppImitation<L> {
  fn some(&mut self) {
    self.linearizer.new_epoch(Epoch::new(
      vec![(UniqueU64BlobId(0), UniqueU64BlobId(15))],
      None,
    ));
  }
}

#[test]
fn test_linear() {
  let mut app = AppImitation::<LogLineriazer> {
    linearizer: LogLineriazer { sequence: vec![] },
  };

  app.some();
}
