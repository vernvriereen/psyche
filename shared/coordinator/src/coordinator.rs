use crate::{
    model::{self, Checkpoint, Model, LLM},
    traits::Backend,
    Committee, CommitteeProof, CommitteeSelection, WitnessProof,
};

use anchor_lang::{prelude::borsh, AnchorDeserialize, AnchorSerialize, InitSpace};
use bytemuck::{Pod, Zeroable};
use psyche_core::{
    serde_deserialize_string, serde_serialize_string, sha256, Bloom, FixedVec, NodeIdentity,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::hash::Hash;

pub const SOLANA_MAX_STRING_LEN: usize = 64;
pub const SOLANA_MAX_NUM_CLIENTS: usize = 64;
pub const SOLANA_MAX_NUM_WITNESSES: usize = 16;

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
    WaitingForMembers = 0,
    Warmup = 1,
    RoundTrain = 2,
    RoundWitness = 3,
    Cooldown = 4,
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
#[serde(bound = "I: Serialize + DeserializeOwned")]
pub struct Client<I: NodeIdentity> {
    pub id: I,
    pub dropping_at_end_of_round: bool,
}

impl<I: NodeIdentity> anchor_lang::Space for Client<I> {
    const INIT_SPACE: usize = 1;
}

impl<I: NodeIdentity> Hash for Client<I> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

#[derive(
    Clone,
    Default,
    Debug,
    Zeroable,
    Copy,
    AnchorDeserialize,
    AnchorSerialize,
    Serialize,
    Deserialize,
    InitSpace,
)]
#[repr(C)]
pub struct Round {
    pub height: u32,
    pub clients_len: u32,
    pub tie_breaker_tasks: u32,
    pub data_index: u64,
    pub random_seed: u64,
    pub witnesses: FixedVec<Witness, SOLANA_MAX_NUM_WITNESSES>,
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
    InitSpace,
)]
#[repr(C)]
pub struct Witness {
    pub index: u64,
    pub proof: WitnessProof,
    pub commit_bloom: WitnessBloom,
    pub participant_bloom: WitnessBloom,
    pub order_bloom: WitnessBloom,
}

#[derive(Clone, Debug)]
pub enum CoordinatorError {
    NoActiveRound,
    InvalidWitness,
    InvalidRunState,
    DuplicateWitness,
    InvalidHealthCheck,
    Disabled,
    Finished,
    InvalidCheckpoint,
    WitnessesFull,
}

pub type Commitment = [u8; 32];
pub type HealthChecks = Vec<CommitteeProof>;

pub const NUM_STORED_ROUNDS: usize = 4;

#[derive(
    Clone,
    Debug,
    Zeroable,
    Copy,
    Serialize,
    Deserialize,
    AnchorDeserialize,
    AnchorSerialize,
    InitSpace,
)]
#[serde(bound = "T: DeserializeOwned")]
#[repr(C)]
pub struct Coordinator<T: NodeIdentity> {
    #[serde(
        serialize_with = "serde_serialize_string",
        deserialize_with = "serde_deserialize_string"
    )]
    pub run_id: [u8; SOLANA_MAX_STRING_LEN],
    pub run_state: RunState,

    #[serde(default)]
    pub run_state_start_unix_timestamp: u64,

    pub warmup_time: u64,
    pub cooldown_time: u64,

    pub max_round_train_time: u64,
    pub round_witness_time: u64,

    #[serde(default)]
    pub rounds: [Round; NUM_STORED_ROUNDS],
    #[serde(default)]
    pub rounds_head: u32,
    #[serde(default)]
    pub first_round: bool,

    pub min_clients: u32,

    #[serde(default)]
    pub clients: FixedVec<Client<T>, SOLANA_MAX_NUM_CLIENTS>,
    #[serde(default)]
    pub dropped_clients: FixedVec<Client<T>, SOLANA_MAX_NUM_CLIENTS>,

    #[serde(default)]
    pub tick: u64,
    #[serde(default)]
    pub last_tick_unix_timestamp: u64,

    pub batches_per_round: u32,
    pub data_indicies_per_batch: u32,

    pub verification_percent: u8,
    pub witness_nodes: u32,
    pub witness_quorum: u32,

    #[serde(default)]
    pub checkpointers: FixedVec<T, SOLANA_MAX_NUM_CLIENTS>,

    #[serde(default)]
    pub epoch: u32,
    pub rounds_per_epoch: u32,

    #[serde(default)]
    pub step: u32,
    pub total_steps: u32,

    #[serde(default)]
    pub last_step_unix_timestamp: u64,
    #[serde(default)]
    pub epoch_start_data_index: u64,

    pub overlapped: bool,

    pub model: Model,
}

