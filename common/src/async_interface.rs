use crate::value::Value;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

/// Inter-modules communication
pub struct AsyncInterface<Req, Res> {
    tx_request: Value<UnboundedSender<Req>>,
    rx_request: Value<UnboundedReceiver<Req>>,
    tx_response: Value<UnboundedSender<Res>>,
    rx_response: Value<UnboundedReceiver<Res>>,
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
            tx_request: Value::Here(tx_request),
            rx_request: Value::Here(rx_request),
            tx_response: Value::Here(tx_response),
            rx_response: Value::Here(rx_response),
        }
    }

    /// can be called only once
    pub fn requester(&mut self) -> ReqResPair<Req, Res> {
        ReqResPair {
            sender: self.tx_request.extract(),
            receiver: self.rx_response.extract(),
        }
    }

    /// can be called only once
    pub fn responder(&mut self) -> ReqResPair<Res, Req> {
        ReqResPair {
            sender: self.tx_response.extract(),
            receiver: self.rx_request.extract(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::AsyncInterface;

    #[test]
    fn common() {
        let mut interface = AsyncInterface::<String, u32>::new();
        let _ = interface.requester();
        let _ = interface.responder();

        assert!(!interface.rx_request.contains());
        assert!(!interface.tx_request.contains());
        assert!(!interface.rx_response.contains());
        assert!(!interface.tx_response.contains());
    }
}
