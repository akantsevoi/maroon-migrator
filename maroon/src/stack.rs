use common::invoker_handler::{InvokerInterface, create_invoker_handler_pair};

use crate::app::{App, Params};
use crate::app_interface::{Request, Response};
use crate::p2p::P2P;

pub fn create_stack(
    node_urls: Vec<String>,
    self_url: String,
    params: Params,
) -> Result<(App, InvokerInterface<Request, Response>), Box<dyn std::error::Error>> {
    let mut p2p = P2P::new(node_urls, self_url)?;
    let my_id = p2p.peer_id;

    let p2p_channels = p2p.interface_channels();
    _ = p2p.prepare()?;

    tokio::spawn(async move {
        p2p.start_event_loop().await;
    });

    let (state_invoker, state_handler) = create_invoker_handler_pair();

    Ok((
        App::new(my_id, p2p_channels, state_handler, params)?,
        state_invoker,
    ))
}
