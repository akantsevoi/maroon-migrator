use std::collections::HashMap;
use std::time::Duration;

use crate::app_interface::CurrentOffsets;
use crate::p2p_interface::*;
use crate::test_helpers::{new_test_instance, reaches_state, test_tx};
use common::async_interface::AsyncInterface;
use common::invoker_handler::create_invoker_handler_pair;
use common::range_key::{KeyOffset, KeyRange, TransactionID};
use libp2p::PeerId;
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

#[tokio::test(flavor = "multi_thread")]
async fn request_transactions_from_app() {
    let mut interface = AsyncInterface::new();
    let (state_invoker, handler) = create_invoker_handler_pair();
    let mut app = new_test_instance(interface.responder(), handler);
    let (_shutdown_tx, shutdown_rx) = oneshot::channel();

    tokio::spawn(async move {
        app.loop_until_shutdown(shutdown_rx).await;
    });

    let mut requester = interface.requester();

    _ = requester.sender.send(Inbox::NewTransaction(test_tx(2)));
    _ = requester.sender.send(Inbox::NewTransaction(test_tx(3)));
    _ = requester.sender.send(Inbox::NewTransaction(test_tx(1)));
    _ = requester.sender.send(Inbox::NewTransaction(test_tx(0)));
    _ = requester.sender.send(Inbox::NewTransaction(test_tx(4)));

    assert!(
        reaches_state(
            3,
            Duration::from_millis(5),
            &state_invoker,
            CurrentOffsets {
                self_offsets: HashMap::from([(KeyRange(0), KeyOffset(4))]),
                consensus_offset: HashMap::new(),
            }
        )
        .await
    );

    let rnd_peer = PeerId::random();
    requester
        .sender
        .send(Inbox::RequestMissingTxs((
            rnd_peer,
            vec![(TransactionID(1), TransactionID(3))],
        )))
        .expect("channel shouldnt be dropped");

    while let Some(outbox) = requester.receiver.recv().await {
        let Outbox::RequestedTxsForPeer((peer, requested_txs)) = outbox else {
            continue;
        };
        assert_eq!(rnd_peer, peer);
        assert_eq!(requested_txs, vec![test_tx(1), test_tx(2), test_tx(3)]);

        break;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn app_detects_that_its_behind_and_makes_request() {
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
