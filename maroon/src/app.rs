use crate::app_interface::{CurrentOffsets, Request, Response};
use crate::p2p_interface::{Inbox, NodeState, Outbox};
use common::invoker_handler::{HandlerInterface, RequestWrapper};
use common::{
    async_interface::ReqResPair,
    range_key::{self, KeyOffset, KeyRange, TransactionID},
    transaction::Transaction,
};
use libp2p::PeerId;
use log::{error, info};
use std::vec;
use std::{
    collections::{HashMap, HashSet},
    num::NonZeroUsize,
    time::Duration,
};
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::oneshot;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// how often node will send state info to other nodes
    pub advertise_period: std::time::Duration,
    /// minimum amount of nodes that should have the same transactions(+ current one) in order to confirm them
    /// TODO: separate pub struct ConsensusAlgoParams in a separate lib/consensus crate with its own test suite?
    pub consensus_nodes: NonZeroUsize,
}

impl Params {
    pub fn default() -> Params {
        Params {
            advertise_period: Duration::from_secs(5),
            consensus_nodes: NonZeroUsize::new(2).unwrap(),
        }
    }
}

pub struct App {
    params: Params,

    peer_id: PeerId,
    p2p_interface: ReqResPair<Outbox, Inbox>,
    state_interface: HandlerInterface<Request, Response>,

    /// offsets for the current node
    self_offsets: HashMap<KeyRange, KeyOffset>,

    /// offsets for all the nodes this one knows about(+ itself)
    offsets: HashMap<KeyRange, HashMap<PeerId, KeyOffset>>,

    /// consensus offset that is collected from currently running nodes
    /// it's not what is stored on s3 or etcd!!!
    ///
    /// what to do if some nodes are gone and new nodes don't have all the offsets yet? - download from s3
    consensus_offset: HashMap<KeyRange, KeyOffset>,

    transactions: HashMap<TransactionID, Transaction>,
}

impl App {
    pub fn new(
        peer_id: PeerId,
        p2p_interface: ReqResPair<Outbox, Inbox>,
        state_interface: HandlerInterface<Request, Response>,
        params: Params,
    ) -> Result<App, Box<dyn std::error::Error>> {
        Ok(App {
            params,
            peer_id,
            p2p_interface,
            state_interface,
            offsets: HashMap::new(),
            self_offsets: HashMap::new(),
            consensus_offset: HashMap::new(),
            transactions: HashMap::new(),
        })
    }

