use crate::Client;
use psyche_core::{ClosedInterval, IntervalTree, NodeIdentity};

pub fn select_data_for_clients<I: NodeIdentity>(
    clients: &[Client<I>],
    start_index: u64,
    total_indicies: u64,
) -> IntervalTree<u64, I> {
    let mut ret = IntervalTree::new();
    let mut sum = start_index;
    let mut remaining = total_indicies;
    while remaining > 0 {
        for x in clients {
            assert_ne!(x.num_data_indicies, 0);
            let num = (x.num_data_indicies as u64).min(remaining);
            ret.insert(ClosedInterval::new(sum, sum + num - 1), x.id.clone())
                .unwrap();
            sum += num;
            remaining -= num;
        }
    }
    ret
}
