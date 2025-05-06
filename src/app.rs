use crate::interface::{Inbox, KeyOffset, KeyRange, NodeState, Outbox, P2PChannels};
use libp2p::PeerId;
use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

pub struct App {
    peer_id: PeerId,
    channels: P2PChannels,
}

impl App {
    pub fn new(peer_id: PeerId, channels: P2PChannels) -> Result<App, Box<dyn std::error::Error>> {
        Ok(App {
            peer_id,
            channels,
        })
    }

    /// blocks the thread
    pub async fn start_work(&mut self) {
        let mut ticker = tokio::time::interval(Duration::from_secs(5));
        let vector_state = HashMap::<KeyRange, KeyOffset>::new();

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    if let Err(e)=self.channels.sender.send( Outbox::State(NodeState{offsets: vector_state.clone()})) {
                        println!("main send: {e}");
                        continue;
                    };
                },
                Some(payload) = self.channels.receiver.recv() =>  {
                    match payload {
                        Inbox::State((peer_id, state))=>{
                            println!("got update for {peer_id}: {state:?}");
                        },
                        Inbox::Nodes(nodes)=>{
                            recalculate_order(self.peer_id ,&nodes);
                        },
                    }

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