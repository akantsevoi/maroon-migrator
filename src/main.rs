use env_logger::Builder;
use etcd_client::{
    Client, Compare, CompareOp, Error, GetOptions, LeaseKeepAliveStream, LeaseKeeper, PutOptions,
    Txn, TxnOp, TxnOpResponse, WatchOptions,
};
use log::{LevelFilter, info};
use rand::Rng;
use std::io::Write;
use std::vec;

#[tokio::main]
async fn main() -> Result<(), Error> {
    let node_label = std::env::var("NODE_LABEL").ok().unwrap_or_else(|| {
        let mut rng = rand::thread_rng();
        format!("maroon_{}", rng.gen_range(0..i32::MAX))
    });
    let etcd_endpoints = std::env::var("ETCD_ENDPOINTS").expect("ETCD_ENDPOINTS not set");

    let node_label_clone = node_label.clone();
    Builder::new()
        .format(move |buf, record| {
            writeln!(
                buf,
                "{}:{} [{}][{}] - {}",
                record.file().unwrap_or("unknown"),
                record.line().unwrap_or(0),
                record.level(),
                node_label_clone,
                record.args()
            )
        })
        .filter(None, LevelFilter::Info)
        .init();

    info!("Maroon Node started");

    let etcd_endpoints: Vec<String> = etcd_endpoints.split(",").map(|s| s.to_string()).collect();

    info!("endpoints: {:#?}", etcd_endpoints);
    let mut client = Client::connect(&etcd_endpoints, None).await?;
    info!("etcd client created");

    let _ = start_getting_order_updates(&etcd_endpoints, &node_label).await?;
    info!("order updates started");
    let _ = nodes_order_cycle(&mut client, &node_label).await?;

    return Ok(());
}

// order change cycle

const ORDER_PREFIX: &str = "/maroonOrder/";
fn get_the_latest_index(resp: Option<&TxnOpResponse>) -> i64 {
    if let Some(response) = resp {
        match response {
            etcd_client::TxnOpResponse::Get(get_resp) => {
                let mut max = 0;
                for kv in get_resp.kvs() {
                    let key = String::from_utf8_lossy(kv.key());

                    let num_str = key.strip_prefix(ORDER_PREFIX).unwrap_or("0").to_string();

                    let num = num_str.parse::<i64>().unwrap_or(0);
                    if num > max {
                        max = num;
                    }
                }

                return max;
            }
            _ => return 0,
        }
    } else {
        return 0;
    }
}

// watches updates for ORDER_PREFIX
// gets all the nodes, sorts, finds itself - this is that node's order
// if can't find itself - this node is out of order
async fn start_getting_order_updates(
    etcd_endpoints: &Vec<String>,
    node_label: &String,
) -> Result<(), Error> {
    let mut client = Client::connect(etcd_endpoints, None).await?;
    let node_label: String = node_label.clone();

    let (watcher, mut watch_stream) = client
        .watch(ORDER_PREFIX, Some(WatchOptions::new().with_prefix()))
        .await?;
    let _watch_task = tokio::spawn(async move {
        // Keep watcher alive within the task scope
        let _watcher = watcher;
        loop {
            if let Ok(Some(_)) = watch_stream.message().await {
                let res = client
                    .txn(Txn::new().and_then(vec![TxnOp::get(
                        ORDER_PREFIX,
                        Some(GetOptions::new().with_prefix()),
                    )]))
                    .await;
                if let Ok(res) = res {
                    if let Some(get_resp) = res.op_responses().get(0) {
                        match get_resp {
                            TxnOpResponse::Get(get_resp) => {
                                let mut order_to_node_id: Vec<(i64, String)> = Vec::new();
                                for kv in get_resp.kvs() {
                                    let key = String::from_utf8_lossy(kv.key())
                                        .strip_prefix(ORDER_PREFIX)
                                        .unwrap_or("0")
                                        .parse::<i64>()
                                        .unwrap_or(0);
                                    let value = String::from_utf8_lossy(kv.value()).to_string();

                                    order_to_node_id.push((key, value));
                                }

                                order_to_node_id.sort_by_key(|kv| kv.0);

                                info!("current nodes: {:?}", &order_to_node_id);

                                if let Some(pos) =
                                    order_to_node_id.iter().position(|kv| kv.1 == node_label)
                                {
                                    info!("my current order offset is {}", pos);
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    });

    return Ok(());
}

const LEASE_TTL: i64 = 6;
const LEASE_REFRESH_PERIOD: u64 = 3;

async fn nodes_order_cycle(client: &mut Client, node_label: &String) -> Result<(), Error> {
    let node_order_number: i64;

    let mut keeper: LeaseKeeper;
    let mut keeper_stream: LeaseKeepAliveStream;

    loop {
        let res = client
            .txn(Txn::new().and_then(vec![TxnOp::get(
                ORDER_PREFIX,
                Some(GetOptions::new().with_prefix()),
            )]))
            .await?;

        let index = get_the_latest_index(res.op_responses().get(0));
        let try_index = index + 1;
        info!("current latest: {}. Next to try: {}", index, try_index);

        // Lease for current node's order

        let lease = client.lease_grant(LEASE_TTL, None).await?;
        let lease_id = lease.id();

        (keeper, keeper_stream) = client.lease_keep_alive(lease_id).await?;

        let txn_resp = client
            .txn(
                Txn::new()
                    .when(vec![Compare::version(
                        format!("{}{}", ORDER_PREFIX, try_index).as_str(),
                        CompareOp::Equal,
                        0,
                    )])
                    .and_then(vec![TxnOp::put(
                        format!("{}{}", ORDER_PREFIX, try_index).as_str(),
                        node_label.clone(),
                        Some(PutOptions::new().with_lease(lease_id)),
                    )]),
            )
            .await?;

        if !txn_resp.succeeded() {
            continue;
        }

        node_order_number = try_index;
        info!("finished. Got number: {}", node_order_number);

        loop {
            info!(
                "Maroon Node heartbeat. NodeID: {} Number: {}",
                node_label, node_order_number
            );

            match keeper.keep_alive().await {
                Ok(_) => info!("Refreshed lease"),
                Err(e) => info!("Failed to refresh lease: {:?}", e),
            }

            if let Ok(Ok(Some(_resp))) = tokio::time::timeout(
                tokio::time::Duration::from_millis(100),
                keeper_stream.message(),
            )
            .await
            {
                // info!("Lease keep-alive response: {:?}", _resp);
            }

            tokio::time::sleep(tokio::time::Duration::from_secs(LEASE_REFRESH_PERIOD)).await;
        }
    }
}

/*

loop {
    get_next_index
    create_lease_on_next_index
    if unsucess { continue }

    loop {
        renew lease


    }

}

loop(alias watch) {
    get_node_records
    find_until_has_its_own
    if found {
        my order is this
    } else {
        my order is unknown
    }

    sleep 1 sec
}


*/