unsafe impl<T: NodeIdentity + Zeroable> Pod for Coordinator<T> {}

#[allow(dead_code)]
fn default_init_step() -> u32 {
    1
}

impl TryFrom<usize> for RunState {
    type Error = CoordinatorError;

    fn try_from(value: usize) -> std::result::Result<Self, Self::Error> {
        match value {
            0 => Ok(RunState::WaitingForMembers),
            1 => Ok(RunState::Warmup),
            2 => Ok(RunState::RoundTrain),
            3 => Ok(RunState::RoundWitness),
            4 => Ok(RunState::Cooldown),
            _ => Err(CoordinatorError::InvalidRunState),
        }
    }
}

impl From<RunState> for usize {
    fn from(val: RunState) -> Self {
        match val {
            RunState::WaitingForMembers => 0,
            RunState::Warmup => 1,
            RunState::RoundTrain => 2,
            RunState::RoundWitness => 3,
            RunState::Cooldown => 4,
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
impl<T: NodeIdentity> Default for Coordinator<T> {
    fn default() -> Self {
        Self {
            run_id: [0; SOLANA_MAX_STRING_LEN],
            run_state: Default::default(),
            run_state_start_unix_timestamp: Default::default(),
            warmup_time: Default::default(),
            rounds_per_epoch: Default::default(),
            max_round_train_time: Default::default(),
            round_witness_time: Default::default(),
            rounds: Default::default(),
            rounds_head: Default::default(),
            first_round: Default::default(),
            min_clients: Default::default(),
            clients: Default::default(),
            dropped_clients: Default::default(),
            tick: Default::default(),
            last_tick_unix_timestamp: Default::default(),
            batches_per_round: Default::default(),
            data_indicies_per_batch: Default::default(),
            verification_percent: Default::default(),
            witness_nodes: Default::default(),
            witness_quorum: Default::default(),
            step: Default::default(),
            last_step_unix_timestamp: Default::default(),
            epoch: Default::default(),
            model: Model::LLM(LLM::dummy()),
            epoch_start_data_index: Default::default(),
            overlapped: Default::default(),
            total_steps: Default::default(),
            cooldown_time: Default::default(),
            checkpointers: Default::default(),
        }
    }
}

impl std::fmt::Display for CoordinatorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CoordinatorError::NoActiveRound => write!(f, "No active round"),
            CoordinatorError::InvalidWitness => write!(f, "Invalid witness"),
            CoordinatorError::InvalidRunState => write!(f, "Invalid run state"),
            CoordinatorError::DuplicateWitness => write!(f, "Duplicate witness"),
            CoordinatorError::InvalidHealthCheck => write!(f, "Invalid health check"),
            CoordinatorError::Disabled => write!(f, "Disabled"),
            CoordinatorError::Finished => write!(f, "Finished"),
            CoordinatorError::InvalidCheckpoint => write!(f, "Invalid checkpoint"),
            CoordinatorError::WitnessesFull => write!(f, "Witnesses full"),
        }
    }
}

impl std::error::Error for CoordinatorError {}

impl std::fmt::Display for RunState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunState::WaitingForMembers => write!(f, "Waiting for members"),
            RunState::Warmup => write!(f, "Warmup"),
            RunState::RoundTrain => write!(f, "Training"),
            RunState::RoundWitness => write!(f, "Witness"),
            RunState::Cooldown => write!(f, "Cooldown"),
        }
    }
}

