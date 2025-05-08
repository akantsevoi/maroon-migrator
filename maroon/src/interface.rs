use libp2p::PeerId;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fmt::Debug,
};
use tokio::sync::mpsc;

/// Inter-modules communication
pub struct P2PChannels {
    pub receiver: mpsc::UnboundedReceiver<Inbox>,
    pub sender: mpsc::UnboundedSender<Outbox>,
}

/// Input for p2p layer from higher modules perspective
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", content = "data")]
pub enum Outbox {
    State(NodeState),
}

/// Input for the layer that lives on top of p2p layer. Output for p2p Layer
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", content = "data")]
pub enum Inbox {
    State((PeerId, NodeState)),
    Nodes(HashSet<PeerId>),
}

// Node state
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct NodeState {
    pub offsets: HashMap<KeyRange, KeyOffset>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct KeyRange(pub u64);

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct KeyOffset(pub u64);
