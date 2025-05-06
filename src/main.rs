#[macro_use]
mod macros;

mod interface;
mod p2p;

use interface::{Inbox, KeyOffset, KeyRange, NodeState, Outbox};
use libp2p::PeerId;
use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let node_urls: Vec<String> = std::env::var("NODE_URLS")
        .map_err(|e| format!("NODE_URLS not set: {}", e))?
        .split(',')
        .map(String::from)
        .collect();

    let self_url: String =
        std::env::var("SELF_URL").map_err(|e| format!("SELF_URL not set: {}", e))?;

    let _tick = Duration::from_millis(
        std::env::var("TICK")
            .unwrap_or("60".to_string())
            .parse::<u64>()
            .unwrap(),
    );

    let mut p2p = p2p::P2P::new(node_urls, self_url)?;
    let my_id = p2p.peer_id;

    let mut p2p_channels = p2p.interface_channels();
    _ = p2p.prepare()?;

    tokio::spawn(async move {
        p2p.start_event_loop().await;
    });

    let mut ticker = tokio::time::interval(Duration::from_secs(5));
    let vector_state = HashMap::<KeyRange, KeyOffset>::new();

    loop {
        tokio::select! {
            // TODO: check what happens if I have two Futures that are ready
            // what happens? Will it cancel one of it?
            _ = ticker.tick() => {
                if let Err(e)=p2p_channels.sender.send( Outbox::State(NodeState{offsets: vector_state.clone()})) {
                    println!("main send: {e}");
                    continue;
                };
            },
            Some(payload) = p2p_channels.receiver.recv() =>  {
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
