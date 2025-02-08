use crate::{
    assign_data_for_state,
    data_selection::get_batch_ids_for_node,
    model::{self, Checkpoint, HubRepo, Model},
    Committee, CommitteeProof, CommitteeSelection, WitnessProof,
};

use anchor_lang::{prelude::borsh, AnchorDeserialize, AnchorSerialize, InitSpace};
use bytemuck::{Pod, Zeroable};
use psyche_core::{
    serde_deserialize_string, serde_serialize_string, sha256, BatchId, Bloom, FixedVec,
    NodeIdentity, SmallBoolean,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{collections::HashSet, hash::Hash};

pub const SOLANA_MAX_STRING_LEN: usize = 64;
pub const SOLANA_MAX_URL_STRING_LEN: usize = 192;
pub const SOLANA_MAX_NUM_CLIENTS: usize = 64;
pub const SOLANA_MAX_NUM_WITNESSES: usize = 16;
pub const SOLANA_MAX_NUM_CHECKPOINTERS: usize = 4;

pub const BLOOM_FALSE_RATE: f64 = 0.01f64;

// bloom filter with 1024 bits (16 u64)
pub type WitnessBloom = Bloom<16, 8>;

#[repr(u8)]
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Zeroable,
    AnchorDeserialize,
    AnchorSerialize,
    Serialize,
    Deserialize,
    InitSpace,
)]
pub enum RunState {
    #[default]
    Uninitialized = 0,
    WaitingForMembers = 1,
    Warmup = 2,
    RoundTrain = 3,
    RoundWitness = 4,
    Cooldown = 5,
    Finished = 6,
    Paused = 7,
}

#[repr(u8)]
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Zeroable,
    AnchorDeserialize,
    AnchorSerialize,
    Serialize,
    Deserialize,
    InitSpace,
)]
pub enum ClientState {
    #[default]
    Healthy = 0,
    Dropped = 1,
    Withdrawn = 2,
    Ejected = 3,
}

#[derive(
    Clone,
    Debug,
    Zeroable,
    Default,
    Copy,
    Serialize,
    Deserialize,
    AnchorDeserialize,
    AnchorSerialize,
)]
#[serde(bound = "I: Serialize + DeserializeOwned + NodeIdentity")]
pub struct Client<I> {
    pub id: I,
    pub state: ClientState,
    pub exited_height: u32,
}

impl<I: NodeIdentity> Hash for Client<I> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

#[derive(Clone, Default, Debug, Zeroable, Copy, Serialize, Deserialize)]
#[repr(C)]
pub struct Round {
    pub witnesses: FixedVec<Witness, SOLANA_MAX_NUM_WITNESSES>,
    pub data_index: u64,
    pub random_seed: u64,
    pub height: u32,
    pub clients_len: u16,
    pub tie_breaker_tasks: u16,
}

#[derive(
    Clone,
    Debug,
    Zeroable,
    Default,
    Copy,
    AnchorDeserialize,
    AnchorSerialize,
    Serialize,
    Deserialize,
)]
#[repr(C)]
pub struct Witness {
    pub proof: WitnessProof,
    pub participant_bloom: WitnessBloom,
    pub order_bloom: WitnessBloom,
}

#[derive(Clone, Copy, Debug)]
pub enum CoordinatorError {
    NoActiveRound,
    InvalidWitness,
    InvalidRunState,
    DuplicateWitness,
    InvalidHealthCheck,
    Halted,
    InvalidCheckpoint,
    WitnessesFull,
    CannotResume,
    InvalidWithdraw,
    InvalidCommitteeSelection,
}

pub enum TickResult {
    Ticked,
    EpochEnd(bool), // if successfully finished
}

pub type Commitment = [u8; 32];
pub type HealthChecks = Vec<CommitteeProof>;

pub const NUM_STORED_ROUNDS: usize = 4;

#[derive(
    Clone, Debug, Zeroable, Copy, Serialize, Deserialize, AnchorDeserialize, AnchorSerialize,
)]
#[repr(C)]
#[serde(bound = "I: DeserializeOwned + NodeIdentity")]
pub struct CoordinatorConfig<I> {
    pub warmup_time: u64,
    pub cooldown_time: u64,

    pub max_round_train_time: u64,
    pub round_witness_time: u64,

    pub min_clients: u16,

    pub batches_per_round: u16,
    pub data_indicies_per_batch: u16,

