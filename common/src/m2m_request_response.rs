use crate::range_key::U64BlobIdClosedInterval;
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

pub fn create_behaviour() -> json::Behaviour<Request, Response> {
  json::Behaviour::<Request, Response>::new(
    [(
      StreamProtocol::new("/maroon/p2p_direct/1.0.0"),
      ProtocolSupport::Full,
    )],
    request_response::Config::default(),
  )
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type", content = "data")]
pub enum Request {
  /// request missing transactions for given ranges
  ///
  /// TODO: there should be limit. If delay is too big - node should get them from s3
  /// this one is only for small batch of txs
  GetMissingTx(Vec<U64BlobIdClosedInterval>),

  /// sends missing transactions
  MissingTx(Vec<Transaction>),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum Response {
  Ack,
}
