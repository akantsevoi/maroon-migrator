use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

/// Inter-modules communication
pub struct AsyncInterface<Req, Res> {
    tx_request: Option<UnboundedSender<Req>>,
    rx_request: Option<UnboundedReceiver<Req>>,
    tx_response: Option<UnboundedSender<Res>>,
    rx_response: Option<UnboundedReceiver<Res>>,
}

pub struct ReqResPair<S, R> {
    pub sender: UnboundedSender<S>,
    pub receiver: UnboundedReceiver<R>,
}

impl<Req, Res> AsyncInterface<Req, Res> {
    pub fn new() -> AsyncInterface<Req, Res> {
        let (tx_request, rx_request) = mpsc::unbounded_channel::<Req>();
        let (tx_response, rx_response) = mpsc::unbounded_channel::<Res>();

        AsyncInterface {
            tx_request: Some(tx_request),
            rx_request: Some(rx_request),
            tx_response: Some(tx_response),
            rx_response: Some(rx_response),
        }
    }

    /// can be called only once
    pub fn requester(&mut self) -> ReqResPair<Req, Res> {
        ReqResPair {
            sender: self
                .tx_request
                .take()
                .expect("should be moved out only once"),
            receiver: self
                .rx_response
                .take()
                .expect("should be moved out only once"),
        }
    }

    /// can be called only once
    pub fn responder(&mut self) -> ReqResPair<Res, Req> {
        ReqResPair {
            sender: self
                .tx_response
                .take()
                .expect("should be moved out only once"),
            receiver: self
                .rx_request
                .take()
                .expect("should be moved out only once"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::AsyncInterface;

    #[test]
    fn common() {
        let mut interface = AsyncInterface::<String, u32>::new();
        _ = interface.requester();
        _ = interface.responder();

        assert!(interface.rx_request.is_none());
        assert!(interface.tx_request.is_none());
        assert!(interface.rx_response.is_none());
        assert!(interface.tx_response.is_none());
    }
}
