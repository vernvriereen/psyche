#![allow(unexpected_cfgs)]

mod batch_id;
mod bloom;
mod bounded_queue;
mod cancellable_barrier;
mod deterministic_shuffle;
mod fixed_vec;
mod interval_tree;
mod lcg;
mod lr_scheduler;
mod merkle_tree;
mod node_identity;
mod running_average;
mod serde_utils;
mod sha256;
mod similarity;
mod swap_or_not;

pub use batch_id::BatchId;
pub use bloom::Bloom;
pub use bounded_queue::BoundedQueue;
pub use cancellable_barrier::{CancellableBarrier, CancelledBarrier};
pub use deterministic_shuffle::deterministic_shuffle;
pub use fixed_vec::FixedVec;
pub use interval_tree::{ClosedInterval, IntervalTree};
pub use lcg::LCG;
pub use lr_scheduler::*;
pub use merkle_tree::{HashWrapper as RootType, MerkleTree, OwnedProof, Proof};
pub use node_identity::NodeIdentity;
pub use running_average::RunningAverage;
pub use serde_utils::{
    serde_deserialize_optional_string, serde_deserialize_string, serde_deserialize_vec_to_array,
    serde_serialize_array_as_vec, serde_serialize_optional_string, serde_serialize_string,
};
pub use sha256::{sha256, sha256v};
pub use similarity::{
    hamming_distance, is_similar, jaccard_distance, manhattan_distance, DistanceThresholds,
};
pub use swap_or_not::compute_shuffled_index;

pub fn u8_to_string(slice: &[u8]) -> String {
    String::from_utf8_lossy(slice)
        .trim_matches(char::from(0))
        .to_string()
}
