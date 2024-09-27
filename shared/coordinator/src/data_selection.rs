use crate::{Committee, CommitteeSelection, Coordinator};
use psyche_core::{deterministic_shuffle, ClosedInterval, IntervalTree, NodeIdentity};

pub fn select_data_for_state<'a, T: NodeIdentity>(
    state: &Coordinator<T>,
    committee_selection: &CommitteeSelection,
) -> IntervalTree<u64, T> {
    let data_indicies_per_client = state.data_indicies_per_client as u64;
    let round = state.current_round().unwrap();
    let mut ret = IntervalTree::new();
    let mut sum = round.data_index;
    let mut remaining = state.data_indicies_per_round as u64;
    let mut client_shuffle = (0..state.clients.len())
        .map(|i| {
            (
                &state.clients[i],
                committee_selection.get_committee(i as u64).committee
            )
        })
        .collect::<Vec<_>>();
    deterministic_shuffle(&mut client_shuffle, round.random_seed);
    assert_eq!(
        state.data_indicies_per_round % state.data_indicies_per_client,
        0
    );
    let mut verifier_shuffle = (0
        ..(state.data_indicies_per_round / state.data_indicies_per_client) as u64)
        .collect::<Vec<_>>();
    deterministic_shuffle(&mut verifier_shuffle, round.random_seed);
    let mut first_pass = true;
    while remaining > 0 {
        for (client, committee) in &client_shuffle {
            match committee {
                Committee::TieBreaker => assert_eq!(round.tie_breaker_tasks, 0), // TODO
                Committee::Verifier => {
                    if first_pass {
                        match state.previous_round() {
                            Ok(Some(previous_round)) => {
                                let selected = verifier_shuffle.pop().unwrap();
                                let start = previous_round.data_index as u64
                                    + (selected * state.data_indicies_per_client as u64);
                                ret.insert(
                                    ClosedInterval::new(
                                        start,
                                        start + state.data_indicies_per_client as u64 - 1,
                                    ),
                                    client.id.clone(),
                                )
                                .unwrap();
                            }
                            _ => {}
                        }
                    }
                }
                Committee::Trainer => {
                    let num = data_indicies_per_client.min(remaining);
                    ret.insert(ClosedInterval::new(sum, sum + num - 1), client.id.clone())
                        .unwrap();
                    sum += num;
                    remaining -= num;
                }
            }
        }
        first_pass = false;
    }
    ret
}
