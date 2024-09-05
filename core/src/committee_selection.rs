
pub struct CommitteeSelection {
    tie_breaking_tasks: u32,
    verification_percent: u8,
    total_nodes: u32,
    seed: u64,
}

impl CommitteeSelection {
    fn threshold(world_size: u64, committee_size: u64) -> u64 {
        (u64::MAX / world_size) * committee_size
    }
}