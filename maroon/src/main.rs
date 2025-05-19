#[macro_use]
mod macros;

mod app;
mod p2p;
mod p2p_interface;

use app::App;
use std::time::Duration;
use tokio::sync::oneshot;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let node_urls: Vec<String> = std::env::var("NODE_URLS")
        .map_err(|e| format!("NODE_URLS not set: {}", e))?
        .split(',')
        .map(String::from)
        .collect();

    let self_url: String =
        std::env::var("SELF_URL").map_err(|e| format!("SELF_URL not set: {}", e))?;

    let _tick = Duration::from_millis(
        std::env::var("TICK")
            .unwrap_or("60".to_string())
            .parse::<u64>()
            .unwrap(),
    );

    let mut p2p = p2p::P2P::new(node_urls, self_url)?;
    let my_id = p2p.peer_id;

    let p2p_channels = p2p.interface_channels();
    _ = p2p.prepare()?;

    tokio::spawn(async move {
        p2p.start_event_loop().await;
    });

    let mut app = App::new(my_id, p2p_channels)?;

    let (_shutdown_tx, shutdown_rx) = oneshot::channel();
    app.loop_until_shutdown(shutdown_rx).await;

    Ok(())
}
