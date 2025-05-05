use std::{collections::HashSet, fmt::Debug, time::Duration};

use futures::StreamExt;
use interface::{Inbox, Outbox, P2PChannels};
use libp2p::{
    Multiaddr, PeerId,
    core::{transport::Transport as _, upgrade},
    gossipsub::{
        Behaviour as GossipsubBehaviour, ConfigBuilder as GossipsubConfigBuilder,
        Event as GossipsubEvent, MessageAuthenticity, Sha256Topic, TopicHash, ValidationMode,
    },
    identity,
    noise::{Config as NoiseConfig, Error as NoiseError},
    ping::{Behaviour as PingBehaviour, Config as PingConfig, Event as PingEvent},
    swarm::{Config as SwarmConfig, NetworkBehaviour, Swarm, SwarmEvent},
    tcp::{Config as TcpConfig, tokio::Transport as TcpTokioTransport},
    yamux::Config as YamuxConfig,
};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::interface;

#[derive(NetworkBehaviour)]
#[behaviour(out_event = "MaroonEvent")]
struct MaroonBehaviour {
    ping: PingBehaviour,
    gossipsub: GossipsubBehaviour,
}

pub enum MaroonEvent {
    // TODO: remove ping behaviour? I don't think it's needed as it doesn't send any data itself and I don't use it?
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

pub struct P2P {
    pub peer_id: PeerId,

    node_urls: Vec<String>,
    self_url: String,

    swarm: Swarm<MaroonBehaviour>,
    state_topic_hash: TopicHash,
}

impl P2P {
    pub fn new(
        node_urls: Vec<String>,
        self_url: String,
    ) -> Result<P2P, Box<dyn std::error::Error>> {
        let kp = identity::Keypair::generate_ed25519();
        let peer_id = PeerId::from(kp.public());
        println!("Local peer id: {:?}", peer_id);

        let auth_config =
            NoiseConfig::new(&kp).map_err(|e: NoiseError| format!("noise config error: {}", e))?;

        let transport = TcpTokioTransport::new(TcpConfig::default().nodelay(true))
            .upgrade(upgrade::Version::V1)
            .authenticate(auth_config)
            .multiplex(YamuxConfig::default())
            .boxed();

        let mut gossipsub = GossipsubBehaviour::new(
            MessageAuthenticity::Signed(kp.clone()),
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

        let swarm = Swarm::new(
            transport,
            behaviour,
            peer_id,
            SwarmConfig::with_tokio_executor()
                .with_idle_connection_timeout(Duration::from_secs(60)),
        );

        Ok(P2P {
            node_urls,
            self_url,
            peer_id,
            swarm,
            state_topic_hash: state_topic.hash().clone(),
        })
    }

    // spawns a separate tokio thread
    // TODO:
    // consumes self, so it can't be used later. Is it a good design? Looks suspicious
    // - but I need swarm in a separate thread. So I would need to use mutex or smth like this
    // - Do I?
    pub fn start(self) -> Result<P2PChannels, Box<dyn std::error::Error>> {
        let mut swarm = self.swarm;

        swarm
            .listen_on(self.self_url.parse()?)
            .map_err(|e| format!("swarm.listen {e}"))?;

        for url in self.node_urls.clone() {
            if url == self.self_url {
                continue;
            }

            let addr: Multiaddr = url.parse()?;
            println!("Dialing {addr} …");
            swarm.dial(addr)?;
        }

        let (tx, rx) = mpsc::unbounded_channel::<Inbox>();
        let (tx_in, rx_in) = mpsc::unbounded_channel::<Outbox>();

        start_event_loop(swarm, self.peer_id, self.state_topic_hash, rx_in, tx);

        return Ok(P2PChannels {
            receiver: rx,
            sender: tx_in,
        });
    }
}

fn start_event_loop(
    mut swarm: Swarm<MaroonBehaviour>,
    peer_id: PeerId,
    topic_hash: TopicHash,
    rx_in: mpsc::UnboundedReceiver<Outbox>,
    tx: mpsc::UnboundedSender<Inbox>,
) {
    let mut alive_peer_ids: HashSet<PeerId> = HashSet::new();
    alive_peer_ids.insert(peer_id);

    let mut rx_in = rx_in;
    tokio::spawn(async move {
        loop {
            tokio::select! {
                Some(Outbox::State(state)) = rx_in.recv() =>  {
                    let message = P2pMessage {
                        peer_id: peer_id,
                        payload: Outbox::State(state),
                    };

                    let bytes = guard_ok!(serde_json::to_vec(&message), e, {
                        println!("serialize message error: {e}");
                        continue;
                    });

                    if let Err(e) = swarm
                        .behaviour_mut()
                        .gossipsub
                        .publish(topic_hash.clone(), bytes){
                            println!("publish error: {}", e);
                        }
                },
                event = swarm.select_next_some()=>{
                    match event{
                    SwarmEvent::Behaviour(MaroonEvent::Gossipsub(GossipsubEvent::Message {
                        propagation_source: _,
                        message_id: _,
                        message,
                    })) => match serde_json::from_slice::<P2pMessage>(&message.data) {
                        Ok(p2p_message) => {
                            match p2p_message.payload {
                               Outbox::State(state) => {
                                _=tx.send(Inbox::State((p2p_message.peer_id, state)));
                               },
                            }

                        }
                        Err(e) => {
                            println!("swarm deserialize: {e}")
                        }
                    },
                    SwarmEvent::ConnectionEstablished { peer_id, .. } =>{
                        alive_peer_ids.insert(peer_id);
                        _=tx.send(Inbox::Nodes(alive_peer_ids.clone()));
                    },
                    SwarmEvent::ConnectionClosed { peer_id, ..}=>{
                        alive_peer_ids.remove(&peer_id);
                        _=tx.send(Inbox::Nodes(alive_peer_ids.clone()));
                    },
                    _ => {}
                    }
                }
            }
        }
    });
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct P2pMessage {
    peer_id: PeerId,

    // Only outbox can be sent. If more fields will be needed on interface vs network - split into two types
    // This is the only information in that enum that node can send to each other
    payload: Outbox,
}