    pub verification_percent: u8,
    pub witness_nodes: u16,
    pub witness_quorum: u16,

    pub rounds_per_epoch: u32,
    pub total_steps: u32,

    pub overlapped: SmallBoolean,

    // TODO: remove when we implement parameter sharing over p2p
    #[serde(default)]
    pub checkpointers: FixedVec<I, SOLANA_MAX_NUM_CHECKPOINTERS>,
}

#[derive(Clone, Debug, Zeroable, Copy, Serialize, Deserialize)]
#[repr(C)]
#[serde(bound = "T: DeserializeOwned + NodeIdentity")]
pub struct CoordinatorEpochState<T> {
    pub rounds: [Round; NUM_STORED_ROUNDS],
    pub clients: FixedVec<Client<T>, SOLANA_MAX_NUM_CLIENTS>,
    pub exited_clients: FixedVec<Client<T>, SOLANA_MAX_NUM_CLIENTS>,
    pub rounds_head: u32,
    pub first_round: SmallBoolean,
    pub checkpointed: SmallBoolean,
    pub pause: SmallBoolean,
}

#[derive(Clone, Debug, Zeroable, Copy, Serialize, Deserialize)]
#[repr(C)]
pub struct CoordinatorProgress {
    pub epoch: u16,
    pub step: u32,
    pub epoch_start_data_index: u64,
}

#[derive(Clone, Debug, Zeroable, Copy, Serialize, Deserialize)]
#[serde(bound = "T: DeserializeOwned + NodeIdentity")]
#[repr(C)]
pub struct Coordinator<T> {
    #[serde(
        serialize_with = "serde_serialize_string",
        deserialize_with = "serde_deserialize_string"
    )]
    pub run_id: [u8; SOLANA_MAX_STRING_LEN],

    pub run_state: RunState,

    pub model: Model,

    pub config: CoordinatorConfig<T>,

    #[serde(default)]
    pub progress: CoordinatorProgress,
    #[serde(default)]
    pub prev_epoch_progress: CoordinatorProgress,

    #[serde(default)]
    pub epoch_state: CoordinatorEpochState<T>,

    #[serde(default)]
    pub run_state_start_unix_timestamp: u64,
    #[serde(default)]
    pub tick: u64,
    #[serde(default)]
    pub last_tick_unix_timestamp: u64,
    #[serde(default)]
    pub last_step_unix_timestamp: u64,
}

unsafe impl<T: NodeIdentity + Zeroable> Pod for Coordinator<T> {}

impl TryFrom<usize> for RunState {
    type Error = CoordinatorError;

    fn try_from(value: usize) -> std::result::Result<Self, Self::Error> {
        match value {
            0 => Ok(RunState::Uninitialized),
            1 => Ok(RunState::WaitingForMembers),
            2 => Ok(RunState::Warmup),
            3 => Ok(RunState::RoundTrain),
            4 => Ok(RunState::RoundWitness),
            5 => Ok(RunState::Cooldown),
            6 => Ok(RunState::Finished),
            7 => Ok(RunState::Paused),
            _ => Err(CoordinatorError::InvalidRunState),
        }
    }
}

impl From<RunState> for usize {
    fn from(val: RunState) -> Self {
        match val {
            RunState::Uninitialized => 0,
            RunState::WaitingForMembers => 1,
            RunState::Warmup => 2,
            RunState::RoundTrain => 3,
            RunState::RoundWitness => 4,
            RunState::Cooldown => 5,
            RunState::Finished => 6,
            RunState::Paused => 7,
        }
    }
}

impl<T: NodeIdentity> AsRef<[u8]> for Client<T> {
    fn as_ref(&self) -> &[u8] {
        self.id.as_ref()
    }
}

impl<T: NodeIdentity> PartialEq for Client<T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<T: NodeIdentity> Eq for Client<T> {}

impl std::fmt::Display for CoordinatorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CoordinatorError::NoActiveRound => write!(f, "No active round"),
            CoordinatorError::InvalidWitness => write!(f, "Invalid witness"),
            CoordinatorError::InvalidRunState => write!(f, "Invalid run state"),
            CoordinatorError::DuplicateWitness => write!(f, "Duplicate witness"),
            CoordinatorError::InvalidHealthCheck => write!(f, "Invalid health check"),
            CoordinatorError::Halted => write!(f, "Halted"),
            CoordinatorError::InvalidCheckpoint => write!(f, "Invalid checkpoint"),
            CoordinatorError::WitnessesFull => write!(f, "Witnesses full"),
            CoordinatorError::CannotResume => write!(f, "Cannot resume"),
            CoordinatorError::InvalidWithdraw => write!(f, "Invalid withdraw"),
            CoordinatorError::InvalidCommitteeSelection => write!(f, "Invalid committee selection"),
        }
    }
}

