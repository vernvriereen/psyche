use crate::Client;
use psyche_core::{deterministic_shuffle, ClosedInterval, IntervalTree, NodeIdentity};

pub fn select_data_for_clients<I: NodeIdentity>(
    clients: &[Client<I>],
    start_index: u64,
    total_indicies: u64,
    random_seed: u64,
) -> IntervalTree<u64, I> {
    let mut ret = IntervalTree::new();
    let mut sum = start_index;
    let mut remaining = total_indicies;
    let mut client_shuffle = (0..clients.len()).collect::<Vec<_>>();
    deterministic_shuffle(&mut client_shuffle, random_seed);
    while remaining > 0 {
        for i in &client_shuffle {
            let client = &clients[*i];
            assert_ne!(client.num_data_indicies, 0);
            let num = (client.num_data_indicies as u64).min(remaining);
            ret.insert(ClosedInterval::new(sum, sum + num - 1), client.id.clone())
                .unwrap();
            sum += num;
            remaining -= num;
        }
    }
    ret
}
