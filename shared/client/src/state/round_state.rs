use std::{collections::HashMap, sync::Arc};

use psyche_coordinator::{CommitteeProof, CommitteeSelection, Witness, WitnessBloom, WitnessProof};
use psyche_core::{BatchId, IntervalTree, NodeIdentity};
use tokio::sync::Mutex;
use tracing::{debug, info};

use crate::{fetch_data::BatchIdSet, TrainingResult};

use super::types::PayloadState;

pub struct RoundState<T: NodeIdentity> {
    pub height: u32,
    pub sent_witness: bool,
    pub downloads: HashMap<psyche_network::Hash, PayloadState<T>>,
    pub results: HashMap<BatchId, Vec<(T, TrainingResult)>>,
    pub commitments_per_client: HashMap<T, u32>,
    pub data_assignments: IntervalTree<BatchId, T>,
    pub blooms: Option<(WitnessBloom, WitnessBloom)>,
    pub committee_info: Option<(CommitteeProof, WitnessProof, CommitteeSelection)>,
    pub batch_ids_not_yet_trained_on: Option<(usize, Arc<Mutex<BatchIdSet>>)>,
}

impl<T: NodeIdentity> RoundState<T> {
    pub fn new() -> Self {
        Self {
            height: 0,
            sent_witness: false,
            downloads: HashMap::new(),
            results: HashMap::new(),
            commitments_per_client: HashMap::new(),
            data_assignments: IntervalTree::new(),
            blooms: None,
            committee_info: None,
            batch_ids_not_yet_trained_on: None,
        }
    }
}

impl<T: NodeIdentity> Default for RoundState<T> {
    fn default() -> Self {
        RoundState::new()
    }
}

impl<T: NodeIdentity> RoundState<T> {
    pub fn get_witness_to_send(&mut self, index: u64) -> Option<Witness> {
        if self.sent_witness {
            return None;
        }

        let (_, witness_proof, _) = self.committee_info.as_ref()?;

        if !witness_proof.witness {
            return None;
        }

        let blooms = self.blooms;
        let (participant_bloom, order_bloom) = blooms?;

        info!("Submitting witness blooms");
        self.sent_witness = true;

        debug!("Participant bloom: {:?}", participant_bloom);
        debug!("Order bloom: {:?}", order_bloom);

        Some(Witness {
            index,
            proof: *witness_proof,
            participant_bloom,
            order_bloom,
        })
    }
}