impl std::error::Error for CoordinatorError {}

impl std::fmt::Display for RunState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunState::Uninitialized => write!(f, "Uninitialized"),
            RunState::WaitingForMembers => write!(f, "Waiting for members"),
            RunState::Warmup => write!(f, "Warmup"),
            RunState::RoundTrain => write!(f, "Training"),
            RunState::RoundWitness => write!(f, "Witness"),
            RunState::Cooldown => write!(f, "Cooldown"),
            RunState::Finished => write!(f, "Finished"),
            RunState::Paused => write!(f, "Paused"),
        }
    }
}

impl<T: NodeIdentity> Default for CoordinatorEpochState<T> {
    fn default() -> Self {
        Self {
            rounds: Default::default(),
            rounds_head: Default::default(),
            first_round: true.into(),
            pause: Default::default(),
            checkpointed: Default::default(),
            clients: Default::default(),
            exited_clients: Default::default(),
        }
    }
}

impl Default for CoordinatorProgress {
    fn default() -> Self {
        Self {
            epoch: Default::default(),
            step: 1,
            epoch_start_data_index: Default::default(),
        }
    }
}

impl<T: NodeIdentity> Client<T> {
    pub fn new(id: T) -> Self {
        Self {
            id,
            state: ClientState::Healthy,
            exited_height: 0,
        }
    }
}

impl<T: NodeIdentity> Coordinator<T> {
    pub fn tick<'a, 'b>(
        &'a mut self,
        new_clients: Option<impl ExactSizeIterator<Item = &'b T>>,
        unix_timestamp: u64,
        random_seed: u64,
    ) -> std::result::Result<TickResult, CoordinatorError> {
        let ret = match self.run_state {
            RunState::Uninitialized | RunState::Finished | RunState::Paused => {
                Err(CoordinatorError::Halted)
            }
            run_state => {
                if self.epoch_state.pause.into() {
                    self.epoch_state.pause = false.into();
                    self.change_state(unix_timestamp, RunState::Paused);
                    Ok(TickResult::EpochEnd(false))
                } else if run_state == RunState::WaitingForMembers {
                    self.tick_waiting_for_members(new_clients, unix_timestamp)
                } else if run_state == RunState::Cooldown {
                    self.tick_cooldown(unix_timestamp)
                } else {
                    match run_state {
                        RunState::Warmup => self.tick_warmup(unix_timestamp, random_seed),
                        RunState::RoundTrain => self.tick_round_train(unix_timestamp),
                        RunState::RoundWitness => {
                            self.tick_round_witness(unix_timestamp, random_seed)
                        }
                        _ => unreachable!(),
                    }
                }
            }
        }?;
        self.tick += 1;
        self.last_tick_unix_timestamp = unix_timestamp;
        Ok(ret)
    }

    pub fn witness(
        &mut self,
        from: &T,
        witness: Witness,
        unix_timestamp: u64,
    ) -> std::result::Result<(), CoordinatorError> {
        if self.halted() {
            return Err(CoordinatorError::Halted);
        }
        if !CommitteeSelection::from_coordinator(self, false)?.verify_witness_for_client::<T>(
            from,
            &witness.proof,
            &self.epoch_state.clients,
        ) || !witness.proof.witness
        {
            return Err(CoordinatorError::InvalidWitness);
        }

        if !matches!(
            self.run_state,
            RunState::RoundWitness | RunState::RoundTrain,
        ) {
            return Err(CoordinatorError::InvalidRunState);
        }

        let witness_nodes = if self.config.witness_nodes == 0 {
            self.epoch_state.clients.len()
        } else {
            self.config.witness_nodes as usize
        };

        let round = self.current_round().unwrap();
        for witness in round.witnesses.iter() {
            if self.epoch_state.clients[witness.proof.index as usize].id == *from {
                return Err(CoordinatorError::DuplicateWitness);
            }
        }
        let round = self.current_round_mut_unchecked();
        round
            .witnesses
            .push(witness)
            .map_err(|_| CoordinatorError::WitnessesFull)?;

        if round.witnesses.len() == witness_nodes && !(self.run_state == RunState::RoundWitness) {
            self.change_state(unix_timestamp, RunState::RoundWitness);
        }
        Ok(())
    }

