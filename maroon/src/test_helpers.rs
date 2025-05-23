use std::collections::HashMap;

use crate::app::App;
use crate::p2p_interface::{Inbox, Outbox};
use common::async_interface::ReqResPair;
use common::gm_request_response::{Transaction, TxStatus};
use common::range_key::{self, KeyRange, TransactionID};
use libp2p::PeerId;

#[cfg(test)]
pub fn new_test_instance(channels: ReqResPair<Outbox, Inbox>) -> App {
    App::new(PeerId::random(), channels, crate::app::Params::default())
        .expect("failed to create test App instance")
}

#[cfg(test)]
pub fn test_tx(id: u64) -> Transaction {
    Transaction {
        id: TransactionID(id),
        status: TxStatus::Pending,
    }
}
