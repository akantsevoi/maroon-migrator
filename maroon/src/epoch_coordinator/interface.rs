use common::range_key::U64BlobIdClosedInterval;
use libp2p::PeerId;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::{self, UnboundedReceiver};

use super::epoch::Epoch;
use etcd_client::{
  Client, Compare, CompareOp, Error, GetOptions, LeaseKeepAliveStream, LeaseKeeper, PutOptions,
  Txn, TxnOp, TxnOpResponse, WatchOptions,
};
use log::{debug, info};
use std::{fmt, time::Duration};

const MAROON_PREFIX: &str = "/maroon";
const MAROON_LATEST: &str = "/maroon/latest";
const MAROON_HISTORY: &str = "/maroon/history";

#[tokio::test(flavor = "multi_thread")]
async fn test_etcd() {
  let node_urls = vec![
    "http://localhost:2379".to_string(),
    "http://localhost:2380".to_string(),
    "http://localhost:2381".to_string(),
  ];

  _ = start_getting_order_updates(&node_urls).await;

  let mut client = Client::connect(node_urls, None).await.unwrap();

  let peer_id = PeerId::random();
  let peer_id_2 = PeerId::random();
  let first_obj = EpochObject {
    peer_id: peer_id,
    epoch: Epoch::new(vec![U64BlobIdClosedInterval::new(0, 13)], None),
  };

  let second_obj = EpochObject {
    peer_id: peer_id_2,
    epoch: Epoch::new(vec![U64BlobIdClosedInterval::new(0, 11)], None),
  };

  let resp = client
    .txn(Txn::new().and_then(vec![
      TxnOp::put(MAROON_LATEST, serde_json::to_vec(&first_obj).unwrap(), None),
      TxnOp::put(
        format!("{}/{}", MAROON_HISTORY, 0),
        serde_json::to_vec(&first_obj).unwrap(),
        None,
      ),
    ]))
    .await
    .unwrap();

  println!("Response1: {:?}", resp);

  let resp = client
    .txn(
      Txn::new()
        .when(vec![Compare::version(
          format!("{}/{}", MAROON_HISTORY, 0),
          CompareOp::Equal,
          0,
        )])
        .and_then(vec![
          TxnOp::put(
            MAROON_LATEST,
            serde_json::to_vec(&second_obj).unwrap(),
            None,
          ),
          TxnOp::put(
            format!("{}/{}", MAROON_HISTORY, 0),
            serde_json::to_vec(&second_obj).unwrap(),
            None,
          ),
        ]),
    )
    .await
    .unwrap();

  println!("ResponseWITH_COMPARE: {:?}", resp);

  tokio::time::sleep(Duration::from_secs(3)).await;
  assert!(false);
}

#[derive(Serialize, Deserialize, Debug)]
struct EpochObject {
  epoch: Epoch,
  peer_id: PeerId,
}

async fn start_getting_order_updates(etcd_endpoints: &Vec<String>) -> Result<(), Error> {
  let mut client = Client::connect(etcd_endpoints, None).await?;

  let (watcher, mut watch_stream) = client
    .watch(MAROON_LATEST, Some(WatchOptions::new().with_prefix()))
    .await?;
  let _watch_task = tokio::spawn(async move {
    // Keep watcher alive within the task scope
    let _watcher = watcher;
    loop {
      if let Ok(Some(message)) = watch_stream.message().await {
        for event in message.events() {
          if let Some(kv) = event.kv() {
            if let Ok(epoch_obj) = serde_json::from_slice::<EpochObject>(kv.value()) {
              println!("EpochObject: {:?}", epoch_obj);
            }
          }
        }

        // let res = client
        //   .txn(Txn::new().and_then(vec![TxnOp::get(
        //     MAROON_PREFIX,
        //     Some(GetOptions::new().with_prefix()),
        //   )]))
        //   .await;
        // if let Ok(res) = res {
        //   if let Some(get_resp) = res.op_responses().get(0) {
        //     match get_resp {
        //       TxnOpResponse::Get(get_resp) => {
        //         let mut order_to_node_id: Vec<(i64, String)> = Vec::new();
        //         for kv in get_resp.kvs() {
        //           let key = String::from_utf8_lossy(kv.key())
        //             .strip_prefix(MAROON_PREFIX)
        //             .unwrap_or("0")
        //             .parse::<i64>()
        //             .unwrap_or(0);
        //           let value = String::from_utf8_lossy(kv.value()).to_string();

        //           order_to_node_id.push((key, value));
        //         }

        //         order_to_node_id.sort_by_key(|kv| kv.0);

        //         info!("current nodes: {:?}", &order_to_node_id);
        //       }
        //       _ => {}
        //     }
        //   }
        // }
      }
    }
  });

  return Ok(());
}

pub struct Coordinator {}

#[derive(Debug)]
pub enum CommitError {
  CommitFailed(String),
}

impl fmt::Display for CommitError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      CommitError::CommitFailed(msg) => write!(f, "Failed to commit epoch: {}", msg),
    }
  }
}

impl std::error::Error for CommitError {}

async fn commit_new_epoch(new: Epoch) -> Result<(), CommitError> {
  Ok(())
}

// use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

struct EpochUpdates {}

fn get_listen_interface() -> UnboundedReceiver<EpochUpdates> {
  let (_tx_request, rx_request) = mpsc::unbounded_channel::<EpochUpdates>();
  rx_request
}

/// etcd structure
///
/// /maroon
///     /history
///         /0
///         /1
///         ...
///         /100500
///             epoch_obj
///     /latest
///         epoch_obj
///
/// epoch_obj: {
///     hash
///     ranges
///     peer_id
/// }
fn slient() {}