impl<T: NodeIdentity> Coordinator<T> {
    pub fn tick(
        &mut self,
        backend: &dyn Backend<T>,
        unix_timestamp: u64,
        random_seed: u64,
    ) -> std::result::Result<(), CoordinatorError> {
        if self.min_clients == 0 {
            return Err(CoordinatorError::Disabled);
        }
        match self.run_state {
            RunState::WaitingForMembers => self.tick_waiting_for_members(backend, unix_timestamp),
            RunState::Warmup => self.tick_warmup(unix_timestamp, random_seed),
            RunState::RoundTrain => self.tick_round_train(unix_timestamp),
            RunState::RoundWitness => self.tick_round_witness(unix_timestamp, random_seed),
            RunState::Cooldown => self.tick_cooldown(unix_timestamp),
        }?;
        self.tick += 1;
        self.last_tick_unix_timestamp = unix_timestamp;
        Ok(())
    }

    pub fn witness(
        &mut self,
        from: &Client<T>,
        witness: Witness,
        unix_timestamp: u64,
    ) -> std::result::Result<(), CoordinatorError> {
        if self.min_clients == 0 {
            return Err(CoordinatorError::Disabled);
        }
        if !CommitteeSelection::from_coordinator(self, self.overlapped && !self.first_round)?
            .verify_witness_for_client(&from.id, &witness.proof, &self.clients)
        {
            return Err(CoordinatorError::InvalidWitness);
        }
        if self.run_state != RunState::RoundWitness && self.run_state != RunState::RoundTrain {
            return Err(CoordinatorError::InvalidRunState);
        }

        let round = self.current_round().unwrap();
        for witness in round.witnesses.iter() {
            if self.clients[witness.index as usize] == *from {
                return Err(CoordinatorError::DuplicateWitness);
            }
        }
        let round = self.current_round_mut_unchecked();
        round
            .witnesses
            .push(witness)
            .map_err(|_| CoordinatorError::WitnessesFull)?;

        if round.witnesses.len()
            == match self.witness_nodes {
                0 => self.clients.len(),
                witness_nodes => witness_nodes as usize,
            }
        {
            self.change_state(unix_timestamp, RunState::RoundWitness);
        }
        Ok(())
    }

    pub fn health_check(
        &mut self,
        _from: &Client<T>,
        checks: HealthChecks,
    ) -> std::result::Result<u32, CoordinatorError> {
        if self.min_clients == 0 {
            return Err(CoordinatorError::Disabled);
        }
        for proof in &checks {
            if self.healthy(proof) {
                return Err(CoordinatorError::InvalidHealthCheck);
            }
        }
        let mut dropped = 0;
        for proof in &checks {
            let index = proof.index as usize;
            if !self.clients[index].dropping_at_end_of_round {
                self.clients[index].dropping_at_end_of_round = true;
                dropped += 1;
            }
        }
        // todo: reward `from` for `dropped` health checks
        Ok(dropped)
    }

