pub mod p2p;
pub mod p2p_interface;

pub use p2p::P2P;
pub use p2p_interface::{Inbox, NodeState, Outbox};
