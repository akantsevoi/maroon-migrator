use super::{
  interface::{CurrentOffsets, Request, Response},
  params::Params,
};
use crate::{
  epoch::Epoch,
  linearizer::{Linearizer, LogLineriazer},
  network::{Inbox, NodeState, Outbox},
};
use common::{
  duplex_channel::Endpoint,
  invoker_handler::{HandlerInterface, RequestWrapper},
  range_key::{
    self, KeyOffset, KeyRange, U64BlobIdClosedInterval, UniqueU64BlobId,
    range_offset_from_unique_blob_id, unique_blob_id_from_range_and_offset,
  },
  transaction::Transaction,
};
use libp2p::PeerId;
use log::{debug, error, info};
use std::{
  collections::{HashMap, HashSet},
  num::NonZeroUsize,
  vec,
};
use tokio::{
  sync::oneshot,
  time::{MissedTickBehavior, interval},
};

pub struct App<L: Linearizer> {
  params: Params,

  peer_id: PeerId,

  p2p_interface: Endpoint<Outbox, Inbox>,
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

  /// describes which offsets have been already commited and => where the current epoch starts
  /// can be recalculated from `epochs`
  commited_offsets: HashMap<KeyRange, KeyOffset>,

  /// TODO: at some point it will be pointless to store all the epochs in the variable on the node, keep that in mind
  /// all epochs
  /// it's what will be stored on etcd & s3
  epochs: Vec<Epoch>,

  transactions: HashMap<UniqueU64BlobId, Transaction>,

  linearizer: L,
}

impl<L: Linearizer> App<L> {
  pub fn new(
    peer_id: PeerId,
    p2p_interface: Endpoint<Outbox, Inbox>,
    state_interface: HandlerInterface<Request, Response>,
    params: Params,
  ) -> Result<App<LogLineriazer>, Box<dyn std::error::Error>> {
    Ok(App {
      params,
      peer_id,
      p2p_interface,
      state_interface,
      offsets: HashMap::new(),
      self_offsets: HashMap::new(),
      consensus_offset: HashMap::new(),
      commited_offsets: HashMap::new(),
      epochs: Vec::new(),
      transactions: HashMap::new(),
      linearizer: LogLineriazer::new(),
    })
  }

