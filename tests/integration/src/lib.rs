#![allow(unused_imports)]

use std::{collections::HashMap, num::NonZeroUsize, thread::sleep, time::Duration};

use common::{
    async_interface::ReqResPair,
    gm_request_response::{Request, Transaction, TxStatus},
    meta_exchange::Response,
    range_key::{KeyOffset, KeyRange, TransactionID},
};
use gateway::core::Gateway;
use maroon::{
    app::{App, CurrentOffsets, Params, Request as AppRequest, Response as AppResponse},
    stack,
};
use tokio::sync::oneshot;

#[tokio::test(flavor = "multi_thread")]
async fn test_some() {
    env_logger::init();

    let (_shutdown_tx_0, shutdown_rx_0) = oneshot::channel();
    let (_shutdown_tx_1, shutdown_rx_1) = oneshot::channel();
    let (_shutdown_tx_2, shutdown_rx_2) = oneshot::channel();

    let params = Params {
        advertise_period: Duration::from_millis(500),
        consensus_nodes: NonZeroUsize::new(2).unwrap(),
    };

    let mut app0 = stack::create_stack(
        vec![
            "/ip4/127.0.0.1/tcp/3001".to_string(),
            "/ip4/127.0.0.1/tcp/3002".to_string(),
        ],
        "/ip4/0.0.0.0/tcp/3000".to_string(),
        params.clone(),
    )
    .unwrap();
    let mut app1 = stack::create_stack(
        vec![
            "/dns4/localhost/tcp/3000".to_string(),
            "/dns4/localhost/tcp/3002".to_string(),
        ],
        "/ip4/0.0.0.0/tcp/3001".to_string(),
        params.clone(),
    )
    .unwrap();
    let mut app2 = stack::create_stack(
        vec![
            "/ip4/127.0.0.1/tcp/3000".to_string(),
            "/ip4/127.0.0.1/tcp/3001".to_string(),
        ],
        "/ip4/0.0.0.0/tcp/3002".to_string(),
        params,
    )
    .unwrap();

    let mut state_check_0 = app0.get_state_interface();
    let mut state_check_1 = app1.get_state_interface();
    let mut state_check_2 = app2.get_state_interface();

    tokio::spawn(async move { app0.loop_until_shutdown(shutdown_rx_0).await });
    tokio::spawn(async move { app1.loop_until_shutdown(shutdown_rx_1).await });
    tokio::spawn(async move { app2.loop_until_shutdown(shutdown_rx_2).await });

    let mut gw = Gateway::new(vec![
        "/ip4/127.0.0.1/tcp/3000".to_string(),
        "/ip4/127.0.0.1/tcp/3001".to_string(),
        "/ip4/127.0.0.1/tcp/3002".to_string(),
    ])
    .unwrap();

    gw.start_on_background().await;
    tokio::time::sleep(Duration::from_secs(1)).await;

    _ = gw
        .send_request(Request::NewTransaction(Transaction {
            id: TransactionID(1),
            status: TxStatus::Created,
        }))
        .await;
    _ = gw
        .send_request(Request::NewTransaction(Transaction {
            id: TransactionID(0),
            status: TxStatus::Created,
        }))
        .await;

    let (mut app0_correct, mut app1_correct, mut app2_correct) = (false, false, false);

    let mut attempts_left = 3;
    let get_state_and_compare = async |interface: &mut ReqResPair<AppRequest, AppResponse>,
                                       offsets: &CurrentOffsets|
           -> bool {
        interface.sender.send(AppRequest::GetState).unwrap();
        let app_state_response = interface.receiver.recv().await.unwrap();
        println!("got app: {app_state_response:?}");

        let AppResponse::State(app_state) = app_state_response;
        app_state == *offsets
    };
    let desired_state = CurrentOffsets {
        self_offsets: HashMap::from([(KeyRange(0), KeyOffset(1))]),
        consensus_offset: HashMap::from([(KeyRange(0), KeyOffset(1))]),
    };

    while attempts_left > 0 || !(app0_correct && app1_correct && app2_correct) {
        app0_correct = get_state_and_compare(&mut state_check_0, &desired_state).await;
        app1_correct = get_state_and_compare(&mut state_check_1, &desired_state).await;
        app2_correct = get_state_and_compare(&mut state_check_2, &desired_state).await;

        tokio::time::sleep(Duration::from_secs(1)).await;
        attempts_left -= 1;
    }

    assert!(app0_correct);
    assert!(app1_correct);
    assert!(app2_correct);
}
