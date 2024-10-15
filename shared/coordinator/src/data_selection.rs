use crate::{Committee, CommitteeSelection, Coordinator};
use psyche_core::{deterministic_shuffle, ClosedInterval, IntervalTree, NodeIdentity};

pub fn assign_data_for_state<T: NodeIdentity>(
    state: &Coordinator<T>,
    committee_selection: &CommitteeSelection,
) -> IntervalTree<u64, T> {
    let data_indicies_per_client = state.data_indicies_per_batch as u64;
    let round = state.current_round().unwrap();
    let mut ret = IntervalTree::new();
    let mut sum = round.data_index;
    let mut remaining = (state.batches_per_round * state.data_indicies_per_batch) as u64;
    let mut client_shuffle = (0..state.clients.len())
        .map(|i| {
            (
                &state.clients[i],
                committee_selection.get_committee(i as u64).committee,
            )
        })
        .collect::<Vec<_>>();
    deterministic_shuffle(&mut client_shuffle, round.random_seed);
    assert_eq!(state.batches_per_round % state.data_indicies_per_batch, 0);
    let mut verifier_shuffle =
        (0..(state.batches_per_round / state.data_indicies_per_batch) as u64).collect::<Vec<_>>();
    deterministic_shuffle(&mut verifier_shuffle, round.random_seed);
    let mut first_pass = true;
    while remaining > 0 {
        for (client, committee) in &client_shuffle {
            match committee {
                Committee::TieBreaker => assert_eq!(round.tie_breaker_tasks, 0), // TODO
                Committee::Verifier => {
                    if first_pass {
                        if let Ok(Some(previous_round)) = state.previous_round() {
                            let selected = verifier_shuffle.pop().unwrap();
                            let start = previous_round.data_index
                                + (selected * state.data_indicies_per_batch as u64);
                            ret.insert(
                                ClosedInterval::new(
                                    start,
                                    start + state.data_indicies_per_batch as u64 - 1,
                                ),
                                client.id.clone(),
                            )
                            .unwrap();
                        }
                    }
                }
                Committee::Trainer => {
                    let num = data_indicies_per_client.min(remaining);
                    if num > 0 {
                        ret.insert(ClosedInterval::new(sum, sum + num - 1), client.id.clone())
                            .unwrap();
                        sum += num;
                        remaining -= num;
                    }
                }
            }
        }
        first_pass = false;
    }
    ret
}

pub fn get_batch_ids_for_state<T: NodeIdentity>(state: &Coordinator<T>) -> Vec<u64> {
    let round = match state.current_round() {
        Ok(round) => round,
        Err(_) => {
            return vec![];
        }
    };
    let batch_index = round.data_index / state.data_indicies_per_batch as u64;
    (batch_index..(batch_index + state.batches_per_round as u64)).collect::<Vec<_>>()
}
