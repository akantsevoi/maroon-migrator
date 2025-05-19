use crate::app::App;
use crate::p2p::P2P;
use tokio::sync::oneshot;

pub async fn create_stack_and_loop_until_shutdown(
    node_urls: Vec<String>,
    self_url: String,
    shutdown: oneshot::Receiver<()>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut p2p = P2P::new(node_urls, self_url)?;
    let my_id = p2p.peer_id;

    let p2p_channels = p2p.interface_channels();
    _ = p2p.prepare()?;

    tokio::spawn(async move {
        p2p.start_event_loop().await;
    });

    let mut app = App::new(my_id, p2p_channels)?;

    app.loop_until_shutdown(shutdown).await;

    Ok(())
}
