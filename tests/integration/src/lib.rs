#![allow(unused_imports)]

use core::time;
use std::thread::sleep;

use common::{
    gm_request_response::{Request, Transaction, TxStatus},
    range_key::TransactionID,
};
use gateway::core::Gateway;
use maroon::stack;
use tokio::sync::oneshot;

#[tokio::test(flavor = "multi_thread")]
async fn test_some() {
    env_logger::init();

    let (_shutdown_tx_0, shutdown_rx_0) = oneshot::channel();
    let (_shutdown_tx_1, shutdown_rx_1) = oneshot::channel();
    let (_shutdown_tx_2, shutdown_rx_2) = oneshot::channel();

    tokio::spawn(async move {
        let result = stack::create_stack_and_loop_until_shutdown(
            vec![
                "/ip4/127.0.0.1/tcp/3001".to_string(),
                "/ip4/127.0.0.1/tcp/3002".to_string(),
            ],
            "/ip4/0.0.0.0/tcp/3000".to_string(),
            shutdown_rx_0,
        )
        .await;

        assert!(result.is_ok(), "Failed to create stack: {:?}", result);
    });

    tokio::spawn(async move {
        let result = stack::create_stack_and_loop_until_shutdown(
            vec![
                "/ip4/127.0.0.1/tcp/3000".to_string(),
                "/ip4/127.0.0.1/tcp/3002".to_string(),
            ],
            "/ip4/0.0.0.0/tcp/3001".to_string(),
            shutdown_rx_1,
        )
        .await;

        assert!(result.is_ok(), "Failed to create stack: {:?}", result);
    });

    tokio::spawn(async move {
        let result = stack::create_stack_and_loop_until_shutdown(
            vec![
                "/dns4/localhost/tcp/3000".to_string(),
                "/dns4/localhost/tcp/3001".to_string(),
            ],
            "/ip4/0.0.0.0/tcp/3002".to_string(),
            shutdown_rx_2,
        )
        .await;

        assert!(result.is_ok(), "Failed to create stack: {:?}", result);
    });

    let mut gw = Gateway::new(vec![
        "/ip4/127.0.0.1/tcp/3000".to_string(),
        "/ip4/127.0.0.1/tcp/3001".to_string(),
        "/ip4/127.0.0.1/tcp/3002".to_string(),
    ])
    .unwrap();

    gw.start_on_background().await;
    sleep(time::Duration::from_secs(1));

    _ = gw
        .send_request(Request::NewTransaction(Transaction {
            id: TransactionID(0),
            status: TxStatus::Created,
        }))
        .await;

    sleep(time::Duration::from_secs(10));
}
