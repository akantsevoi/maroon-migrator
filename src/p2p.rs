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
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

use crate::interface;

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

/// Internal structure that holds channels for inter-module communication
struct Channels {
    tx_in: UnboundedSender<Inbox>,
    rx_in: Owned<UnboundedReceiver<Inbox>>,
    tx_out: UnboundedSender<Outbox>,
    rx_out: UnboundedReceiver<Outbox>,
}

enum Owned<T> {
    Here(T),
    Moved,
}

pub struct P2P {
    pub peer_id: PeerId,

    node_urls: Vec<String>,
    self_url: String,

    swarm: Swarm<MaroonBehaviour>,
    state_topic_hash: TopicHash,

    channels: Channels,
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

        let (tx_in, rx_in) = mpsc::unbounded_channel::<Inbox>();
        let (tx_out, rx_out) = mpsc::unbounded_channel::<Outbox>();

        Ok(P2P {
            node_urls,
            self_url,
            peer_id,
            swarm,
            state_topic_hash: state_topic.hash().clone(),
            channels: Channels {
                tx_in,
                rx_in: Owned::Here(rx_in),
                tx_out,
                rx_out,
            },
        })
    }

    /// starts listening and performs all the bindings but doesn't react yeat
    pub fn prepare(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.swarm
            .listen_on(self.self_url.parse()?)
            .map_err(|e| format!("swarm.listen {e}"))?;

        for url in self.node_urls.clone() {
            if url == self.self_url {
                continue;
            }

            let addr: Multiaddr = url.parse()?;
            println!("Dialing {addr} …");
            self.swarm.dial(addr)?;
        }

        Ok(())
    }

    /// Gets p2p channels that will be used for communication
    /// can be called only once
    pub fn interface_channels(&mut self) -> P2PChannels {
        let mut moved = Owned::Moved;
        std::mem::swap(&mut moved, &mut self.channels.rx_in);

        match moved {
            Owned::Moved => {
                todo!(
                    "already moved. This fn can be called only once. Maybe return proper error here?"
                )
            }
            Owned::Here(rx_in) => {
                return P2PChannels {
                    receiver: rx_in,
                    sender: self.channels.tx_out.clone(),
                };
            }
        }
    }

    /// blocking operation, so you might want to spawn it on a separate thread
    /// after calling this - channels at `interface_channels` will start to send messages
    /// TODO: add stop/finish channel
    pub async fn start_event_loop(self) {
        let mut alive_peer_ids: HashSet<PeerId> = HashSet::new();
        alive_peer_ids.insert(self.peer_id);
        let mut swarm = self.swarm;

        let mut rx_out = self.channels.rx_out;
        let tx_in = self.channels.tx_in;
        loop {
            tokio::select! {
                Some(Outbox::State(state)) = rx_out.recv() =>  {
                    let message = P2pMessage {
                        peer_id: self.peer_id,
                        payload: Outbox::State(state),
                    };

                    let bytes = guard_ok!(serde_json::to_vec(&message), e, {
                        println!("serialize message error: {e}");
                        continue;
                    });

                    if let Err(e) = swarm
                        .behaviour_mut()
                        .gossipsub
                        .publish(self.state_topic_hash.clone(), bytes){
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
                                _=tx_in.send(Inbox::State((p2p_message.peer_id, state)));
                               },
                            }

                        }
                        Err(e) => {
                            println!("swarm deserialize: {e}")
                        }
                    },
                    SwarmEvent::Behaviour(MaroonEvent::Ping(PingEvent { .. })) =>{
                        // TODO: have an idea to use result.duration for calculating logical time between nodes. let's see
                    },
                    SwarmEvent::ConnectionEstablished { peer_id, .. } =>{
                        alive_peer_ids.insert(peer_id);
                        _=tx_in.send(Inbox::Nodes(alive_peer_ids.clone()));
                    },
                    SwarmEvent::ConnectionClosed { peer_id, ..}=>{
                        alive_peer_ids.remove(&peer_id);
                        _=tx_in.send(Inbox::Nodes(alive_peer_ids.clone()));
                    },
                    _ => {}
                    }
                }
            }
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct P2pMessage {
    peer_id: PeerId,

    // Only outbox can be sent. If more fields will be needed on interface vs network - split into two types
    // This is the only information in that enum that node can send to each other
    payload: Outbox,
}
