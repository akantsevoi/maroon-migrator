use std::collections::HashSet;

use futures::StreamExt;
use libp2p::{
    Multiaddr, PeerId,
    core::{transport::Transport as _, upgrade},
    identity,
    noise::{Config as NoiseConfig, Error as NoiseError},
    ping::{Behaviour as PingBehaviour, Config as PingConfig, Event as PingEvent},
    swarm::{Config as SwarmConfig, NetworkBehaviour, Swarm, SwarmEvent},
    tcp::{Config as TcpConfig, tokio::Transport as TokioTcpTransport},
    yamux::Config as YamuxConfig,
};

#[derive(NetworkBehaviour)]
#[behaviour(out_event = "MaroonBehaviourEvent")]
struct MaroonBehaviour {
    ping: PingBehaviour,
}

pub enum MaroonBehaviourEvent {
    Ping(PingEvent),
}

impl From<PingEvent> for MaroonBehaviourEvent {
    fn from(e: PingEvent) -> Self {
        MaroonBehaviourEvent::Ping(e)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse NODE_URLS environment variable
    let node_urls: Vec<String> = std::env::var("NODE_URLS")
        .map_err(|e| format!("NODE_URLS not set: {}", e))?
        .split(',')
        .map(|s| s.to_string())
        .collect();

    let self_url: String =
        std::env::var("SELF_URL").map_err(|e| format!("SELF_URL not set: {}", e))?;

    // Generate identity keypair and peer ID
    let local_key = identity::Keypair::generate_ed25519();
    let local_peer_id = PeerId::from(local_key.public());
    println!("Local peer id: {:?}", local_peer_id);

    let auth_config = NoiseConfig::new(&local_key)
        .map_err(|e: NoiseError| format!("noise config error: {}", e))?;

    // Build the TCP transport with Tokio, Noise, and Yamux
    let transport = TokioTcpTransport::new(TcpConfig::default().nodelay(true))
        .upgrade(upgrade::Version::V1)
        .authenticate(auth_config)
        .multiplex(YamuxConfig::default())
        .boxed();

    let behaviour = MaroonBehaviour {
        ping: PingBehaviour::new(
            PingConfig::new()
                .with_interval(std::time::Duration::from_secs(5))
                .with_timeout(std::time::Duration::from_secs(10)),
        ),
    };

    let mut swarm = Swarm::new(
        transport,
        behaviour,
        local_peer_id,
        SwarmConfig::with_tokio_executor()
            .with_idle_connection_timeout(std::time::Duration::from_secs(60)),
    );

    swarm.listen_on(self_url.parse()?)?;

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

    loop {
        match swarm.select_next_some().await {
            SwarmEvent::NewListenAddr { address, .. } => {
                println!("Listening on {address}");
            }
            SwarmEvent::Behaviour(MaroonBehaviourEvent::Ping(PingEvent {
                peer,
                connection,
                result,
            })) => {
                println!("Ping with {peer}: {result:?}: {connection:?}");
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
            }
            SwarmEvent::ConnectionEstablished {
                peer_id,
                connection_id,
                endpoint,
                num_established,
                concurrent_dial_errors,
                established_in,
            } => {
                println!(
                    "Connection established {peer_id}: {endpoint:?}: {connection_id:?}: {num_established}"
                );
                alive_peer_ids.insert(peer_id);
            }
            _ => {}
        }

        recalculate_order(&alive_peer_ids);
    }
}

fn recalculate_order(ids: &HashSet<PeerId>) {
    let mut peer_ids: Vec<&PeerId> = ids.iter().collect();

    peer_ids.sort();

    println!("nodes order: {:?}", peer_ids);
}
