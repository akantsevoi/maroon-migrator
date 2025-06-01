use maroon::app::Params;
use std::num::NonZeroUsize;
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

  let consensus_nodes = std::env::var("CONSENSUS_NODES")
    .unwrap_or_else(|_| "2".to_string())
    .parse::<usize>()
    .map(NonZeroUsize::new)
    .unwrap()
    .unwrap();

  let params = Params::default().set_consensus_nodes(consensus_nodes);

  let (_shutdown_tx, shutdown_rx) = oneshot::channel();

  let (mut app, _) = maroon::stack::create_stack(node_urls, self_url, params)?;
  app.loop_until_shutdown(shutdown_rx).await;

  Ok(())
}
