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
