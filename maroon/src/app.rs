use crate::p2p_interface::{Inbox, KeyOffset, KeyRange, NodeState, Outbox, P2PChannels};
use libp2p::PeerId;
use std::{
    collections::{HashMap, HashSet},
    num::NonZeroUsize,
    time::Duration,
};
use tokio::sync::oneshot;

pub struct App {
    /// minimum amount of nodes that should have the same transactions(+ current one) in order to confirm them
    consensus_nodes: NonZeroUsize,
    peer_id: PeerId,
    channels: P2PChannels,

    /// offsets for the current node
    self_offsets: HashMap<KeyRange, KeyOffset>,

    /// offsets for all the nodes this one knows about(+ itself)
    offsets: HashMap<KeyRange, HashMap<PeerId, KeyOffset>>,

    /// consensus offset that is collected from currently running nodes
    /// it's not what is stored on s3 or etcd!!!
    ///
    /// what to do if some nodes are gone and new nodes don't have all the offsets yet? - download from s3
    consensus_offset: HashMap<KeyRange, KeyOffset>,
}

impl App {
    pub fn new(peer_id: PeerId, channels: P2PChannels) -> Result<App, Box<dyn std::error::Error>> {
        Ok(App {
            consensus_nodes: NonZeroUsize::new(2).unwrap(), // TODO: move to constructor+env variables
            peer_id,
            channels,
            offsets: HashMap::new(),
            self_offsets: HashMap::new(),
            consensus_offset: HashMap::new(),
        })
    }

    /// starts a loop that processes events and executes logic
    pub async fn loop_until_shutdown(&mut self, mut shutdown: oneshot::Receiver<()>) {
        let mut ticker = tokio::time::interval(Duration::from_secs(5));

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    if let Err(e)=self.channels.sender.send( Outbox::State(NodeState{offsets: self.self_offsets.clone()})) {
                        println!("main send: {e}");
                        continue;
                    };
                },
                Some(payload) = self.channels.receiver.recv() =>  {
                    match payload {
                        Inbox::State((peer_id, state))=>{
                            for (k,v) in state.offsets {
                                if let Some(in_map) = self.offsets.get_mut(&k) {
                                    in_map.insert(peer_id, v);
                                } else{
                                    self.offsets.insert(k, HashMap::from([(peer_id, v)]));
                                }
                            }
                            self.recalculate_consensus_offsets();
                        },
                        Inbox::Nodes(nodes)=>{
                            recalculate_order(self.peer_id ,&nodes);
                        },
                    }
                },
                _ = &mut shutdown =>{
                    println!("shutdown the app");
                    break;
                }
            }
        }
    }

    fn recalculate_consensus_offsets(&mut self) {
        // TODO: Should I be worried that I might have some stale values in consensus_offset?
        for (k, v) in &self.offsets {
            if let Some(max) = consensus_maximum(&v, self.consensus_nodes) {
                self.consensus_offset.insert(*k, *max);
            }
        }
    }
}

/// returns maximum offset among peers keeping in mind the `n_consensus`
/// if `n_consensus` is 2 - it will find the maximum number that is present in at least 2 peers
fn consensus_maximum(
    map: &HashMap<PeerId, KeyOffset>,
    n_consensus: NonZeroUsize,
) -> Option<&KeyOffset> {
    let n = n_consensus.get();
    if map.len() < n {
        return None;
    }

    let mut refs: Vec<&KeyOffset> = map.values().collect();
    refs.sort_unstable_by(|a, b| b.cmp(a));

    Some(refs[n - 1])
}

fn recalculate_order(self_id: PeerId, ids: &HashSet<PeerId>) {
    let mut peer_ids: Vec<&PeerId> = ids.iter().collect();

    peer_ids.sort();
    let delay_factor = peer_ids
        .iter()
        .position(|e| **e == self_id)
        .expect("self peer id should exist here. Otherwise it's stupid");

    println!(
        "My delay factor is {}! Nodes order: {:?}",
        delay_factor, peer_ids
    );
}

#[cfg(test)]
impl App {
    pub fn new_test_instance(channels: P2PChannels) -> App {
        App::new(PeerId::random(), channels).expect("failed to create test App instance")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::{sync::mpsc, time};

    #[tokio::test(flavor = "multi_thread")]
    async fn app_process_message_and_shuts_down() {
        let (tx_in, rx_in) = mpsc::unbounded_channel::<Inbox>();
        let (tx_out, _rx_out) = mpsc::unbounded_channel::<Outbox>();
        let (shutdown_tx, shutdown_rx) = oneshot::channel();

        let n1_peer_id = PeerId::random();
        let n2_peer_id = PeerId::random();

        let mut app = App::new_test_instance(P2PChannels {
            receiver: rx_in,
            sender: tx_out,
        });

        let handle = tokio::spawn(async move {
            app.loop_until_shutdown(shutdown_rx).await;
            app.consensus_offset
        });

        _ = tx_in
            .send(Inbox::State((
                n1_peer_id,
                NodeState {
                    offsets: HashMap::from([
                        (KeyRange(1), KeyOffset(3)),
                        (KeyRange(2), KeyOffset(7)),
                        (KeyRange(4), KeyOffset(1)),
                    ]),
                },
            )))
            .expect("no errors");
        _ = tx_in
            .send(Inbox::State((
                n2_peer_id,
                NodeState {
                    offsets: HashMap::from([
                        (KeyRange(1), KeyOffset(2)),
                        (KeyRange(2), KeyOffset(9)),
                    ]),
                },
            )))
            .expect("no errors");

        // wait until app consumes the event
        time::sleep(Duration::from_millis(10)).await;
        shutdown_tx.send(()).expect("no error on shutdown");

        let calculated_offset = handle.await.expect("should get proper map");

        assert_eq!(
            calculated_offset,
            HashMap::from([(KeyRange(1), KeyOffset(2)), (KeyRange(2), KeyOffset(7))])
        )
    }

    #[test]
    fn calculate_consensus_maximum() {
        let p_id_1 = PeerId::random();
        let p_id_2 = PeerId::random();
        let p_id_3 = PeerId::random();

        struct Case {
            map: Vec<(PeerId, KeyOffset)>,
            want: Option<KeyOffset>,
        }

        let cases = [
            Case {
                map: vec![],
                want: None,
            },
            Case {
                map: vec![(p_id_1, KeyOffset(10))],
                want: None,
            },
            Case {
                map: vec![
                    (p_id_1, KeyOffset(10)),
                    (p_id_2, KeyOffset(2)),
                    (p_id_3, KeyOffset(4)),
                ],
                want: Some(KeyOffset(4)),
            },
        ];

        for (i, case) in cases.iter().enumerate() {
            let hm: HashMap<_, _> = case.map.clone().into_iter().collect();
            assert_eq!(
                consensus_maximum(&hm, NonZeroUsize::new(2).unwrap()).copied(),
                case.want,
                "case #{} failed: {:?} → {:?}",
                i,
                case.map,
                case.want
            );
        }
    }
}