    pub fn health_check(
        &mut self,
        _from: &T,
        checks: HealthChecks,
    ) -> std::result::Result<u32, CoordinatorError> {
        if self.halted() {
            return Err(CoordinatorError::Halted);
        }
        for proof in &checks {
            if self.healthy(proof) {
                return Err(CoordinatorError::InvalidHealthCheck);
            }
        }
        let mut dropped = 0;
        for proof in &checks {
            let index = proof.index as usize;
            let client = &mut self.epoch_state.clients[index];
            if client.state == ClientState::Healthy {
                client.state = ClientState::Dropped;
                dropped += 1;
            }
        }
        // todo: reward `from` for `dropped` health checks
        Ok(dropped)
    }

    pub fn checkpoint(
        &mut self,
        from: &T,
        hub_repo: HubRepo,
    ) -> std::result::Result<(), CoordinatorError> {
        if self.epoch_state.checkpointed.is_false()
            && self.config.checkpointers.iter().any(|x| x == from)
        {
            // TODO: In the case of more than one checkpointer, this will overwrite the hub repo
            // with the last checkpointed one. We could instead have a vector of hub repos to have
            // more download options.
            match &mut self.model {
                Model::LLM(llm) => match llm.checkpoint {
                    Checkpoint::P2P(_) => llm.checkpoint = Checkpoint::P2P(hub_repo),
                    Checkpoint::Hub(_) => llm.checkpoint = Checkpoint::Hub(hub_repo),
                    _ => {}
                },
            }
            self.epoch_state.checkpointed = true.into();
            Ok(())
        } else {
            Err(CoordinatorError::InvalidCheckpoint)
        }
    }

    pub fn withdraw(&mut self, index: u64) -> std::result::Result<(), CoordinatorError> {
        let index = index as usize;
        if index < self.epoch_state.clients.len() {
            let client = &mut self.epoch_state.clients[index];
            if client.state == ClientState::Healthy {
                client.state = ClientState::Withdrawn;
                return Ok(());
            }
        }
        Err(CoordinatorError::InvalidWithdraw)
    }

    pub fn pause(&mut self) -> std::result::Result<(), CoordinatorError> {
        self.epoch_state.pause = true.into();
        Ok(())
    }

    pub fn resume(&mut self, unix_timestamp: u64) -> std::result::Result<(), CoordinatorError> {
        if self.run_state != RunState::Paused {
            return Err(CoordinatorError::CannotResume);
        }
        // resume from previous epoch's progress
        self.progress = self.prev_epoch_progress;
        self.start_waiting_for_members(unix_timestamp);
        Ok(())
    }

    pub fn healthy(&self, proof: &CommitteeProof) -> bool {
        let round = match self.previous_round() {
            Some(round) => round,
            None => {
                return true;
            }
        };
        let index = proof.index;
        if index < round.clients_len as u64 {
            let client =
                match self.get_client_at_historical_index(index as usize, round.clients_len) {
                    Some(client) => client,
                    None => {
                        return false;
                    }
                };
            let selection = match CommitteeSelection::from_coordinator(self, false) {
                Ok(selection) => selection,
                Err(_) => {
                    return false;
                }
            };
            if !selection.verify_committee_for_client(&client.id, proof, &self.epoch_state.clients)
            {
                return false;
            }
            match proof.committee {
                Committee::TieBreaker => todo!(),
                Committee::Verifier => todo!(),
                Committee::Trainer => Self::trainer_healthy_by_witnesses(
                    self,
                    &client.id,
                    &round.witnesses,
                    self.config.witness_quorum,
                ),
            }
        } else {
            false
        }
    }

    pub fn trainer_healthy_by_witnesses(
        coordinator: &Self,
        id: &T,
        witnesses: &[Witness],
        witness_quorum: u16,
    ) -> bool {
        let prev_round_committee_selection =
            CommitteeSelection::from_coordinator(coordinator, true).unwrap();
        let prev_round_data_assignments =
            assign_data_for_state(coordinator, true, &prev_round_committee_selection);

        let batch_ids = get_batch_ids_for_node(
            &prev_round_data_assignments,
            id,
            coordinator.config.data_indicies_per_batch,
        );

        let score = Self::trainer_healthy_score_by_witnesses(&batch_ids, id, witnesses);
        match witness_quorum {
            0 => score as usize == witnesses.len() * batch_ids.len(),
            witness_quorum => score >= witness_quorum * batch_ids.len() as u16,
        }
    }

