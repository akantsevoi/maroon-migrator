use super::interface::{Inbox, Outbox};
use crate::network::interface::NodeState;
use common::{
  duplex_channel::Endpoint,
  gm_request_response::{
    self, Behaviour as GMBehaviour, Event as GMEvent, Request as GMRequest, Response as GMResponse,
  },
  m2m_request_response::{
    self, Behaviour as M2MBehaviour, Event as M2MEvent, Request as M2MRequest,
    Response as M2MResponse,
  },
  meta_exchange::{
    self, Behaviour as MetaExchangeBehaviour, Event as MEEvent, Request as MERequest,
    Response as MEResponse, Role,
  },
};
use derive_more::From;
use futures::StreamExt;
use libp2p::dns::Transport as DnsTransport;
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
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, fmt::Debug, time::Duration};
use tokio::sync::mpsc::UnboundedSender;

#[derive(NetworkBehaviour)]
#[behaviour(out_event = "MaroonEvent")]
struct MaroonBehaviour {
  ping: PingBehaviour,
  gossipsub: GossipsubBehaviour,
  request_response: GMBehaviour,
  meta_exchange: MetaExchangeBehaviour,
  m2m_req_res: M2MBehaviour,
}

#[derive(From)]
pub enum MaroonEvent {
  Ping(PingEvent),
  Gossipsub(GossipsubEvent),
  RequestResponse(GMEvent),
  MetaExchange(MEEvent),
  M2MReqRes(M2MEvent),
}

pub struct P2P {
  pub peer_id: PeerId,

  node_urls: Vec<String>,
  self_url: String,

  swarm: Swarm<MaroonBehaviour>,
  node_p2p_topic: TopicHash,

  interface_endpoint: Endpoint<Inbox, Outbox>,
}

impl P2P {
  pub fn new(
    node_urls: Vec<String>,
    self_url: String,
    interface_endpoint: Endpoint<Inbox, Outbox>,
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
      m2m_req_res: m2m_request_response::create_behaviour(),
    };

    let swarm = Swarm::new(
      DnsTransport::system(transport).unwrap().boxed(),
      behaviour,
      peer_id,
      SwarmConfig::with_tokio_executor().with_idle_connection_timeout(Duration::from_secs(60)),
    );

    Ok(P2P {
      node_urls,
      self_url,
      peer_id,
      swarm,
      node_p2p_topic: node_p2p_topic.hash().clone(),
      interface_endpoint,
    })
  }

  /// starts listening and performs all the bindings but doesn't react yeat
  pub fn prepare(&mut self) -> Result<(), Box<dyn std::error::Error>> {
    self
      .swarm
      .listen_on(self.self_url.parse()?)
      .map_err(|e| format!("swarm.listen {e}"))?;

    for url in self.node_urls.clone() {
      if url == self.self_url {
        continue;
      }

      let addr: Multiaddr = url.parse()?;
      debug!("Dialing {addr} …");
      self.swarm.dial(addr)?;
    }

    Ok(())
  }

  /// blocking operation, so you might want to spawn it on a separate thread
  /// after calling this - channels at `interface_channels` will start to send messages
  /// TODO: add stop/finish channel
  pub async fn start_event_loop(self) {
    let mut alive_peer_ids: HashSet<PeerId> = HashSet::new();
    let mut alive_gateway_ids: HashSet<PeerId> = HashSet::new();

    alive_peer_ids.insert(self.peer_id);
    let mut swarm = self.swarm;

    let mut receiver = self.interface_endpoint.receiver;
    let to_app = self.interface_endpoint.sender;

    loop {
      tokio::select! {
          Some(outbox) = receiver.recv() => {
              handle_receiver_outbox(
                  &mut swarm,
                  outbox,
                  self.peer_id,
                  self.node_p2p_topic.clone(),
              );
          },
          event = swarm.select_next_some() => {
              handle_swarm_event(
                  &mut swarm,
                  event,
                  &to_app,
                  &mut alive_peer_ids,
                  &mut alive_gateway_ids,
              );
          }
      }
    }
  }
}

fn handle_receiver_outbox(
  swarm: &mut Swarm<MaroonBehaviour>,
  outbox_message: Outbox,
  peer_id: PeerId,
  node_p2p_topic: TopicHash,
) {
  match outbox_message {
    Outbox::State(state) => {
      let message = GossipMessage {
        peer_id: peer_id,
        payload: GossipPayload::State(state),
      };

      let bytes = guard_ok!(serde_json::to_vec(&message), e, {
        error!("serialize message error: {e}");
        return;
      });

      if let Err(e) = swarm
        .behaviour_mut()
        .gossipsub
        .publish(node_p2p_topic, bytes)
      {
        warn!("gossip broadcast error: {}", e);
      }
    }
    Outbox::RequestMissingTxs((peer_id, ranges)) => {
      swarm
        .behaviour_mut()
        .m2m_req_res
        .send_request(&peer_id, M2MRequest::GetMissingTx(ranges));
    }
    Outbox::RequestedTxsForPeer((peer_id, missing_txs)) => {
      swarm
        .behaviour_mut()
        .m2m_req_res
        .send_request(&peer_id, M2MRequest::MissingTx(missing_txs));
    }
  }
}

