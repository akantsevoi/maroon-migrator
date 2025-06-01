use common::{
  gm_request_response::Request,
  range_key::{KeyOffset, KeyRange, unique_blob_id_from_range_and_offset},
  transaction::{Transaction, TxStatus},
};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  env_logger::init();

  let node_urls: Vec<String> = std::env::var("NODE_URLS")
    .unwrap_or(
      "/ip4/127.0.0.1/tcp/3000,/ip4/127.0.0.1/tcp/3001,/ip4/127.0.0.1/tcp/3002".to_string(),
    )
    .split(',')
    .map(String::from)
    .collect();

  let key_range = KeyRange(
    std::env::var("KEY_RANGE")
      .unwrap_or("0".to_string())
      .parse::<u64>()
      .unwrap(),
  );

  let mut gw = gateway::core::Gateway::new(node_urls)?;
  gw.start_in_background().await;

  // wait until connected
  tokio::time::sleep(Duration::from_secs(2)).await;

  for i in 0..100 {
    _ = gw
      .send_request(Request::NewTransaction(Transaction {
        id: unique_blob_id_from_range_and_offset(key_range, KeyOffset(i)),
        status: TxStatus::Created,
      }))
      .await?;
    tokio::time::sleep(Duration::from_secs(1)).await;
  }

  tokio::time::sleep(Duration::from_secs(10)).await;
  Ok(())
}
