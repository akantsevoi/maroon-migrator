use core::time;
use std::collections::HashMap;
use std::time::Duration;

use crate::app_interface::{
    CurrentOffsets, Request as AppStateRequest, Response as AppStateResponse,
};
use crate::p2p_interface::*;
use crate::test_helpers::{new_test_instance, test_tx};
use common::async_interface::AsyncInterface;
use common::invoker_handler::{InvokerInterface, create_invoker_handler_pair};
use common::range_key::{KeyOffset, KeyRange};
use tokio::sync::oneshot;

#[tokio::test(flavor = "multi_thread")]
async fn app_gets_missing_transaction() {
    let mut interface = AsyncInterface::new();
    let (state_invoker, handler) = create_invoker_handler_pair();
    let mut app = new_test_instance(interface.responder(), handler);
    let (_shutdown_tx, shutdown_rx) = oneshot::channel();

    tokio::spawn(async move {
        app.loop_until_shutdown(shutdown_rx).await;
    });

    let requester = interface.requester();

    // app gets some transaction from the future
    _ = requester.sender.send(Inbox::NewTransaction(test_tx(5)));
    _ = requester.sender.send(Inbox::NewTransaction(test_tx(0)));

    assert!(
        reaches_state(
            3,
            Duration::from_millis(5),
            &state_invoker,
            CurrentOffsets {
                self_offsets: HashMap::from([(KeyRange(0), KeyOffset(0))]),
                consensus_offset: HashMap::new(),
            }
        )
        .await
    );

    // and now app gets missing transaction
    _ = requester.sender.send(Inbox::MissingTx(vec![
        test_tx(3),
        test_tx(4),
        test_tx(2),
        test_tx(0),
        test_tx(1),
    ]));

    assert!(
        reaches_state(
            3,
            Duration::from_millis(5),
            &state_invoker,
            CurrentOffsets {
                self_offsets: HashMap::from([(KeyRange(0), KeyOffset(5))]),
                consensus_offset: HashMap::new(),
            }
        )
        .await
    );
}

async fn reaches_state(
    attempts: u32,
    tick: time::Duration,
    state_invoker: &InvokerInterface<AppStateRequest, AppStateResponse>,
    exp_state: CurrentOffsets,
) -> bool {
    for _ in 0..attempts {
        let AppStateResponse::State(current_state) =
            state_invoker.request(AppStateRequest::GetState).await;

        if exp_state == current_state {
            return true;
        }

        tokio::time::sleep(tick).await;
    }

    return false;
}
