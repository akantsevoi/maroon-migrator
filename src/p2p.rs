use std::{collections::HashSet, fmt::Debug, time::Duration};

use futures::StreamExt;
use libp2p::{
    Multiaddr, PeerId,
    core::{transport::Transport as _, upgrade},
    gossipsub::{
        Behaviour as GossipsubBehaviour, ConfigBuilder as GossipsubConfigBuilder,
        Event as GossipsubEvent, MessageAuthenticity, Sha256Topic, TopicHash, ValidationMode,
    },
    identity::{self, Keypair},
    noise::{Config as NoiseConfig, Error as NoiseError},
    ping::{Behaviour as PingBehaviour, Config as PingConfig, Event as PingEvent},
    swarm::{Config as SwarmConfig, NetworkBehaviour, Swarm, SwarmEvent},
    tcp::{Config as TcpConfig, tokio::Transport as TcpTokioTransport},
    yamux::Config as YamuxConfig,
};
use serde::{Deserialize, Serialize, de::value};
use tokio::sync::mpsc;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct NodeState {
    pub value: i32,
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

pub struct P2P {
    node_urls: Vec<String>,
    self_url: String,

    kp: Keypair,
    peer_id: PeerId,

    swarm: Swarm<MaroonBehaviour>,
    state_topic_hash: TopicHash,
}

pub struct P2PChannels {
    pub receiver: mpsc::UnboundedReceiver<Payload>,
    pub sender: mpsc::UnboundedSender<Payload>,
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
            kp,
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

        let mut alive_peer_ids: HashSet<PeerId> = HashSet::new();
        alive_peer_ids.insert(self.peer_id);

        let (tx, rx) = mpsc::unbounded_channel::<Payload>();
        let (tx_in, mut rx_in) = mpsc::unbounded_channel::<Payload>();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    Some(cmd) = rx_in.recv() =>  {
                        let message = P2pMessage {
                            peer_id: self.peer_id,
                            payload: cmd,
                        };

                        guard_ok!(serde_json::to_vec(&message), bytes, e, {
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
                                _ = tx.send(p2p_message.payload);
                            }
                            Err(e) => {
                                println!("deserialize: {e}")
                            }
                        },
                        _ => {}
                        }
                    }
                }
            }
        });

        return Ok(P2PChannels {
            receiver: rx,
            sender: tx_in,
        });

        /*
        loop {
            tokio::select! {
                _= ticker.tick() => {



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
        */
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", content = "data")]
pub enum Payload {
    State(NodeState),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct P2pMessage {
    pub peer_id: PeerId,
    pub payload: Payload,
}

// Public interface. Make it as trait??
impl P2P {
    pub fn broadcast(&mut self, state: NodeState) -> Result<(), Box<dyn std::error::Error>> {
        let message = P2pMessage {
            peer_id: self.peer_id,
            payload: Payload::State(state),
        };

        let bytes = serde_json::to_vec(&message)?;
        _ = self
            .swarm
            .behaviour_mut()
            .gossipsub
            .publish(self.state_topic_hash.clone(), bytes)?;

        Ok(())
    }
}
