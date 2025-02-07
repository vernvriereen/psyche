use std::fmt;

use crate::{Committee, CommitteeSelection, Coordinator, Round};
use psyche_core::{deterministic_shuffle, BatchId, ClosedInterval, IntervalTree, NodeIdentity};

/// Assigns data batches to nodes based on committee roles.  
/// - `previous_round: true` reconstructs prior assignments for Healthchecks validation  
pub fn assign_data_for_state<T: NodeIdentity>(
    coordinator: &Coordinator<T>,
    previous_round: bool,
    committee_selection: &CommitteeSelection,
) -> IntervalTree<BatchId, T> {
    let data_indicies_per_client = coordinator.config.data_indicies_per_batch as u64;
    let round = if previous_round {
        coordinator.previous_round()
    } else {
        coordinator.current_round()
    }
    .unwrap();
    let mut ret = IntervalTree::new();
    let mut sum = round.data_index;
    let mut remaining =
        (coordinator.config.batches_per_round * coordinator.config.data_indicies_per_batch) as u64;
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

/// Retrieves all batch IDs assigned to a specific node from an interval tree, converting data indices to batches.
pub fn get_batch_ids_for_node<V: fmt::Display + Eq + std::hash::Hash>(
    tree: &IntervalTree<BatchId, V>,
    node_identity: &V,
    data_indicies_per_batch: u16,
) -> Vec<BatchId> {
    let batch_ids: Vec<BatchId> = tree
        .iter()
        .filter_map(|(interval, assigned_node)| {
            if assigned_node == node_identity {
                let start = u64::from(interval.start);
                let end = u64::from(interval.end);
                let start_batch = start / data_indicies_per_batch as u64;
                let end_batch = end / data_indicies_per_batch as u64;
                Some(start_batch..=end_batch)
            } else {
                None
            }
        })
        .flat_map(|range| range.map(BatchId::from_u64))
        .collect();

    batch_ids
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_batch_ids_for_node() {
        // Test empty tree
        let empty_tree = IntervalTree::new();
        assert!(get_batch_ids_for_node(&empty_tree, &"node_1", 1).is_empty());

        // Test single interval
        let mut tree = IntervalTree::new();
        tree.insert(
            ClosedInterval::new(BatchId::from_u64(1), BatchId::from_u64(3)),
            "node_1",
        )
        .unwrap();
        assert_eq!(
            get_batch_ids_for_node(&tree, &"node_1", 1),
            vec![1, 2, 3]
                .into_iter()
                .map(BatchId::from_u64)
                .collect::<Vec<_>>()
        );

        // Test multiple intervals for same node
        let mut tree = IntervalTree::new();
        tree.insert(
            ClosedInterval::new(BatchId::from_u64(1), BatchId::from_u64(2)),
            "node_1",
        )
        .unwrap();
        tree.insert(
            ClosedInterval::new(BatchId::from_u64(4), BatchId::from_u64(5)),
            "node_1",
        )
        .unwrap();
        assert_eq!(
            get_batch_ids_for_node(&tree, &"node_1", 1),
            vec![1, 2, 4, 5]
                .into_iter()
                .map(BatchId::from_u64)
                .collect::<Vec<_>>()
        );

        // Test node with no batches
        let mut tree = IntervalTree::new();
        tree.insert(
            ClosedInterval::new(BatchId::from_u64(1), BatchId::from_u64(3)),
            "node_1",
        )
        .unwrap();
        assert!(get_batch_ids_for_node(&tree, &"node_2", 1).is_empty());

        // Test multiple nodes
        let mut tree = IntervalTree::new();
        tree.insert(
            ClosedInterval::new(BatchId::from_u64(1), BatchId::from_u64(2)),
            "node_1",
        )
        .unwrap();
        tree.insert(
            ClosedInterval::new(BatchId::from_u64(3), BatchId::from_u64(4)),
            "node_2",
        )
        .unwrap();
        assert_eq!(
            get_batch_ids_for_node(&tree, &"node_1", 1),
            vec![1, 2]
                .into_iter()
                .map(BatchId::from_u64)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_get_batch_ids_for_node_unit_interval() {
        // Test single batch ID
        let mut tree = IntervalTree::new();
        tree.insert(
            ClosedInterval::new(BatchId::from_u64(5), BatchId::from_u64(5)),
            "node_1",
        )
        .unwrap();
        assert_eq!(
            get_batch_ids_for_node(&tree, &"node_1", 1),
            vec![5]
                .into_iter()
                .map(BatchId::from_u64)
                .collect::<Vec<_>>()
        );

        // Test non-consecutive intervals
        let mut tree = IntervalTree::new();
        tree.insert(
            ClosedInterval::new(BatchId::from_u64(1), BatchId::from_u64(1)),
            "node_1",
        )
        .unwrap();
        tree.insert(
            ClosedInterval::new(BatchId::from_u64(2), BatchId::from_u64(2)),
            "node_2",
        )
        .unwrap();
        tree.insert(
            ClosedInterval::new(BatchId::from_u64(3), BatchId::from_u64(3)),
            "node_1",
        )
        .unwrap();
        assert_eq!(
            get_batch_ids_for_node(&tree, &"node_1", 1),
            vec![1, 3]
                .into_iter()
                .map(BatchId::from_u64)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            get_batch_ids_for_node(&tree, &"node_2", 1),
            vec![2]
                .into_iter()
                .map(BatchId::from_u64)
                .collect::<Vec<_>>()
        );
    }
    #[test]
    fn test_get_batch_ids_for_node_with_data_indices_per_round() {
        // Test one node
        let mut tree = IntervalTree::new();
        tree.insert(
            ClosedInterval::new(BatchId::from_u64(5), BatchId::from_u64(5)),
            "node_1",
        )
        .unwrap();
        assert_eq!(
            get_batch_ids_for_node(&tree, &"node_1", 5),
            vec![1]
                .into_iter()
                .map(BatchId::from_u64)
                .collect::<Vec<_>>()
        );

        // Test two nodes
        let mut tree = IntervalTree::new();
        tree.insert(
            ClosedInterval::new(BatchId::from_u64(0), BatchId::from_u64(1)),
            "node_1",
        )
        .unwrap();
        tree.insert(
            ClosedInterval::new(BatchId::from_u64(2), BatchId::from_u64(3)),
            "node_2",
        )
        .unwrap();
        assert_eq!(
            get_batch_ids_for_node(&tree, &"node_1", 2),
            vec![0]
                .into_iter()
                .map(BatchId::from_u64)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            get_batch_ids_for_node(&tree, &"node_2", 2),
            vec![1]
                .into_iter()
                .map(BatchId::from_u64)
                .collect::<Vec<_>>()
        );
    }
}
