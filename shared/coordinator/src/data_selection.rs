use crate::Client;
use psyche_core::{Interval, IntervalTree, NodeIdentity};

pub fn select_data_for_clients<'a, I: NodeIdentity>(
    clients: &'a [Client<I>],
) -> IntervalTree<u64, &'a Client<I>> {
    let mut ret = IntervalTree::new();
    let mut sum = 0u64;
    for x in clients {
        ret.insert(Interval::new(sum, x.num_data_indicies as u64 - 1u64), x)
            .unwrap();
        sum += x.num_data_indicies as u64;
    }
    ret
}
