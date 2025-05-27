use std::collections::HashMap;
use std::time::Duration;

use crate::app_interface::CurrentOffsets;
use crate::p2p_interface::*;
use crate::test_helpers::{new_test_instance, reaches_state, test_tx};
use common::duplex_channel::create_a_b_duplex_pair;
use common::invoker_handler::create_invoker_handler_pair;
use common::range_key::{KeyOffset, KeyRange, TransactionID};
use libp2p::PeerId;
use tokio::sync::oneshot;

///
/// In this file we're testing app as a black box by accessing only publicly available interface module
/// not really integration tests, but not unit either
///

#[tokio::test(flavor = "multi_thread")]
async fn app_calculates_consensus_offset() {
    let (ab_endpoint, ba_endpoint) = create_a_b_duplex_pair::<Inbox, Outbox>();
    let (state_invoker, handler) = create_invoker_handler_pair();
    let mut app = new_test_instance(ba_endpoint, handler);
    let (_shutdown_tx, shutdown_rx) = oneshot::channel();

    let n1_peer_id = PeerId::random();
    let n2_peer_id = PeerId::random();

    tokio::spawn(async move {
        app.loop_until_shutdown(shutdown_rx).await;
    });

    ab_endpoint
        .sender
        .send(Inbox::State((
            n1_peer_id,
            NodeState {
                offsets: HashMap::from([
                    (KeyRange(1), KeyOffset(3)),
                    (KeyRange(2), KeyOffset(7)),
                    (KeyRange(4), KeyOffset(1)),
                ]),
            },
        )))
        .unwrap();
    ab_endpoint
        .sender
        .send(Inbox::State((
            n2_peer_id,
            NodeState {
                offsets: HashMap::from([(KeyRange(1), KeyOffset(2)), (KeyRange(2), KeyOffset(9))]),
            },
        )))
        .unwrap();

    assert!(
        reaches_state(
            3,
            Duration::from_millis(5),
            &state_invoker,
            CurrentOffsets {
                self_offsets: HashMap::new(),
                consensus_offset: HashMap::from([
                    (KeyRange(1), KeyOffset(2)),
                    (KeyRange(2), KeyOffset(7))
                ]),
            }
        )
        .await
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn app_gets_missing_transaction() {
    let (ab_endpoint, ba_endpoint) = create_a_b_duplex_pair::<Inbox, Outbox>();
    let (state_invoker, handler) = create_invoker_handler_pair();
    let mut app = new_test_instance(ba_endpoint, handler);
    let (_shutdown_tx, shutdown_rx) = oneshot::channel();

    tokio::spawn(async move {
        app.loop_until_shutdown(shutdown_rx).await;
    });

    // app gets some transaction from the future
    _ = ab_endpoint.sender.send(Inbox::NewTransaction(test_tx(5)));
    _ = ab_endpoint.sender.send(Inbox::NewTransaction(test_tx(0)));

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
    _ = ab_endpoint.sender.send(Inbox::MissingTx(vec![
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
async fn app_gets_missing_transactions_that_smbd_else_requested() {
    let (mut ab_endpoint, ba_endpoint) = create_a_b_duplex_pair::<Inbox, Outbox>();
    let (state_invoker, handler) = create_invoker_handler_pair();
    let mut app = new_test_instance(ba_endpoint, handler);
    let (_shutdown_tx, shutdown_rx) = oneshot::channel();

    tokio::spawn(async move {
        app.loop_until_shutdown(shutdown_rx).await;
    });

    _ = ab_endpoint.sender.send(Inbox::NewTransaction(test_tx(2)));
    _ = ab_endpoint.sender.send(Inbox::NewTransaction(test_tx(3)));
    _ = ab_endpoint.sender.send(Inbox::NewTransaction(test_tx(1)));
    _ = ab_endpoint.sender.send(Inbox::NewTransaction(test_tx(0)));
    _ = ab_endpoint.sender.send(Inbox::NewTransaction(test_tx(4)));

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
    ab_endpoint
        .sender
        .send(Inbox::RequestMissingTxs((
            rnd_peer,
            vec![(TransactionID(1), TransactionID(3))],
        )))
        .expect("channel shouldnt be dropped");

    while let Some(outbox) = ab_endpoint.receiver.recv().await {
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
    let (mut ab_endpoint, ba_endpoint) = create_a_b_duplex_pair::<Inbox, Outbox>();
    let (state_invoker, handler) = create_invoker_handler_pair();
    let mut app = new_test_instance(ba_endpoint, handler);
    let (_shutdown_tx, shutdown_rx) = oneshot::channel();

    tokio::spawn(async move {
        app.loop_until_shutdown(shutdown_rx).await;
    });

    _ = ab_endpoint.sender.send(Inbox::NewTransaction(test_tx(0)));
    _ = ab_endpoint.sender.send(Inbox::NewTransaction(test_tx(4)));

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

    let rnd_peer = PeerId::random();
    ab_endpoint
        .sender
        .send(Inbox::State((
            rnd_peer,
            NodeState {
                offsets: HashMap::<KeyRange, KeyOffset>::from([(KeyRange(0), KeyOffset(8))]),
            },
        )))
        .expect("dont drop");

    while let Some(outbox) = ab_endpoint.receiver.recv().await {
        let Outbox::RequestMissingTxs((peer, requested_ranges)) = outbox else {
            continue;
        };
        assert_eq!(rnd_peer, peer);
        assert_eq!(
            requested_ranges,
            vec![
                (TransactionID(1), TransactionID(3)),
                (TransactionID(5), TransactionID(8))
            ]
        );

        break;
    }
}
