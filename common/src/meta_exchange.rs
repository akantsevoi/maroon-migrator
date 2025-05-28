use libp2p::swarm::StreamProtocol;
use libp2p_request_response::{
  self as request_response, Event as RequestResponseEvent, ProtocolSupport,
  json::{self},
};
use serde::{Deserialize, Serialize};
use std::fmt::Debug;

pub type Event = RequestResponseEvent<Request, Response>;
pub type Behaviour = json::Behaviour<Request, Response>;

pub fn create_behaviour() -> json::Behaviour<Request, Response> {
  json::Behaviour::<Request, Response>::new(
    [(
      StreamProtocol::new("/maroon/meta_exchange/1.0.0"),
      ProtocolSupport::Full,
    )],
    request_response::Config::default(),
  )
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Request {
  pub role: Role,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Response {
  pub role: Role,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum Role {
  Gateway,
  Node,
}
