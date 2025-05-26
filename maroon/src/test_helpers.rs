use crate::app::App;
use crate::app_interface::{Request as AppStateRequest, Response as AppStateResponse};
use crate::p2p_interface::{Inbox, Outbox};
use common::invoker_handler::HandlerInterface;
use common::{
    async_interface::ReqResPair,
    range_key::TransactionID,
    transaction::{Transaction, TxStatus},
};
use libp2p::PeerId;

#[cfg(test)]
pub fn new_test_instance(
    p2p_interface: ReqResPair<Outbox, Inbox>,
    state_interface: HandlerInterface<AppStateRequest, AppStateResponse>,
) -> App {
    App::new(
        PeerId::random(),
        p2p_interface,
        state_interface,
        crate::app::Params::default(),
    )
    .expect("failed to create test App instance")
}

#[cfg(test)]
pub fn test_tx(id: u64) -> Transaction {
    Transaction {
        id: TransactionID(id),
        status: TxStatus::Pending,
    }
}
