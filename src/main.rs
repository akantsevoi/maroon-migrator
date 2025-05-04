#[macro_use]
mod macros;

mod interface;
mod p2p;

use interface::{Inbox, NodeState, Outbox};
use libp2p::PeerId;
use std::{collections::HashSet, time::Duration};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let node_urls: Vec<String> = std::env::var("NODE_URLS")
        .map_err(|e| format!("NODE_URLS not set: {}", e))?
        .split(',')
        .map(|s| s.to_string())
        .collect();

    let self_url: String =
        std::env::var("SELF_URL").map_err(|e| format!("SELF_URL not set: {}", e))?;

    let p2p = p2p::P2P::new(node_urls, self_url)?;
    let my_id = p2p.peer_id;
    let mut p2_pchannels = p2p.start()?;

    let mut ticker = tokio::time::interval(Duration::from_secs(5));

    let mut counter: i32 = 0;
    loop {
        tokio::select! {
            _ = ticker.tick() => {
                if let Err(e)=p2_pchannels.sender.send(Outbox::State(NodeState{value:counter})) {
                    println!("main send: {e}");
                    continue;
                };
                counter+=1;
            },
            Some(payload) = p2_pchannels.receiver.recv() =>  {
                match payload {
                    Inbox::State((peer_id, state))=>{
                        println!("got update for {peer_id}: {state:?}");
                    },
                    Inbox::Nodes(nodes)=>{
                        recalculate_order(my_id ,&nodes);
                    },
                }

            }
        }
    }
}

fn recalculate_order(self_id: PeerId, ids: &HashSet<PeerId>) {
    let mut peer_ids: Vec<&PeerId> = ids.iter().collect();

    peer_ids.sort();
    let delay_factor = peer_ids
        .iter()
        .position(|e| **e == self_id)
        .expect("self peer id should exist here. Otherwise it's stupid");

    println!(
        "My delay factor is {}! Nodes order: {:?}",
        delay_factor, peer_ids
    );
}
