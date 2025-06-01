use std::{num::NonZeroUsize, time::Duration};

#[derive(Clone, Copy, Debug)]
pub struct Params {
  /// how often node will send state info to other nodes
  pub advertise_period: std::time::Duration,
  /// minimum amount of nodes that should have the same transactions(+ current one) in order to confirm them
  /// TODO: separate pub struct ConsensusAlgoParams in a separate lib/consensus crate with its own test suite?
  pub consensus_nodes: NonZeroUsize,

  /// TODO: it will be logical time in the future
  ///
  /// periods between epochs <br>
  /// this parameter only says **when** you should start a new epoch <br>
  /// however due to multiple reasons a new epoch might not start after this period
  pub epoch_period: std::time::Duration,
}

impl Params {
  pub fn default() -> Params {
    Params {
      advertise_period: Duration::from_secs(5),
      consensus_nodes: NonZeroUsize::new(2).unwrap(),
      epoch_period: Duration::from_secs(10),
    }
  }

  pub fn set_advertise_period(mut self, new_period: Duration) -> Params {
    self.advertise_period = new_period;
    self
  }

  pub fn set_consensus_nodes(mut self, n_consensus: NonZeroUsize) -> Params {
    self.consensus_nodes = n_consensus;
    self
  }

  pub fn set_epoch_period(mut self, new_period: Duration) -> Params {
    self.epoch_period = new_period;
    self
  }
}