  /// starts a loop that processes events and executes logic
  pub async fn loop_until_shutdown(&mut self, mut shutdown: oneshot::Receiver<()>) {
    let mut advertise_offset_ticker = interval(self.params.advertise_period);
    advertise_offset_ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut commit_epoch_ticker = interval(self.params.epoch_period);

    loop {
      tokio::select! {
          _ = advertise_offset_ticker.tick() => {
            self.advertise_offsets_and_request_missing();
          },
          _ = commit_epoch_ticker.tick() => {
            self.commit_epoch_if_needed();
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
      Inbox::RequestMissingTxs((peer_id, intervals)) => {
        let mut capacity: usize = 0;
        for interval in &intervals {
          capacity += interval.ids_count();
        }

        let mut response: Vec<Transaction> = Vec::with_capacity(capacity);

        for interval in intervals {
          let mut pointer = interval.start();
          let end = interval.end();
          while pointer <= end {
            let Some(tx) = self.transactions.get(&pointer) else {
              continue;
            };
            response.push(tx.clone());
            pointer += UniqueU64BlobId(1);
          }
        }

        debug!("send_back_missing_txs to peerID:[{}]", peer_id);
        self
          .p2p_interface
          .sender
          .send(Outbox::RequestedTxsForPeer((peer_id, response)))
          .expect("TODO: shouldnt drop sender");
      }
    }
  }

  fn advertise_offsets_and_request_missing(&mut self) {
    self.recalculate_consensus_offsets();
    debug!("broadcast_self_state: {:?}", self.self_offsets);
    self.p2p_interface.send(Outbox::State(NodeState {
      offsets: self.self_offsets.clone(),
    }));

    let delays = self_delays(&self.transactions, &self.self_offsets, &self.offsets);
    if delays.len() == 0 {
      return;
    }

    info!("delay detected: {:?}", delays);

    for (peer_id, intervals) in delays {
      self
        .p2p_interface
        .sender
        .send(Outbox::RequestMissingTxs((peer_id, intervals)))
        .expect("dont drop channel");
    }
  }

  fn handle_request(&self, wrapper: RequestWrapper<Request, Response>) {
    match wrapper.request {
      Request::GetState => {
        if let Err(unsent_response) = wrapper.response.send(Response::State(CurrentOffsets {
          self_offsets: self.self_offsets.clone(),
          consensus_offset: self.consensus_offset.clone(),
        })) {
          error!("couldnt send response: {unsent_response}");
        }
      }
    }
  }

  fn commit_epoch_if_needed(&mut self) {
    let increments = calculate_epoch_increments(&self.consensus_offset, &self.commited_offsets);

    let prev_hash = self.epochs.last().map(|e| e.hash);
    let new_epoch = Epoch::new(increments, prev_hash);
    info!("NEW EPOCH: {}", &new_epoch);

    self.linearizer.new_epoch(new_epoch.clone());

    {
      // TODO: in the future the code below won't exist
      // it won't be pushed here immediately into the local variables, it will be pushed to etcd
      // and if there is a success it will be returned back to the node and added to epochs
      // maybe there will be some "optimistic" epochs chain for some form of optimisations, I don't know, let's see
      // maybe all these local variables wouldn't make any sense

      for interval in &new_epoch.increments {
        let (range, new_offset) = range_offset_from_unique_blob_id(interval.end());
        self.commited_offsets.insert(range, new_offset);
      }
      self.epochs.push(new_epoch);
    }
  }
}

fn calculate_epoch_increments(
  consensus_offset: &HashMap<KeyRange, KeyOffset>,
  commited_offsets: &HashMap<KeyRange, KeyOffset>,
) -> Vec<U64BlobIdClosedInterval> {
  let mut increments = Vec::new();
  for (range, offset) in consensus_offset {
    let mut start = KeyOffset(0);
    if let Some(prev) = commited_offsets.get(&range) {
      start = *prev + KeyOffset(1);
    }

    if start >= *offset {
      continue;
    }

    increments.push(U64BlobIdClosedInterval::new_from_range_and_offsets(
      *range, start, *offset,
    ));
  }

  increments
}

/// moves offset pointer for a particular peerID(node)
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

/// wrapper around `update_self_offsets`
fn update_self_offset(
  self_offsets: &mut HashMap<KeyRange, KeyOffset>,
  transactions: &mut HashMap<UniqueU64BlobId, Transaction>,
  tx: Transaction,
) -> Option<(KeyRange, KeyOffset)> {
  let mut updates = update_self_offsets(self_offsets, transactions, txs_to_range_tx_map(vec![tx]));
  if updates.len() == 0 {
    None
  } else {
    updates.pop()
  }
}

/// inserts transactions, updates self_offset pointers if should
/// returns changed offsets if there are any
fn update_self_offsets(
  self_offsets: &mut HashMap<KeyRange, KeyOffset>,
  transactions: &mut HashMap<UniqueU64BlobId, Transaction>,
  range_transactions: HashMap<KeyRange, Vec<Transaction>>,
) -> Vec<(KeyRange, KeyOffset)> {
  let mut updates = Vec::<(KeyRange, KeyOffset)>::new();

  for (range, txs) in range_transactions {
    let mut has_0_tx = false;
    for tx in txs {
      let (_, offset) = range_key::range_offset_from_unique_blob_id(tx.id);
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

    let mut key = range_key::unique_blob_id_from_range_and_offset(range, *start);
    while transactions.contains_key(&(key + UniqueU64BlobId(1))) {
      // TODO: there is an overflow error here. If one range is finished transaction can still be in the map, but offset will be above the maximum
      key += UniqueU64BlobId(1);
    }

    let (_, new_offset) = range_key::range_offset_from_unique_blob_id(key);
    self_offsets.insert(range, new_offset);

    updates.push((range, new_offset));
  }

  debug!("my_current_offset: {:?}", &self_offsets);

  updates
}

/// calculates delays that current node (self_delays) has compare to other nodes `offsets`
/// also uses transactions in order to reduce amount of requested transactions
fn self_delays(
  transactions: &HashMap<UniqueU64BlobId, Transaction>,
  self_offsets: &HashMap<KeyRange, KeyOffset>,
  offsets: &HashMap<KeyRange, HashMap<PeerId, KeyOffset>>,
) -> HashMap<PeerId, Vec<U64BlobIdClosedInterval>> {
  let mut result = HashMap::<PeerId, Vec<U64BlobIdClosedInterval>>::new();

  for (range, nodes) in offsets {
    let Some((peer_id, offset)) = nodes
      .iter()
      .max_by_key(|(_peer, v)| **v)
      .map(|(peer, &offset)| (peer.clone(), offset))
    else {
      continue;
    };

    let right_border = offset.clone();
    let mut left_border = KeyOffset(0);
    if let Some(current) = self_offsets.get(&range) {
      if *current >= right_border {
        continue;
      } else {
        left_border = *current + KeyOffset(1);
      }
    };

    let mut new_intervals = Vec::<U64BlobIdClosedInterval>::new();

    let mut pointer = left_border;
    while pointer <= right_border {
      if transactions.contains_key(&unique_blob_id_from_range_and_offset(*range, pointer)) {
        if left_border < pointer {
          new_intervals.push(U64BlobIdClosedInterval::new_from_range_and_offsets(
            *range,
            left_border,
            pointer - KeyOffset(1),
          ));
        }
        pointer += KeyOffset(1);
        left_border = pointer;
      } else {
        pointer += KeyOffset(1);
      }
    }
    if pointer != left_border {
      new_intervals.push(U64BlobIdClosedInterval::new_from_range_and_offsets(
        *range,
        left_border,
        pointer - KeyOffset(1),
      ));
    }

    if let Some(ranges) = result.get_mut(&peer_id) {
      ranges.append(&mut new_intervals);
    } else {
      result.insert(peer_id.clone(), new_intervals);
    }
  }

  result
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

fn txs_to_range_tx_map(txs: Vec<Transaction>) -> HashMap<KeyRange, Vec<Transaction>> {
  let mut range_map: HashMap<KeyRange, Vec<Transaction>> = HashMap::new();
  for tx in txs {
    let range = range_key::range_from_unique_blob_id(tx.id);

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
  use crate::test_helpers::test_tx;

  use super::*;
  use common::transaction::{Transaction, TxStatus};

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
      initial_transactions: HashMap<UniqueU64BlobId, Transaction>,
      transaction: Transaction,
      expected_self_offsets: HashMap<KeyRange, KeyOffset>,
      expected_transactions: HashMap<UniqueU64BlobId, Transaction>,
    }

    let cases = [
      Case {
        label: "empty",
        initial_self_offsets: HashMap::new(),
        initial_transactions: HashMap::new(),
        transaction: Transaction {
          id: UniqueU64BlobId(0),
          status: TxStatus::Pending,
        },
        expected_self_offsets: HashMap::from([(KeyRange(0), KeyOffset(0))]),
        expected_transactions: HashMap::from([(
          UniqueU64BlobId(0),
          Transaction {
            id: UniqueU64BlobId(0),
            status: TxStatus::Pending,
          },
        )]),
      },
      Case {
        label: "add already existing transaction. no effect",
        initial_self_offsets: HashMap::from([(KeyRange(0), KeyOffset(0))]),
        initial_transactions: HashMap::from([(
          UniqueU64BlobId(0),
          Transaction {
            id: UniqueU64BlobId(0),
            status: TxStatus::Pending,
          },
        )]),
        transaction: Transaction {
          id: UniqueU64BlobId(0),
          status: TxStatus::Pending,
        },
        expected_self_offsets: HashMap::from([(KeyRange(0), KeyOffset(0))]),
        expected_transactions: HashMap::from([(
          UniqueU64BlobId(0),
          Transaction {
            id: UniqueU64BlobId(0),
            status: TxStatus::Pending,
          },
        )]),
      },
      Case {
        label: "add next transaction",
        initial_self_offsets: HashMap::from([(KeyRange(0), KeyOffset(0))]),
        initial_transactions: HashMap::from([(
          UniqueU64BlobId(0),
          Transaction {
            id: UniqueU64BlobId(0),
            status: TxStatus::Pending,
          },
        )]),
        transaction: Transaction {
          id: UniqueU64BlobId(1),
          status: TxStatus::Pending,
        },
        expected_self_offsets: HashMap::from([(KeyRange(0), KeyOffset(1))]),
        expected_transactions: HashMap::from([
          (
            UniqueU64BlobId(0),
            Transaction {
              id: UniqueU64BlobId(0),
              status: TxStatus::Pending,
            },
          ),
          (
            UniqueU64BlobId(1),
            Transaction {
              id: UniqueU64BlobId(1),
              status: TxStatus::Pending,
            },
          ),
        ]),
      },
      Case {
        label: "add transaction, fill the gap, empty initial offset",
        initial_self_offsets: HashMap::from([]),
        initial_transactions: HashMap::from([(
          UniqueU64BlobId(1),
          Transaction {
            id: UniqueU64BlobId(1),
            status: TxStatus::Pending,
          },
        )]),
        transaction: Transaction {
          id: UniqueU64BlobId(0),
          status: TxStatus::Pending,
        },
        expected_self_offsets: HashMap::from([(KeyRange(0), KeyOffset(1))]),
        expected_transactions: HashMap::from([
          (
            UniqueU64BlobId(0),
            Transaction {
              id: UniqueU64BlobId(0),
              status: TxStatus::Pending,
            },
          ),
          (
            UniqueU64BlobId(1),
            Transaction {
              id: UniqueU64BlobId(1),
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
            UniqueU64BlobId(0),
            Transaction {
              id: UniqueU64BlobId(0),
              status: TxStatus::Pending,
            },
          ),
          (
            UniqueU64BlobId(2),
            Transaction {
              id: UniqueU64BlobId(2),
              status: TxStatus::Pending,
            },
          ),
          (
            UniqueU64BlobId(4),
            Transaction {
              id: UniqueU64BlobId(4),
              status: TxStatus::Pending,
            },
          ),
        ]),
        transaction: Transaction {
          id: UniqueU64BlobId(1),
          status: TxStatus::Pending,
        },
        expected_self_offsets: HashMap::from([(KeyRange(0), KeyOffset(2))]),
        expected_transactions: HashMap::from([
          (
            UniqueU64BlobId(0),
            Transaction {
              id: UniqueU64BlobId(0),
              status: TxStatus::Pending,
            },
          ),
          (
            UniqueU64BlobId(1),
            Transaction {
              id: UniqueU64BlobId(1),
              status: TxStatus::Pending,
            },
          ),
          (
            UniqueU64BlobId(2),
            Transaction {
              id: UniqueU64BlobId(2),
              status: TxStatus::Pending,
            },
          ),
          (
            UniqueU64BlobId(4),
            Transaction {
              id: UniqueU64BlobId(4),
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
            UniqueU64BlobId(0),
            Transaction {
              id: UniqueU64BlobId(0),
              status: TxStatus::Pending,
            },
          ),
          (
            UniqueU64BlobId(2),
            Transaction {
              id: UniqueU64BlobId(2),
              status: TxStatus::Pending,
            },
          ),
          (
            UniqueU64BlobId(4),
            Transaction {
              id: UniqueU64BlobId(4),
              status: TxStatus::Pending,
            },
          ),
        ]),
        transaction: Transaction {
          id: UniqueU64BlobId(3),
          status: TxStatus::Pending,
        },
        expected_self_offsets: HashMap::from([(KeyRange(0), KeyOffset(0))]),
        expected_transactions: HashMap::from([
          (
            UniqueU64BlobId(0),
            Transaction {
              id: UniqueU64BlobId(0),
              status: TxStatus::Pending,
            },
          ),
          (
            UniqueU64BlobId(3),
            Transaction {
              id: UniqueU64BlobId(3),
              status: TxStatus::Pending,
            },
          ),
          (
            UniqueU64BlobId(2),
            Transaction {
              id: UniqueU64BlobId(2),
              status: TxStatus::Pending,
            },
          ),
          (
            UniqueU64BlobId(4),
            Transaction {
              id: UniqueU64BlobId(4),
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

  #[test]
  fn test_self_delays_calculation() {
    struct Case<'a> {
      label: &'a str,
      self_offsets: HashMap<KeyRange, KeyOffset>,
      transactions: HashMap<UniqueU64BlobId, Transaction>,
      offsets: HashMap<KeyRange, HashMap<PeerId, KeyOffset>>,
      expected_ranges: HashMap<PeerId, Vec<U64BlobIdClosedInterval>>,
    }

    let peer_id_0 = PeerId::random();
    let peer_id_1 = PeerId::random();

    for case in vec![
      Case {
        label: "empty everything",
        self_offsets: HashMap::new(),
        transactions: HashMap::new(),
        offsets: HashMap::new(),
        expected_ranges: HashMap::new(),
      },
      Case {
        label: "self in front",
        self_offsets: HashMap::from([(KeyRange(0), KeyOffset(5)), (KeyRange(2), KeyOffset(3))]),
        transactions: HashMap::new(),
        offsets: HashMap::from([
          (
            KeyRange(0),
            HashMap::from([(peer_id_0, KeyOffset(5)), (peer_id_1, KeyOffset(2))]),
          ),
          (
            KeyRange(2),
            HashMap::from([(peer_id_0, KeyOffset(1)), (peer_id_1, KeyOffset(2))]),
          ),
        ]),
        expected_ranges: HashMap::new(),
      },
      Case {
        label: "self behind few and few gaps in txs",
        self_offsets: HashMap::from([(KeyRange(0), KeyOffset(1)), (KeyRange(2), KeyOffset(1))]),
        transactions: HashMap::from([
          (UniqueU64BlobId(5), test_tx(5)),
          (UniqueU64BlobId(6), test_tx(6)),
        ]),
        offsets: HashMap::from([
          (
            KeyRange(0),
            HashMap::from([(peer_id_0, KeyOffset(8)), (peer_id_1, KeyOffset(2))]),
          ),
          (
            KeyRange(2),
            HashMap::from([(peer_id_0, KeyOffset(3)), (peer_id_1, KeyOffset(1))]),
          ),
          (
            KeyRange(3),
            HashMap::from([(peer_id_0, KeyOffset(1)), (peer_id_1, KeyOffset(3))]),
          ),
        ]),
        expected_ranges: HashMap::from([
          (
            peer_id_0,
            vec![
              U64BlobIdClosedInterval::new(2, 4),
              U64BlobIdClosedInterval::new(7, 8),
              U64BlobIdClosedInterval::new_from_range_and_offsets(
                KeyRange(2),
                KeyOffset(2),
                KeyOffset(3),
              ),
            ],
          ),
          (
            peer_id_1,
            vec![U64BlobIdClosedInterval::new_from_range_and_offsets(
              KeyRange(3),
              KeyOffset(0),
              KeyOffset(3),
            )],
          ),
        ]),
      },
    ] {
      let mut ranges = self_delays(&case.transactions, &case.self_offsets, &case.offsets);
      for (_, v) in ranges.iter_mut() {
        v.sort();
      }
      assert_eq!(case.expected_ranges, ranges, "{}", case.label);
    }
  }

  #[test]
  fn test_calculate_epoch_increments() {
    struct Case<'a> {
      label: &'a str,
      consensus_offset: HashMap<KeyRange, KeyOffset>,
      commited_offsets: HashMap<KeyRange, KeyOffset>,
      expected_increments: Vec<U64BlobIdClosedInterval>,
    }

    for case in vec![
      Case {
        label: "empty everything",
        consensus_offset: HashMap::new(),
        commited_offsets: HashMap::new(),
        expected_increments: vec![],
      },
      Case {
        label: "nothing commited before",
        consensus_offset: [(KeyRange(0), KeyOffset(2))].into(),
        commited_offsets: [].into(),
        expected_increments: vec![U64BlobIdClosedInterval::new(0, 2)],
      },
      Case {
        label: "no progress",
        consensus_offset: [(KeyRange(0), KeyOffset(2))].into(),
        commited_offsets: [(KeyRange(0), KeyOffset(2))].into(),
        expected_increments: vec![],
      },
      Case {
        label: "consensus delayed by any reasons",
        consensus_offset: [(KeyRange(0), KeyOffset(1))].into(),
        commited_offsets: [(KeyRange(0), KeyOffset(2))].into(),
        expected_increments: vec![],
      },
      Case {
        label: "progress in some",
        consensus_offset: [
          (KeyRange(0), KeyOffset(10)),
          (KeyRange(1), KeyOffset(2)),
          (KeyRange(2), KeyOffset(6)),
        ]
        .into(),
        commited_offsets: [
          (KeyRange(0), KeyOffset(6)),
          (KeyRange(1), KeyOffset(2)),
          (KeyRange(2), KeyOffset(8)),
        ]
        .into(),
        expected_increments: vec![U64BlobIdClosedInterval::new(7, 10)],
      },
    ] {
      let mut increments =
        calculate_epoch_increments(&case.consensus_offset, &case.commited_offsets);
      increments.sort();
      assert_eq!(case.expected_increments, increments, "{}", case.label);
    }
  }
}
