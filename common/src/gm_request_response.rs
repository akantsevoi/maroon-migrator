use crate::transaction::Transaction;
use libp2p::swarm::StreamProtocol;
use libp2p_request_response::{
  self as request_response, Event as RequestResponseEvent, ProtocolSupport,
  json::{self},
};
use serde::{Deserialize, Serialize};
use std::fmt::Debug;

pub type Event = RequestResponseEvent<Request, Response>;
pub type Behaviour = json::Behaviour<Request, Response>;

pub fn create_behaviour(protocol: ProtocolSupport) -> json::Behaviour<Request, Response> {
  json::Behaviour::<Request, Response>::new(
    [(StreamProtocol::new("/maroon/request/1.0.0"), protocol)],
    request_response::Config::default(),
  )
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type", content = "data")]
pub enum Request {
  NewTransaction(Transaction),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum Response {
  Acknowledged,
  Rejected,
}