fn handle_swarm_event(
  swarm: &mut Swarm<MaroonBehaviour>,
  event: SwarmEvent<MaroonEvent>,
  to_app: &UnboundedSender<Inbox>,
  alive_peer_ids: &mut HashSet<PeerId>,
  alive_gateway_ids: &mut HashSet<PeerId>,
) {
  match event {
    SwarmEvent::Behaviour(MaroonEvent::Gossipsub(GossipsubEvent::Message { message, .. })) => {
      match serde_json::from_slice::<GossipMessage>(&message.data) {
        Ok(p2p_message) => match p2p_message.payload {
          GossipPayload::State(state) => {
            _ = to_app.send(Inbox::State((p2p_message.peer_id, state)));
          }
        },
        Err(e) => {
          error!("swarm deserialize: {e}");
        }
      }
    }
    SwarmEvent::Behaviour(MaroonEvent::MetaExchange(meta_exchange)) => {
      handle_meta_exchange(
        swarm,
        meta_exchange,
        alive_peer_ids,
        alive_gateway_ids,
        to_app,
      );
    }
    SwarmEvent::Behaviour(MaroonEvent::Ping(PingEvent { .. })) => {
      // TODO: have an idea to use result.duration for calculating logical time between nodes. let's see
    }
    SwarmEvent::Behaviour(MaroonEvent::RequestResponse(gm_request_response)) => {
      handle_request_response(swarm, &to_app, gm_request_response);
    }
    SwarmEvent::Behaviour(MaroonEvent::M2MReqRes(m2m_request_response)) => {
      handle_m2m_req_res(swarm, &to_app, m2m_request_response);
    }
    SwarmEvent::ConnectionEstablished { peer_id, .. } => {
      swarm
        .behaviour_mut()
        .meta_exchange
        .send_request(&peer_id, MERequest { role: Role::Node });
    }
    SwarmEvent::ConnectionClosed { peer_id, .. } => {
      alive_gateway_ids.remove(&peer_id);
      if alive_peer_ids.remove(&peer_id) {
        _ = to_app.send(Inbox::Nodes(alive_peer_ids.clone()));
      }
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

fn handle_m2m_req_res(
  swarm: &mut Swarm<MaroonBehaviour>,
  to_app: &UnboundedSender<Inbox>,
  m2m_request_response: M2MEvent,
) {
  let M2MEvent::Message { message, peer, .. } = m2m_request_response else {
    return;
  };

  let RequestResponseMessage::Request {
    request, channel, ..
  } = message
  else {
    return;
  };

  match request {
    M2MRequest::GetMissingTx(ranges) => to_app
      .send(Inbox::RequestMissingTxs((peer, ranges)))
      .expect("TODO: shouldnt panic?"),
    M2MRequest::MissingTx(missing_txs) => to_app
      .send(Inbox::MissingTx(missing_txs))
      .expect("TODO: shouldnt panic?"),
  }

  _ = swarm
    .behaviour_mut()
    .m2m_req_res
    .send_response(channel, M2MResponse::Ack);
}

fn handle_request_response(
  swarm: &mut Swarm<MaroonBehaviour>,
  to_app: &UnboundedSender<Inbox>,
  gm_request_response: GMEvent,
) {
  match gm_request_response {
    GMEvent::Message { message, .. } => match message {
      RequestResponseMessage::Request {
        request_id,
        request,
        channel,
      } => {
        debug!("Got request: {:?}, {:?}", request_id, request);

        match request {
          GMRequest::NewTransaction(tx) => {
            _ = to_app.send(Inbox::NewTransaction(tx));

            _ = swarm
              .behaviour_mut()
              .request_response
              .send_response(channel, GMResponse::Acknowledged);
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
  to_app: &UnboundedSender<Inbox>,
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
      _ = to_app.send(Inbox::Nodes(alive_node_ids.clone()));
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
      debug!("MetaExchangeRequestRes: {:?}", res);
      insert_by_role(request.role);
    }
  }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct GossipMessage {
  peer_id: PeerId,

  // This is the only information in that enum that node can gossip to each other
  payload: GossipPayload,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
enum GossipPayload {
  State(NodeState),
}
