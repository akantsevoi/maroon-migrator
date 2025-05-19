use crate::p2p::{P2P, P2PChannels};
use common::{
    gm_request_response::Request,
    meta_exchange::{Response, Role},
};
pub struct Gateway {
    p2p_channels: P2PChannels,
    p2p: Value<P2P>,
}

enum Value<T> {
    Here(T),
    Moved,
}

impl<T> Value<T> {
    /// Can be performed only if Value::Here, otherwise it will panic
    fn extract(&mut self) -> T {
        let mut moved = Value::Moved;
        std::mem::swap(&mut moved, self);

        match moved {
            Value::Here(val) => return val,
            Value::Moved => panic!("was already moved"),
        }
    }
}

impl Gateway {
    pub fn new(node_urls: Vec<String>) -> Result<Gateway, Box<dyn std::error::Error>> {
        let mut p2p = P2P::new(node_urls)?;
        p2p.prepare().map_err(|e| format!("prepare: {}", e))?;

        let p2p_channels = p2p.interface_channels();

        return Ok(Gateway {
            p2p_channels,
            p2p: Value::Here(p2p),
        });
    }

    pub async fn start_on_background(&mut self) {
        let p2p = self.p2p.extract();

        tokio::spawn(async move {
            p2p.start_event_loop().await;
        });
    }

    pub async fn send_request(
        &mut self,
        request: Request,
    ) -> Result<Response, Box<dyn std::error::Error>> {
        self.p2p_channels.tx_request.send(request)?;

        // TODO: that one doesn't work correctly. I need to listen to a related rx_response to get the right info
        return Ok(Response { role: Role::Node });
    }
}