    pub fn checkpoint(
        &mut self,
        from: &Client<T>,
        checkpoint: Checkpoint,
        unix_timestamp: u64,
    ) -> std::result::Result<(), CoordinatorError> {
        if self.checkpointers.iter().any(|x| *x == from.id) {
            match &mut self.model {
                Model::LLM(llm) => {
                    llm.checkpoint = checkpoint;
                }
            }
            self.finish_cooldown(unix_timestamp);
            Ok(())
        } else {
            Err(CoordinatorError::InvalidCheckpoint)
        }
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
            if !selection.verify_committee_for_client(&client.id, proof, &self.clients) {
                return false;
            }
            match proof.committee {
                Committee::TieBreaker => todo!(),
                Committee::Verifier => todo!(),
                Committee::Trainer => Self::trainer_healthy_by_witnesses(
                    client,
                    &round.witnesses,
                    self.witness_quorum,
                ),
            }
        } else {
            false
        }
    }

    pub fn trainer_healthy_by_witnesses(
        client: &Client<T>,
        witnesses: &[Witness],
        witness_quorum: u32,
    ) -> bool {
        let score: u32 = Self::trainer_healthy_score_by_witnesses(client, witnesses);
        match witness_quorum {
            0 => score as usize == witnesses.len(),
            witness_quorum => score >= witness_quorum,
        }
    }

    pub fn trainer_healthy_score_by_witnesses(client: &Client<T>, witnesses: &[Witness]) -> u32 {
        let hash = sha256(client.id.as_ref());
        let mut score = 0u32;
        for witness in witnesses {
            if witness.participant_bloom.contains(&hash) {
                score += 1;
            }
        }
        score
    }

    pub fn commitment_exists_by_witnesses(
        commitment: &Commitment,
        witnesses: &[Witness],
        witness_quorum: u32,
    ) -> bool {
        let score = Self::commitment_exists_score_by_witnesses(commitment, witnesses);
        match witness_quorum {
            0 => score as usize == witnesses.len(),
            witness_quorum => score >= witness_quorum,
        }
    }

    pub fn commitment_exists_score_by_witnesses(
        commitment: &Commitment,
        witnesses: &[Witness],
    ) -> u32 {
        let hash = sha256(commitment);
        let mut score = 0u32;
        for witness in witnesses {
            if witness.commit_bloom.contains(&hash) {
                score += 1;
            }
        }
        score
    }

    pub fn select_consensus_commitment_by_witnesses(
        commitments: &[Commitment],
        witnesses: &[Witness],
        witness_quorum: u32,
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
        self.rounds.get(self.rounds_head as usize)
    }

    pub fn current_round_mut_unchecked(&mut self) -> &mut Round {
        &mut self.rounds[self.rounds_head as usize]
    }

    pub fn previous_round(&self) -> Option<&Round> {
        match self.current_round() {
            Some(round) => match self.rounds_head == 0 && round.height == 0 {
                true => None,
                false => match self.rounds_head == 0 {
                    true => Some(&self.rounds[NUM_STORED_ROUNDS - 1]),
                    false => Some(&self.rounds[self.rounds_head as usize - 1]),
                },
            },
            None => None,
        }
    }

    pub fn previous_previous_round(&self) -> Option<&Round> {
        match self.current_round() {
            Some(round) => match self.rounds_head == 0 && round.height <= 1 {
                true => None,
                false => match self.rounds_head {
                    0 => Some(&self.rounds[NUM_STORED_ROUNDS - 2]),
                    1 => Some(&self.rounds[NUM_STORED_ROUNDS - 1]),
                    n => Some(&self.rounds[n as usize - 2]),
                },
            },
            None => None,
        }
    }

    pub fn active(&self) -> bool {
        !matches!(
            self.run_state,
            RunState::WaitingForMembers | RunState::Warmup
        )
    }

    pub fn get_client_at_historical_index(
        &self,
        n: usize,
        prev_clients_len: u32,
    ) -> Option<&Client<T>> {
        if n < self.clients.len() {
            Some(&self.clients[n])
        } else if n < prev_clients_len as usize {
            let offset: usize = prev_clients_len as usize - n - 1;
            self.dropped_clients.iter().rev().nth(offset)
        } else {
            None
        }
    }

    pub fn get_historical_clients(&self, clients_len: u32) -> Vec<&Client<T>> {
        (0..clients_len)
            .filter_map(|i| self.get_client_at_historical_index(i as usize, clients_len))
            .collect()
    }

    fn tick_waiting_for_members(
        &mut self,
        backend: &dyn Backend<T>,
        unix_timestamp: u64,
    ) -> std::result::Result<(), CoordinatorError> {
        if self.step > self.total_steps {
            return Err(CoordinatorError::Finished);
        }
        let clients = backend.select_new_clients();
        if clients.len() as u32 >= self.min_clients {
            self.clients = FixedVec::from_iter(clients.into_iter().take(SOLANA_MAX_NUM_CLIENTS));
            self.start_warmup(unix_timestamp);
        }
        Ok(())
    }

    fn tick_warmup(
        &mut self,
        unix_timestamp: u64,
        random_seed: u64,
    ) -> std::result::Result<(), CoordinatorError> {
        if (self.clients.len() as u32) < self.min_clients {
            self.start_waiting_for_members(unix_timestamp);
        } else if self.check_timeout(unix_timestamp, self.warmup_time) {
            self.first_round = true;
            self.start_round_train(unix_timestamp, random_seed, 0);
        }
        Ok(())
    }

    fn tick_round_train(
        &mut self,
        unix_timestamp: u64,
    ) -> std::result::Result<(), CoordinatorError> {
        if self.check_timeout(unix_timestamp, self.max_round_train_time) {
            self.change_state(unix_timestamp, RunState::RoundWitness);
        }
        Ok(())
    }

    fn tick_round_witness(
        &mut self,
        unix_timestamp: u64,
        random_seed: u64,
    ) -> std::result::Result<(), CoordinatorError> {
        if self.check_timeout(unix_timestamp, self.round_witness_time) {
            // TODO: Punish idle witnesses
            self.first_round = false;
            self.step += 1;

            // WARNING: O(n) on number of clients, need to refactor
            self.clients.retain(|x| {
                if x.dropping_at_end_of_round {
                    let _ = self.dropped_clients.push(x.clone());
                    false
                } else {
                    true
                }
            });

            let (height, data_index) = self
                .current_round()
                .map(|x| (x.height, x.data_index))
                .unwrap();
            if self.step > self.total_steps {
                self.start_waiting_for_members(unix_timestamp);
            } else if height == self.rounds_per_epoch - 1 {
                self.start_cooldown(unix_timestamp, data_index);
            } else {
                self.start_round_train(unix_timestamp, random_seed, 0);
            }
        }
        Ok(())
    }

    fn tick_cooldown(&mut self, unix_timestamp: u64) -> std::result::Result<(), CoordinatorError> {
        // cooldown_time == 0 means we never automatically advance to the next epoch,
        // so the only way to get there is through the checkpointing code.
        // this forces everything to wait on a valid checkpoint
        if self.cooldown_time > 0 && self.check_timeout(unix_timestamp, self.cooldown_time) {
            self.finish_cooldown(unix_timestamp);
        }
        Ok(())
    }

    fn check_timeout(&self, unix_timestamp: u64, duration: u64) -> bool {
        self.run_state_start_unix_timestamp != unix_timestamp
            && unix_timestamp >= duration + self.run_state_start_unix_timestamp
    }

    fn start_round_train(&mut self, unix_timestamp: u64, random_seed: u64, tie_breaker_tasks: u32) {
        let (next_rounds_head, next_height, next_data_index) = if self.first_round {
            // very first round, don't increment -- just start here
            (0usize, 0u32, self.epoch_start_data_index)
        } else {
            let current_round = &self.rounds[self.rounds_head as usize];
            (
                (self.rounds_head + 1) as usize % self.rounds.len(),
                current_round.height + 1,
                Self::get_next_round_data_index(
                    current_round.data_index,
                    self.batches_per_round,
                    self.data_indicies_per_batch,
                ),
            )
        };
        let round = &mut self.rounds[next_rounds_head];
        self.rounds_head = next_rounds_head as u32;
        round.clients_len = self.clients.len() as u32;
        round.height = next_height;
        round.data_index = next_data_index;
        round.tie_breaker_tasks = tie_breaker_tasks;
        round.random_seed = random_seed;
        round.witnesses.clear();
        self.change_state(unix_timestamp, RunState::RoundTrain);
    }

    fn start_warmup(&mut self, unix_timestamp: u64) {
        self.rounds = Default::default();
        self.change_state(unix_timestamp, RunState::Warmup);
    }

    fn start_waiting_for_members(&mut self, unix_timestamp: u64) {
        self.dropped_clients.clear();
        self.change_state(unix_timestamp, RunState::WaitingForMembers);
    }

    fn start_cooldown(&mut self, unix_timestamp: u64, data_index: u64) {
        self.epoch_start_data_index = Self::get_next_round_data_index(
            data_index,
            self.batches_per_round,
            self.data_indicies_per_batch,
        );

        match &mut self.model {
            Model::LLM(llm) => {
                llm.checkpoint = Checkpoint::Ephemeral;
            }
        }
        self.change_state(unix_timestamp, RunState::Cooldown);
    }

    fn finish_cooldown(&mut self, unix_timestamp: u64) {
        self.epoch += 1;
        self.start_waiting_for_members(unix_timestamp);
    }

    fn change_state(&mut self, unix_timestamp: u64, new_state: RunState) {
        self.run_state_start_unix_timestamp = unix_timestamp;
        self.run_state = new_state;
    }

    fn get_next_round_data_index(
        data_index: u64,
        batches_per_round: u32,
        data_indicies_per_batch: u32,
    ) -> u64 {
        data_index + (batches_per_round * data_indicies_per_batch) as u64
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

impl Round {
    pub fn empty() -> Self {
        Self {
            height: 0,
            clients_len: 0,
            tie_breaker_tasks: 0,
            data_index: 0,
            random_seed: 0,
            witnesses: Default::default(),
        }
    }
}
