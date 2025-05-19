use common::{
    gm_request_response::{
        self, Behaviour as GMBehaviour, Event as GMEvent, Request, Response, Transaction, TxStatus,
    },
    meta_exchange::{
        self, Behaviour as MetaExchangeBehaviour, Event as MEEvent, Request as MERequest,
        Response as MEResponse, Role,
    },
    range_key::TransactionID,
};
use futures::StreamExt;
use libp2p::dns::{self, Transport as DnsTransport};
use libp2p::{
    Multiaddr, PeerId,
    core::{transport::Transport as _, upgrade},
    identity,
    noise::{Config as NoiseConfig, Error as NoiseError},
    ping::{Behaviour as PingBehaviour, Config as PingConfig, Event as PingEvent},
    swarm::{Config as SwarmConfig, NetworkBehaviour, Swarm, SwarmEvent},
    tcp::{Config as TcpConfig, tokio::Transport as TcpTokioTransport},
    yamux::Config as YamuxConfig,
};
use libp2p_request_response::{Message as RequestResponseMessage, ProtocolSupport};
use std::{collections::HashSet, time::Duration};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

#[derive(NetworkBehaviour)]
#[behaviour(out_event = "GatewayEvent")]
struct GatewayBehaviour {
    ping: PingBehaviour,
    request_response: GMBehaviour,
    meta_exchange: MetaExchangeBehaviour,
}
pub enum GatewayEvent {
    Ping(PingEvent),
    RequestResponse(GMEvent),
    MetaExchange(MEEvent),
}

impl From<PingEvent> for GatewayEvent {
    fn from(e: PingEvent) -> Self {
        GatewayEvent::Ping(e)
    }
}

impl From<GMEvent> for GatewayEvent {
    fn from(e: GMEvent) -> Self {
        GatewayEvent::RequestResponse(e)
    }
}

impl From<MEEvent> for GatewayEvent {
    fn from(e: MEEvent) -> Self {
        GatewayEvent::MetaExchange(e)
    }
}

/// Internal structure that holds channels for inter-module communication
struct Channels {
    tx_request: UnboundedSender<Request>,
    rx_request: UnboundedReceiver<Request>,
    tx_response: UnboundedSender<Response>,
    rx_response: Value<UnboundedReceiver<Response>>,
}

pub struct P2PChannels {
    pub tx_request: UnboundedSender<Request>,
    pub rx_response: UnboundedReceiver<Response>,
}

enum Value<T> {
    Here(T),
    Moved,
}

pub struct P2P {
    pub peer_id: PeerId,

    node_urls: Vec<String>,

    swarm: Swarm<GatewayBehaviour>,
    channels: Channels,
}

impl P2P {
    pub fn new(node_urls: Vec<String>) -> Result<P2P, Box<dyn std::error::Error>> {
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

        let behaviour = GatewayBehaviour {
            ping: PingBehaviour::new(
                PingConfig::new()
                    .with_interval(Duration::from_secs(5))
                    .with_timeout(Duration::from_secs(10)),
            ),
            request_response: gm_request_response::create_behaviour(ProtocolSupport::Outbound),
            meta_exchange: meta_exchange::create_behaviour(),
        };

        let swarm = Swarm::new(
            DnsTransport::system(transport).unwrap().boxed(),
            behaviour,
            peer_id,
            SwarmConfig::with_tokio_executor()
                .with_idle_connection_timeout(Duration::from_secs(60)),
        );

        let (tx_request, rx_request) = mpsc::unbounded_channel::<Request>();
        let (tx_response, rx_response) = mpsc::unbounded_channel::<Response>();

        Ok(P2P {
            node_urls,
            peer_id,
            swarm,
            channels: Channels {
                tx_request,
                rx_request,
                tx_response,
                rx_response: Value::Here(rx_response),
            },
        })
    }

    /// starts listening and performs all the bindings but doesn't react yeat
    pub fn prepare(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        for url in &self.node_urls {
            let addr: Multiaddr = url
                .parse()
                .map_err(|e| format!("parse url: {}: {}", url, e))?;
            println!("Dialing {addr} …");
            self.swarm.dial(addr)?;
        }

        Ok(())
    }

