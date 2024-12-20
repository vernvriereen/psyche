use crate::{Committee, CommitteeSelection, Coordinator, Round};
use psyche_core::{deterministic_shuffle, BatchId, ClosedInterval, IntervalTree, NodeIdentity};

pub fn assign_data_for_state<T: NodeIdentity>(
    coordinator: &Coordinator<T>,
    committee_selection: &CommitteeSelection,
) -> IntervalTree<BatchId, T> {
    let data_indicies_per_client = coordinator.config.data_indicies_per_batch as u64;
    let round = coordinator.current_round().unwrap();
    let mut ret = IntervalTree::new();
    let mut sum = round.data_index;
    let mut remaining = (coordinator.config.batches_per_round * coordinator.config.data_indicies_per_batch) as u64;
    let mut client_shuffle = (0..coordinator.epoch_state.clients.len())
        .map(|i| {
            (
                &coordinator.epoch_state.clients[i],
                committee_selection.get_committee(i as u64).committee,
            )
        })
        .collect::<Vec<_>>();
    deterministic_shuffle(&mut client_shuffle, round.random_seed);
    //assert_eq!(coordinator.batches_per_round % coordinator.data_indicies_per_batch, 0);
    // let mut verifier_shuffle =
    //     (0..(coordinator.batches_per_round / coordinator.data_indicies_per_batch) as u64).collect::<Vec<_>>();
    // deterministic_shuffle(&mut verifier_shuffle, round.random_seed);
    // let mut first_pass = true;
    while remaining > 0 {
        for (client, committee) in &client_shuffle {
            match committee {
                Committee::TieBreaker => assert_eq!(round.tie_breaker_tasks, 0), // TODO
                Committee::Verifier => {
                    // if first_pass {
                    //     if let Some(previous_round) = coordinator.previous_round() {
                    //         let selected = verifier_shuffle.pop().unwrap();
                    //         let start = previous_round.data_index
                    //             + (selected * coordinator.data_indicies_per_batch as u64);
                    //         ret.insert(
                    //             ClosedInterval::new(
                    //                 start,
                    //                 start + coordinator.data_indicies_per_batch as u64 - 1,
                    //             ),
                    //             client.id.clone(),
                    //         )
                    //         .unwrap();
                    //     }
                    // }
                }
                Committee::Trainer => {
                    let num = data_indicies_per_client.min(remaining);
                    if num > 0 {
                        ret.insert(
                            ClosedInterval::new(
                                BatchId::from_u64(sum),
                                BatchId::from_u64(sum + num - 1),
                            ),
                            client.id,
                        )
                        .unwrap();
                        sum += num;
                        remaining -= num;
                    }
                }
            }
        }
        // first_pass = false;
    }
    ret
}

pub fn get_batch_ids_for_round<T: NodeIdentity>(
    round: &Round,
    coordinator: &Coordinator<T>,
) -> Vec<BatchId> {
    let batch_index = round.data_index / coordinator.config.data_indicies_per_batch as u64;
    (batch_index..(batch_index + coordinator.config.batches_per_round as u64))
        .map(BatchId::from_u64)
        .collect::<Vec<_>>()
}
