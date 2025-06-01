#![allow(unused_imports)]

use std::{collections::HashMap, num::NonZeroUsize, thread::sleep, time::Duration};

use common::{
  duplex_channel::Endpoint,
  gm_request_response::Request,
  invoker_handler::InvokerInterface,
  meta_exchange::Response,
  range_key::{KeyOffset, KeyRange, UniqueU64BlobId},
  transaction::{Transaction, TxStatus},
};
use gateway::core::Gateway;
use maroon::{
  app::{App, CurrentOffsets, Params, Request as AppRequest, Response as AppResponse},
  stack,
};
use tokio::sync::oneshot;

#[tokio::test(flavor = "multi_thread")]
async fn basic() {
  _ = env_logger::Builder::new().try_init();

  let (_shutdown_tx_0, shutdown_rx_0) = oneshot::channel();
  let (_shutdown_tx_1, shutdown_rx_1) = oneshot::channel();
  let (_shutdown_tx_2, shutdown_rx_2) = oneshot::channel();

  let params = Params::default().set_advertise_period(Duration::from_millis(500));

  // create nodes and gateway

  let (mut node0, state_invoker_0) = stack::create_stack(
    vec![
      "/ip4/127.0.0.1/tcp/3001".to_string(),
      "/ip4/127.0.0.1/tcp/3002".to_string(),
    ],
    "/ip4/0.0.0.0/tcp/3000".to_string(),
    params.clone(),
  )
  .unwrap();
  let (mut node1, state_invoker_1) = stack::create_stack(
    vec![
      "/dns4/localhost/tcp/3000".to_string(),
      "/dns4/localhost/tcp/3002".to_string(),
    ],
    "/ip4/0.0.0.0/tcp/3001".to_string(),
    params.clone(),
  )
  .unwrap();
  let (mut node2, state_invoker_2) = stack::create_stack(
    vec![
      "/ip4/127.0.0.1/tcp/3000".to_string(),
      "/ip4/127.0.0.1/tcp/3001".to_string(),
    ],
    "/ip4/0.0.0.0/tcp/3002".to_string(),
    params,
  )
  .unwrap();

  let mut gw = Gateway::new(vec![
    "/ip4/127.0.0.1/tcp/3000".to_string(),
    "/ip4/127.0.0.1/tcp/3001".to_string(),
    "/ip4/127.0.0.1/tcp/3002".to_string(),
  ])
  .unwrap();

  // run nodes and gateway

  tokio::spawn(async move { node0.loop_until_shutdown(shutdown_rx_0).await });
  tokio::spawn(async move { node1.loop_until_shutdown(shutdown_rx_1).await });
  tokio::spawn(async move { node2.loop_until_shutdown(shutdown_rx_2).await });

  gw.start_in_background().await;

  // wait until they are connected
  tokio::time::sleep(Duration::from_secs(1)).await;

  // send requests from gateway
  _ = gw
    .send_request(Request::NewTransaction(Transaction {
      id: UniqueU64BlobId(1),
      status: TxStatus::Created,
    }))
    .await;
  _ = gw
    .send_request(Request::NewTransaction(Transaction {
      id: UniqueU64BlobId(0),
      status: TxStatus::Created,
    }))
    .await;

  // check results
  let (mut node0_correct, mut node1_correct, mut node2_correct) = (false, false, false);

  let get_state_and_compare = async |interface: &InvokerInterface<AppRequest, AppResponse>,
                                     offsets: &CurrentOffsets|
         -> bool {
    let app_state_response = interface.request(AppRequest::GetState).await;
    println!("got app: {app_state_response:?}");

    let AppResponse::State(app_state) = app_state_response;
    app_state == *offsets
  };
  let desired_state = CurrentOffsets {
    self_offsets: HashMap::from([(KeyRange(0), KeyOffset(1))]),
    consensus_offset: HashMap::from([(KeyRange(0), KeyOffset(1))]),
  };

  for _ in 0..3 {
    node0_correct = get_state_and_compare(&state_invoker_0, &desired_state).await;
    node1_correct = get_state_and_compare(&state_invoker_1, &desired_state).await;
    node2_correct = get_state_and_compare(&state_invoker_2, &desired_state).await;
    if node0_correct && node1_correct && node2_correct {
      break;
    }
    println!("TICK");
    tokio::time::sleep(Duration::from_secs(1)).await;
  }

  assert!(node0_correct);
  assert!(node1_correct);
  assert!(node2_correct);
}
