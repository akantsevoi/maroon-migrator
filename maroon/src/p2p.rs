use common::{
    gm_request_response::{self, Behaviour as GMBehaviour, Event as GMEvent, Request, Response},
    meta_exchange::{
        self, Behaviour as MetaExchangeBehaviour, Event as MEEvent, Request as MERequest,
        Response as MEResponse, Role,
    },
};
use futures::StreamExt;
use libp2p::dns::{self, Transport as DnsTransport};
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
use libp2p_request_response::{Message as RequestResponseMessage, ProtocolSupport};
use log::{debug, info};
use p2p_interface::{Inbox, Outbox, P2PChannels};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, fmt::Debug, time::Duration};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

use crate::p2p_interface;

#[derive(NetworkBehaviour)]
#[behaviour(out_event = "MaroonEvent")]
struct MaroonBehaviour {
    ping: PingBehaviour,
    gossipsub: GossipsubBehaviour,
    request_response: GMBehaviour,
    meta_exchange: MetaExchangeBehaviour,
}

pub enum MaroonEvent {
    Ping(PingEvent),
    Gossipsub(GossipsubEvent),
    RequestResponse(GMEvent),
    MetaExchange(MEEvent),
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

impl From<GMEvent> for MaroonEvent {
    fn from(e: GMEvent) -> Self {
        MaroonEvent::RequestResponse(e)
    }
}

impl From<MEEvent> for MaroonEvent {
    fn from(e: MEEvent) -> Self {
        MaroonEvent::MetaExchange(e)
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
    node_p2p_topic: TopicHash,

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

        let node_p2p_topic = Sha256Topic::new("node-p2p");
        gossipsub.subscribe(&node_p2p_topic)?;

        let behaviour = MaroonBehaviour {
            ping: PingBehaviour::new(
                PingConfig::new()
                    .with_interval(Duration::from_secs(5))
                    .with_timeout(Duration::from_secs(10)),
            ),
            gossipsub,
            request_response: gm_request_response::create_behaviour(ProtocolSupport::Inbound),
            meta_exchange: meta_exchange::create_behaviour(),
        };

        let swarm = Swarm::new(
            DnsTransport::system(transport).unwrap().boxed(),
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
            node_p2p_topic: node_p2p_topic.hash().clone(),
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
        let mut alive_gateway_ids: HashSet<PeerId> = HashSet::new();

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
                        .publish(self.node_p2p_topic.clone(), bytes){
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
                    SwarmEvent::Behaviour(MaroonEvent::MetaExchange(meta_exchange)) =>{
                        handle_meta_exchange(&mut swarm, meta_exchange, &mut alive_peer_ids, &mut alive_gateway_ids, &tx_in);
                    },
                    SwarmEvent::Behaviour(MaroonEvent::Ping(PingEvent { .. })) =>{
                        // TODO: have an idea to use result.duration for calculating logical time between nodes. let's see
                    },
                    SwarmEvent::Behaviour(MaroonEvent::RequestResponse(gm_request_response)) =>{
                        handle_request_response(&mut swarm, &tx_in, gm_request_response);
                    },
                    SwarmEvent::ConnectionEstablished { peer_id, .. } =>{
                        swarm.behaviour_mut().meta_exchange.send_request(&peer_id, MERequest{role: Role::Node});
                    },
                    SwarmEvent::ConnectionClosed { peer_id, ..}=>{
                        alive_gateway_ids.remove(&peer_id);
                        if alive_peer_ids.remove(&peer_id) {
                            _=tx_in.send(Inbox::Nodes(alive_peer_ids.clone()));
                        }
                    },
                    SwarmEvent::OutgoingConnectionError { peer_id, connection_id, error } => {
                        debug!("OutgoingConnectionError: {peer_id:?} {connection_id} {error}");
                    },
                    _ => {}
                    }
                }
            }
        }
    }
}

fn handle_request_response(
    swarm: &mut Swarm<MaroonBehaviour>,
    tx_in: &UnboundedSender<Inbox>,
    gm_request_response: GMEvent,
) {
    match gm_request_response {
        GMEvent::Message { message, .. } => match message {
            RequestResponseMessage::Request {
                request_id,
                request,
                channel,
            } => {
                println!("Request: {:?}, {:?}", request_id, request);

                match request {
                    Request::NewTransaction(tx) => {
                        _ = tx_in.send(Inbox::NewTransaction(tx));

                        let res = swarm
                            .behaviour_mut()
                            .request_response
                            .send_response(channel, Response::Acknowledged);
                        println!("Response sent: {:?}", res);
                    }
                }
            }
            _ => {}
        },
        _ => {}
    }
}

fn handle_meta_exchange(
    swarm: &mut Swarm<MaroonBehaviour>,
    meta_exchange: MEEvent,
    alive_node_ids: &mut HashSet<PeerId>,
    alive_gateway_ids: &mut HashSet<PeerId>,
    tx_in: &UnboundedSender<Inbox>,
) {
    let MEEvent::Message { message, peer, .. } = meta_exchange else {
        return;
    };

    let mut insert_by_role = |role: Role| match role {
        Role::Gateway => {
            alive_gateway_ids.insert(peer);
        }
        Role::Node => {
            alive_node_ids.insert(peer);
            _ = tx_in.send(Inbox::Nodes(alive_node_ids.clone()));
        }
    };

    match message {
        RequestResponseMessage::Response { response, .. } => insert_by_role(response.role),
        RequestResponseMessage::Request {
            channel, request, ..
        } => {
            let res = swarm
                .behaviour_mut()
                .meta_exchange
                .send_response(channel, MEResponse { role: Role::Node });
            println!("MetaExchangeRequestRes: {:?}", res);
            insert_by_role(request.role);
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