    /// Calculates a trainer's health score based on witness confirmations.
    /// Counts how many witnesses confirmed each of the trainer's batches.
    /// Final score = 1 point per witness confirmation per batch)
    pub fn trainer_healthy_score_by_witnesses(
        batch_ids: &[BatchId],
        id: &T,
        witnesses: &[Witness],
    ) -> u16 {
        let mut commitments = Vec::new();
        for batch_id in batch_ids {
            let mut committment = Vec::with_capacity(40);
            committment.extend_from_slice(id.as_ref());
            committment.extend_from_slice(&u64::from(*batch_id).to_be_bytes());
            let committment_hash = sha256(&committment);

            commitments.push(committment_hash);
        }

        let mut score = 0u16;
        for witness in witnesses {
            for commitment in &commitments {
                let commitment_hash = sha256(commitment.as_ref());
                if witness.order_bloom.contains(&commitment_hash) {
                    score += 1;
                }
            }
        }

        score
    }

    pub fn select_consensus_commitment_by_witnesses(
        commitments: &[Commitment],
        witnesses: &[Witness],
        witness_quorum: u16,
    ) -> Option<usize> {
        let mut scores = vec![0; commitments.len()];
        for witness in witnesses {
            for (index, commitment) in commitments.iter().enumerate() {
                if witness.order_bloom.contains(&sha256(commitment)) {
                    scores[index] += 1;
                    break;
                }
            }
        }
        scores
            .into_iter()
            .enumerate()
            .filter(|(_, score)| match witness_quorum {
                0 => *score as usize == witnesses.len(),
                witness_quorum => *score >= witness_quorum,
            })
            .max_by_key(|(_, score)| *score)
            .map(|(index, _)| index)
    }

    pub fn current_round(&self) -> Option<&Round> {
        self.epoch_state
            .rounds
            .get(self.epoch_state.rounds_head as usize)
    }

    pub fn current_round_mut(&mut self) -> Option<&mut Round> {
        self.epoch_state
            .rounds
            .get_mut(self.epoch_state.rounds_head as usize)
    }

    pub fn current_round_unchecked(&self) -> &Round {
        &self.epoch_state.rounds[self.epoch_state.rounds_head as usize]
    }

    pub fn current_round_mut_unchecked(&mut self) -> &mut Round {
        &mut self.epoch_state.rounds[self.epoch_state.rounds_head as usize]
    }

    pub fn previous_round(&self) -> Option<&Round> {
        match self.current_round() {
            Some(round) => match self.epoch_state.rounds_head == 0 && round.height == 0 {
                true => None,
                false => match self.epoch_state.rounds_head == 0 {
                    true => Some(&self.epoch_state.rounds[NUM_STORED_ROUNDS - 1]),
                    false => {
                        Some(&self.epoch_state.rounds[self.epoch_state.rounds_head as usize - 1])
                    }
                },
            },
            None => None,
        }
    }

    pub fn previous_previous_round(&self) -> Option<&Round> {
        match self.current_round() {
            Some(round) => match self.epoch_state.rounds_head == 0 && round.height <= 1 {
                true => None,
                false => match self.epoch_state.rounds_head {
                    0 => Some(&self.epoch_state.rounds[NUM_STORED_ROUNDS - 2]),
                    1 => Some(&self.epoch_state.rounds[NUM_STORED_ROUNDS - 1]),
                    n => Some(&self.epoch_state.rounds[n as usize - 2]),
                },
            },
            None => None,
        }
    }

    pub fn active(&self) -> bool {
        !self.halted()
            && !matches!(
                self.run_state,
                RunState::WaitingForMembers | RunState::Warmup
            )
    }

    pub fn halted(&self) -> bool {
        matches!(
            self.run_state,
            RunState::Uninitialized | RunState::Finished | RunState::Paused
        )
    }

    pub fn get_client_at_historical_index(
        &self,
        n: usize,
        prev_clients_len: u16,
    ) -> Option<&Client<T>> {
        if n < self.epoch_state.clients.len() {
            Some(&self.epoch_state.clients[n])
        } else if n < prev_clients_len as usize {
            let offset: usize = prev_clients_len as usize - n - 1;
            self.epoch_state.exited_clients.iter().rev().nth(offset)
        } else {
            None
        }
    }