    // Gets p2p channels that will be used for communication
    // can be called only once
    pub fn interface_channels(&mut self) -> P2PChannels {
        let mut moved = Value::Moved;
        std::mem::swap(&mut moved, &mut self.channels.rx_response);

        match moved {
            Value::Moved => {
                todo!(
                    "already moved. This fn can be called only once. Maybe return proper error here?"
                )
            }
            Value::Here(rx_resp) => {
                return P2PChannels {
                    tx_request: self.channels.tx_request.clone(),
                    rx_response: rx_resp,
                };
            }
        }
    }

    /// blocking operation, so you might want to spawn it on a separate thread
    /// after calling this - channels at `interface_channels` will start to send messages
    /// TODO: add stop/finish channel
    pub async fn start_event_loop(self) {
        let mut maroon_peer_ids = HashSet::<PeerId>::new();
        let mut swarm = self.swarm;

        let mut rx_request = self.channels.rx_request;
        let tx_response = self.channels.tx_response;
        loop {
            tokio::select! {
                Some(request) = rx_request.recv() =>  {
                    for peer_id in &maroon_peer_ids {
                        println!("Sending request to {}", peer_id);
                        let _request_id = swarm.behaviour_mut().request_response.send_request(peer_id, request.clone());
                    }
                },
                event = swarm.select_next_some()=>{
                    match event{
                        SwarmEvent::Behaviour(GatewayEvent::RequestResponse(gm_request_response)) => {
                            println!("RequestResponse: {:?}", gm_request_response);
                            match gm_request_response{
                                GMEvent::Message{ message, .. } => {
                                    match message{
                                        RequestResponseMessage::Response{request_id, response} => {
                                            println!("Response: {:?}, {:?}", request_id, response);
                                            tx_response.send(response).unwrap();
                                        },
                                        _=>{},
                                    }
                                },
                                _ => {},
                            }
                        },
                        SwarmEvent::Behaviour(GatewayEvent::MetaExchange(meta_exchange)) =>{
                            println!("MetaExchange: {:?}", meta_exchange);
                            match meta_exchange{
                                MEEvent::Message{ message, .. } => {
                                    match message{
                                        RequestResponseMessage::Response{request_id, response} => {
                                            println!("MetaExchangeResponse: {:?} {:?}", request_id, response);
                                        },
                                        RequestResponseMessage::Request{ channel,..} => {
                                            let res = swarm.behaviour_mut().meta_exchange.send_response(channel, MEResponse{role: Role::Gateway});
                                            println!("MetaExchangeRequestRes: {:?}", res);
                                        },
                                        _=>{},
                                    }
                                },
                                _=>{},
                            }
                        },
                    // SwarmEvent::Behaviour(GatewayEvent::Ping(PingEvent { .. })) =>{
                    //     // TODO: have an idea to use result.duration for calculating logical time between nodes. let's see
                    // },
                    SwarmEvent::ConnectionEstablished { peer_id, .. } =>{
                        maroon_peer_ids.insert(peer_id);
                        println!("connected to {}", peer_id);
                    },
                    SwarmEvent::ConnectionClosed { peer_id, ..}=>{
                        maroon_peer_ids.remove(&peer_id);
                        println!("disconnected from {}", peer_id);
                    },
                    _ => {}
                    }
                }
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    //     let node_urls = std::env::var("NODE_URLS");
    let node_urls = Result::<String, std::env::VarError>::Ok(String::from(
        "/ip4/127.0.0.1/tcp/3000,/ip4/127.0.0.1/tcp/3001,/ip4/127.0.0.1/tcp/3002",
    ));
    // let node_urls =
    //     Result::<String, std::env::VarError>::Ok(String::from("/dns4/localhost/tcp/3000"));

    let node_urls: Vec<String> = node_urls
        .map_err(|e| format!("NODE_URLS not set: {}", e))?
        .split(',')
        .map(String::from)
        .collect();

    let mut p2p = P2P::new(node_urls)?;
    p2p.prepare().map_err(|e| format!("prepare: {}", e))?;

    let p2p_channels = p2p.interface_channels();

    tokio::spawn(async move {
        p2p.start_event_loop().await;
    });

    tokio::time::sleep(Duration::from_secs(2)).await;

    for i in 0..10 {
        p2p_channels
            .tx_request
            .send(Request::NewTransaction(Transaction {
                id: TransactionID(i),
                status: TxStatus::Created,
            }))
            .unwrap();
        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    tokio::time::sleep(Duration::from_secs(10)).await;
    Ok(())
}