    /// starts a loop that processes events and executes logic
    pub async fn loop_until_shutdown(&mut self, mut shutdown: oneshot::Receiver<()>) {
        let mut ticker = tokio::time::interval(self.params.advertise_period);
        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    self.handle_on_tick();
                },
                Option::Some(req_wrapper) = self.state_interface.receiver.recv() => {
                    self.handle_request(req_wrapper);
                },
                Some(payload) = self.p2p_interface.receiver.recv() => {
                    self.handle_inbox_message(payload);
                },
                _ = &mut shutdown =>{
                    info!("TODO: shutdown the app");
                    break;
                }
            }
        }
    }

    fn recalculate_consensus_offsets(&mut self) {
        // TODO: Should I be worried that I might have some stale values in consensus_offset?
        for (k, v) in &self.offsets {
            if let Some(max) = consensus_maximum(&v, self.params.consensus_nodes) {
                self.consensus_offset.insert(*k, *max);
            }
        }

        let mut str = String::new();
        for (k, v) in &self.consensus_offset {
            str.push_str(&format!("\n{}: {}", k, v));
        }

        info!("consensus_offset:{}", str);
    }

    fn handle_inbox_message(&mut self, msg: Inbox) {
        match msg {
            Inbox::State((peer_id, state)) => {
                for (k, v) in state.offsets {
                    if let Some(in_map) = self.offsets.get_mut(&k) {
                        in_map.insert(peer_id, v);
                    } else {
                        self.offsets.insert(k, HashMap::from([(peer_id, v)]));
                    }
                }
            }
            Inbox::Nodes(nodes) => {
                recalculate_order(self.peer_id, &nodes);
            }
            Inbox::NewTransaction(tx) => {
                if let Some((new_range, new_offset)) =
                    update_self_offset(&mut self.self_offsets, &mut self.transactions, tx)
                {
                    move_offset_pointer(&mut self.offsets, self.peer_id, new_range, new_offset);
                }
            }
            Inbox::MissingTx(txs) => {
                for (new_range, new_offset) in update_self_offsets(
                    &mut self.self_offsets,
                    &mut self.transactions,
                    txs_to_range_tx_map(txs),
                ) {
                    move_offset_pointer(&mut self.offsets, self.peer_id, new_range, new_offset);
                }
            }
            Inbox::RequestMissingTxs((peer_id, ranges)) => {
                let mut capacity: usize = 0;
                for (left, right) in &ranges {
                    let lu = left.0 as usize;
                    let ru = right.0 as usize;
                    capacity += ru - lu;
                }

                let mut response: Vec<Transaction> = Vec::with_capacity(capacity);

                for (left, right) in ranges {
                    let mut pointer = left;
                    while pointer <= right {
                        let Some(tx) = self.transactions.get(&pointer) else {
                            continue;
                        };
                        response.push(tx.clone());
                        pointer += TransactionID(1);
                    }
                }

                self.p2p_interface
                    .sender
                    .send(Outbox::RequestedTxsForPeer((peer_id, response)))
                    .expect("TODO: shouldnt drop sender");
            }
        }
    }

    fn handle_on_tick(&mut self) {
        self.recalculate_consensus_offsets();
        if let Err(e) = self.p2p_interface.sender.send(Outbox::State(NodeState {
            offsets: self.self_offsets.clone(),
        })) {
            error!("main send: {e}");
        };
    }

    fn handle_request(&self, wrapper: RequestWrapper<Request, Response>) {
        match wrapper.request {
            Request::GetState => {
                if let Err(unsent_response) =
                    wrapper.response.send(Response::State(CurrentOffsets {
                        self_offsets: self.self_offsets.clone(),
                        consensus_offset: self.consensus_offset.clone(),
                    }))
                {
                    error!("couldnt send response: {unsent_response}");
                }
            }
        }
    }
}

fn move_offset_pointer(
    offsets: &mut HashMap<KeyRange, HashMap<PeerId, KeyOffset>>,
    peer_id: PeerId,
    new_range: KeyRange,
    new_offset: KeyOffset,
) {
    let Some(mut_range) = offsets.get_mut(&new_range) else {
        offsets.insert(new_range, HashMap::from([(peer_id, new_offset)]));
        return;
    };

    mut_range.insert(peer_id, new_offset);
}

fn update_self_offset(
    self_offsets: &mut HashMap<KeyRange, KeyOffset>,
    transactions: &mut HashMap<TransactionID, Transaction>,
    tx: Transaction,
) -> Option<(KeyRange, KeyOffset)> {
    let mut updates =
        update_self_offsets(self_offsets, transactions, txs_to_range_tx_map(vec![tx]));
    if updates.len() == 0 {
        None
    } else {
        updates.pop()
    }
}

fn update_self_offsets(
    self_offsets: &mut HashMap<KeyRange, KeyOffset>,
    transactions: &mut HashMap<TransactionID, Transaction>,
    range_transactions: HashMap<KeyRange, Vec<Transaction>>,
) -> Vec<(KeyRange, KeyOffset)> {
    let mut updates = Vec::<(KeyRange, KeyOffset)>::new();

    for (range, txs) in range_transactions {
        let mut has_0_tx = false;
        for tx in txs {
            let (_, offset) = range_key::range_offset_from_key(tx.id);
            transactions.insert(tx.id, tx);

            if offset == KeyOffset(0) {
                has_0_tx = true;
            }
        }

        let start = match self_offsets.get(&range) {
            Some(existing_offset) => existing_offset,
            None => {
                if has_0_tx {
                    self_offsets.insert(range, KeyOffset(0));
                } else {
                    continue;
                }
                &KeyOffset(0)
            }
        };

        let mut key = range_key::key_from_range_and_offset(range, *start);
        while transactions.contains_key(&(key + TransactionID(1))) {
            // TODO: there is an overflow error here. If one range is finished transaction can still be in the map, but offset will be above the maximum
            key += TransactionID(1);
        }

        let (_, new_offset) = range_key::range_offset_from_key(key);
        self_offsets.insert(range, new_offset);

        updates.push((range, new_offset));
    }

    updates
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

    info!(
        "My delay factor is {}! Nodes order: {:?}",
        delay_factor, peer_ids
    );
}

