use crate::p2p::P2P;
use common::{
    async_interface::ReqResPair,
    gm_request_response::{Request, Response},
    meta_exchange::{Response as MEResponse, Role},
};
pub struct Gateway {
    p2p_channels: ReqResPair<Request, Response>,
    p2p: Option<P2P>,
}

impl Gateway {
    pub fn new(node_urls: Vec<String>) -> Result<Gateway, Box<dyn std::error::Error>> {
        let mut p2p = P2P::new(node_urls)?;
        // TODO: prepare works in background and you can't start sending requests immediately when you created Gateway
        // I need to create some sort of state/flags or block the thread that can prevent sending requests before initialization even happened
        p2p.prepare().map_err(|e| format!("prepare: {}", e))?;

        return Ok(Gateway {
            p2p_channels: p2p.interface_channels(),
            p2p: Some(p2p),
        });
    }

    pub async fn start_on_background(&mut self) {
        let p2p = self.p2p.take().expect("can be called only once");

        tokio::spawn(async move {
            p2p.start_event_loop().await;
        });
    }

    pub async fn send_request(
        &mut self,
        request: Request,
    ) -> Result<MEResponse, Box<dyn std::error::Error>> {
        self.p2p_channels.sender.send(request)?;

        // TODO: that one doesn't work correctly. I need to listen to a related rx_response to get the right info
        return Ok(MEResponse { role: Role::Node });
    }
}
