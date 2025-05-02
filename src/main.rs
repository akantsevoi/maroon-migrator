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
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
struct NodeState {
    id: PeerId,
}

#[derive(NetworkBehaviour)]
#[behaviour(out_event = "MaroonEvent")]
struct MaroonBehaviour {
    ping: PingBehaviour,
    gossipsub: GossipsubBehaviour,
}

pub enum MaroonEvent {
    Ping(PingEvent),
    Gossipsub(GossipsubEvent),
}

impl From<PingEvent> for MaroonEvent {
    fn from(e: PingEvent) -> Self {
        MaroonEvent::Ping(e)
    }
}

impl From<GossipsubEvent> for MaroonEvent {
    fn from(e: GossipsubEvent) -> Self {
        MaroonEvent::Gossipsub(e)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let node_urls: Vec<String> = std::env::var("NODE_URLS")
        .map_err(|e| format!("NODE_URLS not set: {}", e))?
        .split(',')
        .map(|s| s.to_string())
        .collect();

    let self_url: String =
        std::env::var("SELF_URL").map_err(|e| format!("SELF_URL not set: {}", e))?;

    let local_key = identity::Keypair::generate_ed25519();
    let local_peer_id = PeerId::from(local_key.public());
    println!("Local peer id: {:?}", local_peer_id);

    let auth_config = NoiseConfig::new(&local_key)
        .map_err(|e: NoiseError| format!("noise config error: {}", e))?;

    let transport = TcpTokioTransport::new(TcpConfig::default().nodelay(true))
        .upgrade(upgrade::Version::V1)
        .authenticate(auth_config)
        .multiplex(YamuxConfig::default())
        .boxed();

    let mut gossipsub = GossipsubBehaviour::new(
        MessageAuthenticity::Signed(local_key.clone()),
        GossipsubConfigBuilder::default()
            .mesh_outbound_min(1)
            .mesh_n_low(1)
            .mesh_n(2)
            .validation_mode(ValidationMode::Permissive)
            .build()
            .map_err(|e| format!("gossipsub config builder: {e}"))?,
    )
    .map_err(|e| format!("gossipsub behaviour creation: {e}"))?;

    let state_topic = Sha256Topic::new("node-state");
    gossipsub.subscribe(&state_topic)?;

    let behaviour = MaroonBehaviour {
        ping: PingBehaviour::new(
            PingConfig::new()
                .with_interval(Duration::from_secs(5))
                .with_timeout(Duration::from_secs(10)),
        ),
        gossipsub,
    };

    let mut swarm = Swarm::new(
        transport,
        behaviour,
        local_peer_id,
        SwarmConfig::with_tokio_executor().with_idle_connection_timeout(Duration::from_secs(60)),
    );

    swarm
        .listen_on(self_url.parse()?)
        .map_err(|e| format!("swarm.listen {e}"))?;

    for url in node_urls {
        if url == self_url {
            continue;
        }

        let addr: Multiaddr = url.parse()?;
        println!("Dialing {addr} …");
        swarm.dial(addr)?;
    }

    let mut alive_peer_ids: HashSet<PeerId> = HashSet::new();
    alive_peer_ids.insert(local_peer_id);

    let mut ticker = tokio::time::interval(Duration::from_secs(5));

    let topic_hash = state_topic.hash();

    loop {
        tokio::select! {
            _= ticker.tick() => {
                let state = NodeState {
                    id: swarm.local_peer_id().clone(),
                };
                let bytes = serde_json::to_vec(&state)?;
                let result = swarm.behaviour_mut().gossipsub.publish(topic_hash.clone(), bytes);

                match result {
                    Ok(message_id)=>{println!("Published state: {}  {:?}",message_id, state);}
                    Err(e) => {
                        println!("Publish state error: {}",e);
                    }
                }


            }
            event = swarm.select_next_some() => match event {
                SwarmEvent::NewListenAddr { address, .. } => {
                    println!("Listening on {address}");
                }
                SwarmEvent::Behaviour(MaroonEvent::Ping(PingEvent {
                    peer,
                    connection,
                    result,
                })) => {
                    println!("Ping with {peer}: {result:?}: {connection:?}");
                }
                SwarmEvent::Behaviour(MaroonEvent::Gossipsub(GossipsubEvent::Message { propagation_source, message_id, message })) =>{
                    println!("Got message {propagation_source}: {message_id:?}: {message:?}");
                }
                SwarmEvent::ConnectionClosed {
                    peer_id,
                    connection_id,
                    endpoint,
                    num_established,
                    cause,
                } => {
                    println!(
                        "Connection closed {peer_id}: {endpoint:?}: {connection_id:?}: {num_established}: {cause:?}"
                    );
                    alive_peer_ids.remove(&peer_id);
                    recalculate_order(&alive_peer_ids);

                }
                SwarmEvent::ConnectionEstablished {
                    peer_id,
                    connection_id,
                    endpoint,
                    num_established,
                    concurrent_dial_errors,
                    established_in,
                } => {
                    _ = concurrent_dial_errors;
                    _ = established_in;
                    println!(
                        "Connection established {peer_id}: {endpoint:?}: {connection_id:?}: {num_established}"
                    );
                    alive_peer_ids.insert(peer_id);
                    recalculate_order(&alive_peer_ids);

                }

                _ => {}
            }
        };
    }
}

fn recalculate_order(ids: &HashSet<PeerId>) {
    let mut peer_ids: Vec<&PeerId> = ids.iter().collect();

    peer_ids.sort();

    println!("nodes order: {:?}", peer_ids);
}
