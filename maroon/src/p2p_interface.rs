use common::{
  range_key::{KeyOffset, KeyRange, U64BlobIdClosedInterval},
  transaction::Transaction,
};
use libp2p::PeerId;
use serde::{Deserialize, Serialize};
use std::{
  collections::{HashMap, HashSet},
  fmt::Debug,
};

/// Input for p2p layer from higher modules perspective
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "type", content = "data")]
pub enum Outbox {
  State(NodeState),

  RequestMissingTxs((PeerId, Vec<U64BlobIdClosedInterval>)),
  RequestedTxsForPeer((PeerId, Vec<Transaction>)),
}

/// Input for the layer that lives on top of p2p layer. Output for p2p Layer
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", content = "data")]
pub enum Inbox {
  State((PeerId, NodeState)),
  Nodes(HashSet<PeerId>),
  NewTransaction(Transaction),

  RequestMissingTxs((PeerId, Vec<U64BlobIdClosedInterval>)),
  MissingTx(Vec<Transaction>),
}

// Node state
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct NodeState {
  pub offsets: HashMap<KeyRange, KeyOffset>,
}
