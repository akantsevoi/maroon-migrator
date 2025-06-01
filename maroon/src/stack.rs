use crate::app::{App, Params, Request, Response};
use crate::linearizer::LogLineriazer;
use crate::network::{Inbox, Outbox, P2P};
use common::duplex_channel::create_a_b_duplex_pair;
use common::invoker_handler::{InvokerInterface, create_invoker_handler_pair};

pub fn create_stack(
  node_urls: Vec<String>,
  self_url: String,
  params: Params,
) -> Result<(App<LogLineriazer>, InvokerInterface<Request, Response>), Box<dyn std::error::Error>> {
  let (a2b_endpoint, b2a_endpoint) = create_a_b_duplex_pair::<Inbox, Outbox>();

  let mut p2p = P2P::new(node_urls, self_url, a2b_endpoint)?;
  let my_id = p2p.peer_id;

  _ = p2p.prepare()?;

  tokio::spawn(async move {
    p2p.start_event_loop().await;
  });

  let (state_invoker, state_handler) = create_invoker_handler_pair();

  Ok((
    App::<LogLineriazer>::new(my_id, b2a_endpoint, state_handler, params)?,
    state_invoker,
  ))
}
