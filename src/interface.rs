use libp2p::PeerId;
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, fmt::Debug};
use tokio::sync::mpsc;

pub struct P2PChannels {
    pub receiver: mpsc::UnboundedReceiver<Inbox>,
    pub sender: mpsc::UnboundedSender<Outbox>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", content = "data")]
pub enum Outbox {
    State(NodeState),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", content = "data")]
pub enum Inbox {
    State((PeerId, NodeState)),
    Nodes(HashSet<PeerId>),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct NodeState {
    pub value: i32,
}
