use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

use futures::StreamExt;
use libp2p::{
    Multiaddr, PeerId,
    core::{transport::Transport as _, upgrade},
    gossipsub::{
        Behaviour as GossipsubBehaviour, ConfigBuilder as GossipsubConfigBuilder,
        Event as GossipsubEvent, MessageAuthenticity, Sha256Topic, ValidationMode,
    },
    identity,
    noise::{Config as NoiseConfig, Error as NoiseError},
    ping::{Behaviour as PingBehaviour, Config as PingConfig, Event as PingEvent},
    swarm::{Config as SwarmConfig, NetworkBehaviour, Swarm, SwarmEvent},
    tcp::{Config as TcpConfig, tokio::Transport as TcpTokioTransport},
    yamux::Config as YamuxConfig,
};
use p2p::NodeState;
use serde::{Deserialize, Serialize};

mod p2p;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let node_urls: Vec<String> = std::env::var("NODE_URLS")
        .map_err(|e| format!("NODE_URLS not set: {}", e))?
        .split(',')
        .map(|s| s.to_string())
        .collect();

    let self_url: String =
        std::env::var("SELF_URL").map_err(|e| format!("SELF_URL not set: {}", e))?;

    let op2p = p2p::P2P::new(node_urls, self_url)?;
    let mut p2_pchannels = op2p.start()?;

    let mut ticker = tokio::time::interval(Duration::from_secs(5));

    let mut counter: i32 = 0;
    loop {
        tokio::select! {
            _ = ticker.tick() => {
                if let Err(e)=p2_pchannels.sender.send(p2p::Payload::State(NodeState{value: counter})) {
                    println!("main send: {e}");
                    continue;
                };
                counter+=1;
            },
            Some(payload) = p2_pchannels.receiver.recv() =>  {
                println!("got main payload: {payload:?}");
            }
        }
    }

    Ok(())
}

fn recalculate_order(ids: &HashSet<PeerId>) {
    let mut peer_ids: Vec<&PeerId> = ids.iter().collect();

    peer_ids.sort();

    println!("nodes order: {:?}", peer_ids);
}
