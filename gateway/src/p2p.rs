use common::{
  duplex_channel::Endpoint,
  gm_request_response::{self, Behaviour as GMBehaviour, Event as GMEvent, Request, Response},
  meta_exchange::{
    self, Behaviour as MetaExchangeBehaviour, Event as MEEvent, Response as MEResponse, Role,
  },
};
use derive_more::From;
use futures::StreamExt;
use libp2p::dns::Transport as DnsTransport;
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
use log::{debug, info};
use std::{collections::HashSet, time::Duration};
use tokio::sync::mpsc::UnboundedSender;

#[derive(NetworkBehaviour)]
#[behaviour(out_event = "GatewayEvent")]
struct GatewayBehaviour {
  ping: PingBehaviour,
  request_response: GMBehaviour,
  meta_exchange: MetaExchangeBehaviour,
}

#[derive(From)]
pub enum GatewayEvent {
  Ping(PingEvent),
  RequestResponse(GMEvent),
  MetaExchange(MEEvent),
}

pub struct P2P {
  node_urls: Vec<String>,

  swarm: Swarm<GatewayBehaviour>,

  interface_endpoint: Endpoint<Response, Request>,
}

impl P2P {
  pub fn new(
    node_urls: Vec<String>,
    interface_endpoint: Endpoint<Response, Request>,
  ) -> Result<P2P, Box<dyn std::error::Error>> {
    let kp = identity::Keypair::generate_ed25519();
    let peer_id = PeerId::from(kp.public());
    info!("Local peer id: {:?}", peer_id);

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
      SwarmConfig::with_tokio_executor().with_idle_connection_timeout(Duration::from_secs(60)),
    );

    Ok(P2P {
      node_urls,
      swarm,
      interface_endpoint,
    })
  }

  /// starts listening and performs all the bindings but doesn't react yeat
  pub fn prepare(&mut self) -> Result<(), Box<dyn std::error::Error>> {
    for url in &self.node_urls {
      let addr: Multiaddr = url
        .parse()
        .map_err(|e| format!("parse url: {}: {}", url, e))?;
      debug!("Dialing {addr} …");
      self.swarm.dial(addr)?;
    }

    Ok(())
  }

  /// blocking operation, so you might want to spawn it on a separate thread
  /// after calling this - channels at `interface_channels` will start to send messages
  /// TODO: add stop/finish channel
  pub async fn start_event_loop(self) {
    let mut maroon_peer_ids = HashSet::<PeerId>::new();
    let mut swarm = self.swarm;

    let mut receiver = self.interface_endpoint.receiver;
    let sender = self.interface_endpoint.sender;
    loop {
      tokio::select! {
          Some(request) = receiver.recv() => {
              for peer_id in &maroon_peer_ids {
                  debug!("Sending request {request:?} to {peer_id}");
                  let _request_id = swarm.behaviour_mut().request_response.send_request(peer_id, request.clone());
              }
          },
          event = swarm.select_next_some() => {
              handle_swarm_event(
                  &mut swarm,
                  event,
                  &sender,
                  &mut maroon_peer_ids,
              );
          }
      }
    }
  }
}

fn handle_swarm_event(
  swarm: &mut Swarm<GatewayBehaviour>,
  event: SwarmEvent<GatewayEvent>,
  sender: &UnboundedSender<Response>,
  maroon_peer_ids: &mut HashSet<PeerId>,
) {
  match event {
    SwarmEvent::Behaviour(GatewayEvent::RequestResponse(gm_request_response)) => {
      debug!("RequestResponse: {:?}", gm_request_response);
      match gm_request_response {
        GMEvent::Message { message, .. } => match message {
          RequestResponseMessage::Response {
            request_id,
            response,
          } => {
            debug!("Response: {:?}, {:?}", request_id, response);
            sender.send(response).unwrap();
          }
          _ => {}
        },
        _ => {}
      }
    }
    SwarmEvent::Behaviour(GatewayEvent::MetaExchange(meta_exchange)) => {
      debug!("MetaExchange: {:?}", meta_exchange);
      match meta_exchange {
        MEEvent::Message { message, .. } => match message {
          RequestResponseMessage::Response {
            request_id,
            response,
          } => {
            debug!("MetaExchangeResponse: {:?} {:?}", request_id, response);
          }
          RequestResponseMessage::Request { channel, .. } => {
            let res = swarm.behaviour_mut().meta_exchange.send_response(
              channel,
              MEResponse {
                role: Role::Gateway,
              },
            );
            debug!("MetaExchangeRequestRes: {:?}", res);
          }
        },
        _ => {}
      }
    }
    SwarmEvent::Behaviour(GatewayEvent::Ping(PingEvent { .. })) => {
      // TODO: have an idea to use result.duration for calculating logical time between nodes. let's see
    }
    SwarmEvent::ConnectionEstablished { peer_id, .. } => {
      maroon_peer_ids.insert(peer_id);
      debug!("connected to {}", peer_id);
    }
    SwarmEvent::ConnectionClosed { peer_id, .. } => {
      maroon_peer_ids.remove(&peer_id);
      debug!("disconnected from {}", peer_id);
    }

    SwarmEvent::OutgoingConnectionError {
      peer_id,
      connection_id,
      error,
    } => {
      debug!("OutgoingConnectionError: {peer_id:?} {connection_id} {error}");
    }
    _ => {}
  }
}