pub fn txs_to_range_tx_map(txs: Vec<Transaction>) -> HashMap<KeyRange, Vec<Transaction>> {
    let mut range_map: HashMap<KeyRange, Vec<Transaction>> = HashMap::new();
    for tx in txs {
        let range = range_key::range_index_by_key(tx.id);

        if let Some(bucket) = range_map.get_mut(&range) {
            bucket.push(tx);
        } else {
            range_map.insert(range, vec![tx]);
        }
    }

    return range_map;
}

#[cfg(test)]
mod tests {
    use std::hash::Hash;

    use crate::test_helpers::test_tx;

    use super::*;
    use common::{
        invoker_handler::create_invoker_handler_pair,
        transaction::{Transaction, TxStatus},
    };
    use tokio::{sync::mpsc, time};

    #[tokio::test(flavor = "multi_thread")]
    async fn app_process_message_and_shuts_down() {
        let (tx_in, rx_in) = mpsc::unbounded_channel::<Inbox>();
        let (tx_out, _rx_out) = mpsc::unbounded_channel::<Outbox>();
        let (shutdown_tx, shutdown_rx) = oneshot::channel();

        let n1_peer_id = PeerId::random();
        let n2_peer_id = PeerId::random();
        let (_, handler) = create_invoker_handler_pair();

        let mut app = crate::test_helpers::new_test_instance(
            ReqResPair {
                receiver: rx_in,
                sender: tx_out,
            },
            handler,
        );

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

    #[test]
    fn update_self_offset_test() {
        struct Case<'a> {
            label: &'a str,
            initial_self_offsets: HashMap<KeyRange, KeyOffset>,
            initial_transactions: HashMap<TransactionID, Transaction>,
            transaction: Transaction,
            expected_self_offsets: HashMap<KeyRange, KeyOffset>,
            expected_transactions: HashMap<TransactionID, Transaction>,
        }

        let cases = [
            Case {
                label: "empty",
                initial_self_offsets: HashMap::new(),
                initial_transactions: HashMap::new(),
                transaction: Transaction {
                    id: TransactionID(0),
                    status: TxStatus::Pending,
                },
                expected_self_offsets: HashMap::from([(KeyRange(0), KeyOffset(0))]),
                expected_transactions: HashMap::from([(
                    TransactionID(0),
                    Transaction {
                        id: TransactionID(0),
                        status: TxStatus::Pending,
                    },
                )]),
            },
            Case {
                label: "add already existing transaction. no effect",
                initial_self_offsets: HashMap::from([(KeyRange(0), KeyOffset(0))]),
                initial_transactions: HashMap::from([(
                    TransactionID(0),
                    Transaction {
                        id: TransactionID(0),
                        status: TxStatus::Pending,
                    },
                )]),
                transaction: Transaction {
                    id: TransactionID(0),
                    status: TxStatus::Pending,
                },
                expected_self_offsets: HashMap::from([(KeyRange(0), KeyOffset(0))]),
                expected_transactions: HashMap::from([(
                    TransactionID(0),
                    Transaction {
                        id: TransactionID(0),
                        status: TxStatus::Pending,
                    },
                )]),
            },
            Case {
                label: "add next transaction",
                initial_self_offsets: HashMap::from([(KeyRange(0), KeyOffset(0))]),
                initial_transactions: HashMap::from([(
                    TransactionID(0),
                    Transaction {
                        id: TransactionID(0),
                        status: TxStatus::Pending,
                    },
                )]),
                transaction: Transaction {
                    id: TransactionID(1),
                    status: TxStatus::Pending,
                },
                expected_self_offsets: HashMap::from([(KeyRange(0), KeyOffset(1))]),
                expected_transactions: HashMap::from([
                    (
                        TransactionID(0),
                        Transaction {
                            id: TransactionID(0),
                            status: TxStatus::Pending,
                        },
                    ),
                    (
                        TransactionID(1),
                        Transaction {
                            id: TransactionID(1),
                            status: TxStatus::Pending,
                        },
                    ),
                ]),
            },
            Case {
                label: "add transaction, fill the gap, empty initial offset",
                initial_self_offsets: HashMap::from([]),
                initial_transactions: HashMap::from([(
                    TransactionID(1),
                    Transaction {
                        id: TransactionID(1),
                        status: TxStatus::Pending,
                    },
                )]),
                transaction: Transaction {
                    id: TransactionID(0),
                    status: TxStatus::Pending,
                },
                expected_self_offsets: HashMap::from([(KeyRange(0), KeyOffset(1))]),
                expected_transactions: HashMap::from([
                    (
                        TransactionID(0),
                        Transaction {
                            id: TransactionID(0),
                            status: TxStatus::Pending,
                        },
                    ),
                    (
                        TransactionID(1),
                        Transaction {
                            id: TransactionID(1),
                            status: TxStatus::Pending,
                        },
                    ),
                ]),
            },
            Case {
                label: "add transaction, fill the gap, dont go till the end",
                initial_self_offsets: HashMap::from([(KeyRange(0), KeyOffset(0))]),
                initial_transactions: HashMap::from([
                    (
                        TransactionID(0),
                        Transaction {
                            id: TransactionID(0),
                            status: TxStatus::Pending,
                        },
                    ),
                    (
                        TransactionID(2),
                        Transaction {
                            id: TransactionID(2),
                            status: TxStatus::Pending,
                        },
                    ),
                    (
                        TransactionID(4),
                        Transaction {
                            id: TransactionID(4),
                            status: TxStatus::Pending,
                        },
                    ),
                ]),
                transaction: Transaction {
                    id: TransactionID(1),
                    status: TxStatus::Pending,
                },
                expected_self_offsets: HashMap::from([(KeyRange(0), KeyOffset(2))]),
                expected_transactions: HashMap::from([
                    (
                        TransactionID(0),
                        Transaction {
                            id: TransactionID(0),
                            status: TxStatus::Pending,
                        },
                    ),
                    (
                        TransactionID(1),
                        Transaction {
                            id: TransactionID(1),
                            status: TxStatus::Pending,
                        },
                    ),
                    (
                        TransactionID(2),
                        Transaction {
                            id: TransactionID(2),
                            status: TxStatus::Pending,
                        },
                    ),
                    (
                        TransactionID(4),
                        Transaction {
                            id: TransactionID(4),
                            status: TxStatus::Pending,
                        },
                    ),
                ]),
            },
            Case {
                label: "add transaction, fill the gap but not in the beginning",
                initial_self_offsets: HashMap::from([(KeyRange(0), KeyOffset(0))]),
                initial_transactions: HashMap::from([
                    (
                        TransactionID(0),
                        Transaction {
                            id: TransactionID(0),
                            status: TxStatus::Pending,
                        },
                    ),
                    (
                        TransactionID(2),
                        Transaction {
                            id: TransactionID(2),
                            status: TxStatus::Pending,
                        },
                    ),
                    (
                        TransactionID(4),
                        Transaction {
                            id: TransactionID(4),
                            status: TxStatus::Pending,
                        },
                    ),
                ]),
                transaction: Transaction {
                    id: TransactionID(3),
                    status: TxStatus::Pending,
                },
                expected_self_offsets: HashMap::from([(KeyRange(0), KeyOffset(0))]),
                expected_transactions: HashMap::from([
                    (
                        TransactionID(0),
                        Transaction {
                            id: TransactionID(0),
                            status: TxStatus::Pending,
                        },
                    ),
                    (
                        TransactionID(3),
                        Transaction {
                            id: TransactionID(3),
                            status: TxStatus::Pending,
                        },
                    ),
                    (
                        TransactionID(2),
                        Transaction {
                            id: TransactionID(2),
                            status: TxStatus::Pending,
                        },
                    ),
                    (
                        TransactionID(4),
                        Transaction {
                            id: TransactionID(4),
                            status: TxStatus::Pending,
                        },
                    ),
                ]),
            },
        ];

        for case in cases {
            let mut case = case;
            update_self_offset(
                &mut case.initial_self_offsets,
                &mut case.initial_transactions,
                case.transaction,
            );
            assert_eq!(
                case.expected_self_offsets, case.initial_self_offsets,
                "{}",
                case.label,
            );
            assert_eq!(
                case.expected_transactions, case.initial_transactions,
                "{}",
                case.label,
            );
        }
    }
}
