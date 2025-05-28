use log::error;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

pub fn create_a_b_duplex_pair<A, B>() -> (Endpoint<A, B>, Endpoint<B, A>) {
    let (tx_request, rx_request) = mpsc::unbounded_channel::<A>();
    let (tx_response, rx_response) = mpsc::unbounded_channel::<B>();

    (
        Endpoint {
            sender: tx_request,
            receiver: rx_response,
        },
        Endpoint {
            sender: tx_response,
            receiver: rx_request,
        },
    )
}

pub struct Endpoint<A, B> {
    pub sender: UnboundedSender<A>,
    pub receiver: UnboundedReceiver<B>,
}

impl<A, B> Endpoint<A, B> {
    pub fn send(&self, message: A) {
        let res = self.sender.send(message);
        if let Err(unsent) = res {
            // TODO: should I panic here?
            // keeping in mind why this interface was created - channels shouldn't be dropped. It's not normal
            error!("channel dropped: {unsent}");
        }
    }
}