    pub fn get_historical_clients(&self, clients_len: u16) -> Vec<&Client<T>> {
        (0..clients_len)
            .filter_map(|i| self.get_client_at_historical_index(i as usize, clients_len))
            .collect()
    }

    fn tick_waiting_for_members<'a, 'b>(
        &'a mut self,
        pending_clients: Option<impl ExactSizeIterator<Item = &'b T>>,
        unix_timestamp: u64,
    ) -> std::result::Result<TickResult, CoordinatorError> {
        let Some(mut pending_clients) = pending_clients else {
            return Ok(TickResult::Ticked);
        };

        if pending_clients.len() as u16 >= self.config.min_clients {
            let current_round = self.current_round_unchecked();
            let height = current_round.height;
            self.move_clients_to_exited(height);
            let mut next_round_clients = self.epoch_state.clients;
            let prev_epoch_client_ids: HashSet<_> = self
                .epoch_state
                .clients
                .into_iter()
                .map(|client| client.id)
                .collect();

            for client_id in pending_clients.by_ref() {
                if !prev_epoch_client_ids.contains(client_id) {
                    next_round_clients.push(Client::new(*client_id)).unwrap();
                }
            }

            let Model::LLM(llm) = &mut self.model;
            if self.epoch_state.clients.is_empty() {
                if let Checkpoint::P2P(hub_repo) = llm.checkpoint {
                    llm.checkpoint = Checkpoint::Hub(hub_repo);
                }
            } else if self.progress.epoch != 0 {
                if let Checkpoint::Hub(hub_repo) = llm.checkpoint {
                    llm.checkpoint = Checkpoint::P2P(hub_repo)
                }
            }

            bytemuck::write_zeroes(&mut self.epoch_state);
            self.epoch_state.first_round = true.into();
            self.epoch_state.clients = next_round_clients;
            self.start_warmup(unix_timestamp);
        }

        Ok(TickResult::Ticked)
    }

    fn tick_warmup(
        &mut self,
        unix_timestamp: u64,
        random_seed: u64,
    ) -> std::result::Result<TickResult, CoordinatorError> {
        if self.check_timeout(unix_timestamp, self.config.warmup_time) {
            self.start_round_train(unix_timestamp, random_seed, 0);
        } else {
            self.move_clients_to_exited(0);
        }
        if (self.epoch_state.clients.len() as u16) < self.config.min_clients {
            self.start_waiting_for_members(unix_timestamp);
            Ok(TickResult::EpochEnd(false))
        } else {
            Ok(TickResult::Ticked)
        }
    }

    fn tick_round_train(
        &mut self,
        unix_timestamp: u64,
    ) -> std::result::Result<TickResult, CoordinatorError> {
        if self.check_timeout(unix_timestamp, self.config.max_round_train_time) {
            self.change_state(unix_timestamp, RunState::RoundWitness);
        }
        Ok(TickResult::Ticked)
    }

    fn tick_round_witness(
        &mut self,
        unix_timestamp: u64,
        random_seed: u64,
    ) -> std::result::Result<TickResult, CoordinatorError> {
        if self.check_timeout(unix_timestamp, self.config.round_witness_time) {
            // TODO: Punish idle witnesses
            self.epoch_state.first_round = false.into();
            self.progress.step += 1;

            let current_round = self.current_round_unchecked();
            let height = current_round.height;
            let num_witnesses = current_round.witnesses.len() as u16;
            self.move_clients_to_exited(height);

            // if we finish an epoch or some clients disconnect and we don't
            // reach the minimum number of clients we change state to cooldown.
            if height == self.config.rounds_per_epoch - 1
                || self.epoch_state.clients.len() < self.config.min_clients as usize
                || num_witnesses == 0
                || (num_witnesses < self.config.witness_quorum)
            {
                self.start_cooldown(unix_timestamp);
            } else {
                self.start_round_train(unix_timestamp, random_seed, 0);
            }
        }
        Ok(TickResult::Ticked)
    }

