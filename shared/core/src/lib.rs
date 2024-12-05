#![allow(unexpected_cfgs)]

mod batch_id;
mod bloom;
mod bounded_queue;
mod cancellable_barrier;
mod deterministic_shuffle;
mod interval_tree;
mod lcg;
mod lr_scheduler;
mod merkle_tree;
mod node_identity;
mod running_average;
mod sha256;
mod similarity;
mod swap_or_not;

pub use batch_id::BatchId;
pub use bloom::Bloom;
pub use bounded_queue::BoundedQueue;
pub use cancellable_barrier::{CancellableBarrier, CancelledBarrier};
pub use deterministic_shuffle::deterministic_shuffle;
pub use interval_tree::{ClosedInterval, IntervalTree};
pub use lcg::LCG;
pub use lr_scheduler::*;
pub use merkle_tree::{HashWrapper as RootType, MerkleTree, OwnedProof, Proof};
pub use node_identity::NodeIdentity;
pub use running_average::RunningAverage;
pub use sha256::{sha256, sha256v};
pub use similarity::{
    hamming_distance, is_similar, jaccard_distance, manhattan_distance, DistanceThresholds,
};
pub use swap_or_not::compute_shuffled_index;
