use common::{
    gm_request_response::{Request, Transaction, TxStatus},
    range_key::TransactionID,
};
use log::{debug, error, info, warn};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    //     let node_urls = std::env::var("NODE_URLS");
    let node_urls = Result::<String, std::env::VarError>::Ok(String::from(
        "/ip4/127.0.0.1/tcp/3000,/ip4/127.0.0.1/tcp/3001,/ip4/127.0.0.1/tcp/3002",
    ));
    // let node_urls =
    //     Result::<String, std::env::VarError>::Ok(String::from("/dns4/localhost/tcp/3000"));

    let node_urls: Vec<String> = node_urls
        .map_err(|e| format!("NODE_URLS not set: {}", e))?
        .split(',')
        .map(String::from)
        .collect();

    let mut gw = gateway::core::Gateway::new(node_urls)?;
    gw.start_on_background().await;

    // wait until connected
    tokio::time::sleep(Duration::from_secs(2)).await;

    for i in 0..10 {
        _ = gw
            .send_request(Request::NewTransaction(Transaction {
                id: TransactionID(i),
                status: TxStatus::Created,
            }))
            .await?;
        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    tokio::time::sleep(Duration::from_secs(10)).await;
    Ok(())
}