    fn tick_cooldown(
        &mut self,
        unix_timestamp: u64,
    ) -> std::result::Result<TickResult, CoordinatorError> {
        // cooldown_time == 0 means we never automatically advance to the next epoch,
        // so the only way to get there is through the checkpointing code.
        // this forces everything to wait on a valid checkpoint
        if self.epoch_state.checkpointed.into()
            || (self.config.cooldown_time > 0
                && self.check_timeout(unix_timestamp, self.config.cooldown_time))
        {
            self.prev_epoch_progress = self.progress;
            self.progress.epoch_start_data_index = Self::get_next_round_data_index(
                self.current_round_unchecked().data_index,
                self.config.batches_per_round,
                self.config.data_indicies_per_batch,
            );
            self.progress.epoch += 1;
            let current_round = self.current_round_unchecked();
            let height = current_round.height;
            self.move_clients_to_exited(height);
            self.start_waiting_for_members(unix_timestamp);
            Ok(TickResult::EpochEnd(true))
        } else {
            Ok(TickResult::Ticked)
        }
    }

    fn check_timeout(&self, unix_timestamp: u64, duration: u64) -> bool {
        self.run_state_start_unix_timestamp != unix_timestamp
            && unix_timestamp >= duration + self.run_state_start_unix_timestamp
    }

    fn start_cooldown(&mut self, unix_timestamp: u64) {
        self.change_state(unix_timestamp, RunState::Cooldown);
    }

    fn start_round_train(&mut self, unix_timestamp: u64, random_seed: u64, tie_breaker_tasks: u16) {
        let (next_rounds_head, next_height, next_data_index) =
            if self.epoch_state.first_round.into() {
                // very first round, don't increment -- just start here
                (0usize, 0u32, self.progress.epoch_start_data_index)
            } else {
                let current_round = &self.epoch_state.rounds[self.epoch_state.rounds_head as usize];
                (
                    (self.epoch_state.rounds_head + 1) as usize % self.epoch_state.rounds.len(),
                    current_round.height + 1,
                    Self::get_next_round_data_index(
                        current_round.data_index,
                        self.config.batches_per_round,
                        self.config.data_indicies_per_batch,
                    ),
                )
            };
        let round = &mut self.epoch_state.rounds[next_rounds_head];
        self.epoch_state.rounds_head = next_rounds_head as u32;
        round.clients_len = self.epoch_state.clients.len() as u16;
        round.height = next_height;
        round.data_index = next_data_index;
        round.tie_breaker_tasks = tie_breaker_tasks;
        round.random_seed = random_seed;
        round.witnesses.clear();
        self.change_state(unix_timestamp, RunState::RoundTrain);
    }

    fn start_warmup(&mut self, unix_timestamp: u64) {
        self.change_state(unix_timestamp, RunState::Warmup);
    }

    fn start_waiting_for_members(&mut self, unix_timestamp: u64) {
        self.change_state(
            unix_timestamp,
            if self.progress.step < self.config.total_steps {
                RunState::WaitingForMembers
            } else {
                RunState::Finished
            },
        );
    }

    fn change_state(&mut self, unix_timestamp: u64, new_state: RunState) {
        assert!(self.run_state != new_state);
        self.run_state_start_unix_timestamp = unix_timestamp;
        self.run_state = new_state;
    }

    fn get_next_round_data_index(
        data_index: u64,
        batches_per_round: u16,
        data_indicies_per_batch: u16,
    ) -> u64 {
        data_index + (batches_per_round as u64 * data_indicies_per_batch as u64)
    }

    fn move_clients_to_exited(&mut self, height: u32) {
        // WARNING: O(n) on number of clients, need to refactor
        self.epoch_state.clients.retain(|x| {
            if x.state != ClientState::Healthy {
                self.epoch_state.exited_clients.push(*x).unwrap();
                self.epoch_state
                    .exited_clients
                    .last_mut()
                    .unwrap()
                    .exited_height = height;
                false
            } else {
                true
            }
        });
    }

    pub fn total_tokens(&self) -> u64 {
        self.current_round()
            .map(|y| y.data_index)
            .unwrap_or_default()
            * match &self.model {
                Model::LLM(llm) => match llm.data_type {
                    model::LLMTrainingDataType::Pretraining => llm.max_seq_len as u64,
                    model::LLMTrainingDataType::Finetuning => todo!(),
                },
            }
    }
}

impl<I> CoordinatorConfig<I> {
    pub fn check(&self) -> bool {
        self.max_round_train_time != 0
            && self.round_witness_time != 0
            && self.min_clients != 0
            && self.batches_per_round != 0
            && self.data_indicies_per_batch != 0
            && self.rounds_per_epoch != 0
            && self.total_steps != 0
    }
}
