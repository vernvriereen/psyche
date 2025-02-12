use crate::{coordinator::SOLANA_MAX_URL_STRING_LEN, SOLANA_MAX_STRING_LEN};

use anchor_lang::{prelude::borsh, AnchorDeserialize, AnchorSerialize, InitSpace};
use bytemuck::{Zeroable, ZeroableInOption};
use psyche_core::{
    serde_deserialize_optional_string, serde_deserialize_string, serde_serialize_optional_string,
    serde_serialize_string, u8_to_string, LearningRateScheduler, Shuffle, TokenSize,
};
use serde::{Deserialize, Serialize};

#[derive(
    Clone,
    Debug,
    Copy,
    Zeroable,
    AnchorDeserialize,
    AnchorSerialize,
    Serialize,
    Deserialize,
    InitSpace,
)]
#[repr(C)]
pub enum Model {
    LLM(LLM),
}

unsafe impl ZeroableInOption for Model {}

#[derive(
    Clone,
    Debug,
    Copy,
    Zeroable,
    AnchorDeserialize,
    AnchorSerialize,
    Serialize,
    Deserialize,
    InitSpace,
)]
#[repr(C)]
pub enum LLMArchitecture {
    HfLlama,
}

#[derive(
    Clone,
    Debug,
    Copy,
    Zeroable,
    AnchorDeserialize,
    AnchorSerialize,
    Serialize,
    Deserialize,
    InitSpace,
)]
#[repr(C)]
pub enum LLMTrainingDataType {
    Pretraining,
    Finetuning,
}

#[derive(
    AnchorSerialize,
    AnchorDeserialize,
    InitSpace,
    Serialize,
    Deserialize,
    Clone,
    Debug,
    Zeroable,
    Copy,
)]
#[repr(C)]
#[allow(clippy::large_enum_variant)]
pub enum LLMTrainingDataLocation {
    Dummy,
    Server(
        #[serde(
            serialize_with = "serde_serialize_string",
            deserialize_with = "serde_deserialize_string"
        )]
        [u8; SOLANA_MAX_URL_STRING_LEN],
    ),
    Local(
        #[serde(
            serialize_with = "serde_serialize_string",
            deserialize_with = "serde_deserialize_string"
        )]
        [u8; SOLANA_MAX_URL_STRING_LEN],
    ),
    Http {
        location: HttpTrainingDataLocation,
        token_size_in_bytes: TokenSize,
        shuffle: Shuffle,
    },
}

/// NOTE: Support for Vecs of URLs is not enabled because of the large size it would support.
#[derive(
    AnchorSerialize,
    AnchorDeserialize,
    InitSpace,
    Serialize,
    Deserialize,
    Clone,
    Debug,
    Zeroable,
    Copy,
)]
#[repr(C)]
#[allow(clippy::large_enum_variant)]
pub enum HttpTrainingDataLocation {
    SingleUrl(
        #[serde(
            serialize_with = "serde_serialize_string",
            deserialize_with = "serde_deserialize_string"
        )]
        [u8; SOLANA_MAX_URL_STRING_LEN],
    ),
    NumberedFiles {
        #[serde(
            serialize_with = "serde_serialize_string",
            deserialize_with = "serde_deserialize_string"
        )]
        url_template: [u8; SOLANA_MAX_URL_STRING_LEN],
        start_index: u32,
        n_left_pad_zeros: u8,
        num_files: u32,
    },
    Gcp {
        #[serde(
            serialize_with = "serde_serialize_string",
            deserialize_with = "serde_deserialize_string"
        )]
        bucket_url: [u8; SOLANA_MAX_URL_STRING_LEN],

        /// 0 len === no filter
        #[serde(
            serialize_with = "serde_serialize_string",
            deserialize_with = "serde_deserialize_string"
        )]
        filter_directory: [u8; SOLANA_MAX_URL_STRING_LEN],
    },
}

#[derive(
    AnchorSerialize,
    Default,
    AnchorDeserialize,
    InitSpace,
    Serialize,
    Deserialize,
    Clone,
    Debug,
    Zeroable,
    Copy,
)]
#[repr(C)]
pub struct ConstantLR {
    base_lr: f32,
    warmup_steps: u32,
    warmup_init_lr: f32,
}

#[derive(
    AnchorSerialize,
    AnchorDeserialize,
    InitSpace,
    Serialize,
    Deserialize,
    Clone,
    Debug,
    Zeroable,
    Copy,
)]
#[repr(C)]
pub struct LinearLR {
    base_lr: f32,
    warmup_steps: u32,
    warmup_init_lr: f32,
    total_steps: u32,
    final_lr: f32,
}

#[derive(
    AnchorSerialize,
    AnchorDeserialize,
    InitSpace,
    Serialize,
    Deserialize,
    Clone,
    Debug,
    Zeroable,
    Copy,
)]
#[repr(C)]
pub struct CosineLR {
    base_lr: f32,
    warmup_steps: u32,
    warmup_init_lr: f32,
    total_steps: u32,
    final_lr: f32,
}

#[derive(
    AnchorSerialize,
    AnchorDeserialize,
    InitSpace,
    Serialize,
    Deserialize,
    Clone,
    Debug,
    Zeroable,
    Copy,
)]
#[repr(C)]
pub enum LearningRateSchedule {
    Constant(ConstantLR),
    Linear(LinearLR),
    Cosine(CosineLR),
}

#[derive(
    AnchorSerialize,
    AnchorDeserialize,
    InitSpace,
    Serialize,
    Deserialize,
    Clone,
    Debug,
    Zeroable,
    Copy,
)]
#[repr(C)]
pub enum Optimizer {
    Dummy,
    AdamW {
        betas: [f32; 2],
        weight_decay: f32,
        eps: f32,
        clip_grad_norm: f32,
    },
    Distro {
        clip_grad_norm: Option<f32>,
        compression_decay: f32,
        compression_decay_warmup_steps: u32,
        compression_topk: u16,
        compression_topk_startup: u16,
        compression_topk_startup_steps: u32,
        compression_chunk: u16,
        quantize_1bit: bool,
    },
}

#[derive(
    AnchorSerialize,
    AnchorDeserialize,
    InitSpace,
    Serialize,
    Deserialize,
    Clone,
    Debug,
    Zeroable,
    Copy,
)]
#[repr(C)]
pub struct LLM {
    pub architecture: LLMArchitecture,
    pub checkpoint: Checkpoint,
    pub max_seq_len: u32,
    pub data_type: LLMTrainingDataType,
    pub data_location: LLMTrainingDataLocation,
    pub lr_schedule: LearningRateSchedule,
    pub optimizer: Optimizer,
}

impl LLM {
    pub fn dummy() -> Self {
        Self {
            architecture: LLMArchitecture::HfLlama,
            checkpoint: Checkpoint::Dummy,
            data_location: LLMTrainingDataLocation::Dummy,
            data_type: LLMTrainingDataType::Pretraining,
            lr_schedule: LearningRateSchedule::Constant(ConstantLR::default()),
            max_seq_len: 2048,
            optimizer: Optimizer::Dummy,
        }
    }
}

#[derive(
    Clone,
    Debug,
    Copy,
    AnchorDeserialize,
    AnchorSerialize,
    InitSpace,
    Serialize,
    Deserialize,
    PartialEq,
)]
pub struct HubRepo {
    #[serde(
        serialize_with = "serde_serialize_string",
        deserialize_with = "serde_deserialize_string"
    )]
    pub repo_id: [u8; SOLANA_MAX_STRING_LEN],
    #[serde(
        serialize_with = "serde_serialize_optional_string",
        deserialize_with = "serde_deserialize_optional_string",
        default
    )]
    pub revision: Option<[u8; SOLANA_MAX_STRING_LEN]>,
}

#[derive(
    AnchorSerialize,
    AnchorDeserialize,
    InitSpace,
    Serialize,
    Deserialize,
    Clone,
    Debug,
    Zeroable,
    Copy,
)]
#[repr(C)]
pub enum Checkpoint {
    Dummy,
    Ephemeral,
    Hub(HubRepo),
    P2P,
}

impl std::fmt::Display for Checkpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Checkpoint::Dummy => write!(f, "Dummy"),
            Checkpoint::Ephemeral => write!(f, "Ephemeral"),
            Checkpoint::Hub(hub_repo) => write!(f, "{}", u8_to_string(&hub_repo.repo_id)),
            Checkpoint::P2P => {
                write!(f, "P2P")
            }
        }
    }
}

impl From<ConstantLR> for psyche_core::ConstantLR {
    fn from(value: ConstantLR) -> Self {
        psyche_core::ConstantLR::new(
            value.base_lr as f64,
            value.warmup_steps,
            value.warmup_init_lr as f64,
        )
    }
}

impl From<LinearLR> for psyche_core::LinearLR {
    fn from(value: LinearLR) -> Self {
        psyche_core::LinearLR::new(
            value.base_lr as f64,
            value.warmup_steps,
            value.warmup_init_lr as f64,
            value.total_steps,
            value.final_lr as f64,
        )
    }
}

impl From<CosineLR> for psyche_core::CosineLR {
    fn from(value: CosineLR) -> Self {
        psyche_core::CosineLR::new(
            value.base_lr as f64,
            value.warmup_steps,
            value.warmup_init_lr as f64,
            value.total_steps,
            value.final_lr as f64,
        )
    }
}

// TODO why not unify the values here and in core?
#[derive(Clone)]
pub enum AnyLearningRateScheduler {
    Constant(psyche_core::ConstantLR),
    Linear(psyche_core::LinearLR),
    Cosine(psyche_core::CosineLR),
}

impl AnyLearningRateScheduler {
    pub fn get_lr(&self, step: u32) -> f64 {
        match self {
            Self::Constant(l) => l.get_lr(step),
            Self::Linear(l) => l.get_lr(step),
            Self::Cosine(l) => l.get_lr(step),
        }
    }

    pub fn get_warmup_steps(&self) -> u32 {
        match self {
            Self::Constant(l) => l.get_warmup_steps(),
            Self::Linear(l) => l.get_warmup_steps(),
            Self::Cosine(l) => l.get_warmup_steps(),
        }
    }

    pub fn get_warmup_init_lr(&self) -> f64 {
        match self {
            Self::Constant(l) => l.get_warmup_init_lr(),
            Self::Linear(l) => l.get_warmup_init_lr(),
            Self::Cosine(l) => l.get_warmup_init_lr(),
        }
    }
}

impl From<LearningRateSchedule> for AnyLearningRateScheduler {
    fn from(value: LearningRateSchedule) -> Self {
        match value {
            LearningRateSchedule::Constant(c) => Self::Constant(c.into()),
            LearningRateSchedule::Linear(c) => Self::Linear(c.into()),
            LearningRateSchedule::Cosine(c) => Self::Cosine(c.into()),
        }
    }
}

impl Model {
    pub fn check(&self) -> bool {
        match self {
            Model::LLM(llm) => {
                llm.max_seq_len != 0
                    && match llm.data_location {
                        LLMTrainingDataLocation::Dummy => false,
                        LLMTrainingDataLocation::Server(url) => url[0] != 0,
                        LLMTrainingDataLocation::Local(_) => true,
                        LLMTrainingDataLocation::Http { location, .. } => match location {
                            HttpTrainingDataLocation::SingleUrl(url) => url[0] != 0,
                            HttpTrainingDataLocation::NumberedFiles {
                                url_template,
                                num_files,
                                ..
                            } => url_template[0] != 0 && num_files > 0,
                            HttpTrainingDataLocation::Gcp { bucket_url, .. } => bucket_url[0] != 0,
                        },
                    }
                    && match llm.checkpoint {
                        Checkpoint::Dummy => false,
                        Checkpoint::Ephemeral => true,
                        Checkpoint::Hub(hub_repo) => hub_repo.repo_id[0] != 0,
                        Checkpoint::P2P => true,
                    }
                    && match llm.optimizer {
                        Optimizer::Dummy => false,
                        Optimizer::AdamW { .. } => true,
                        Optimizer::Distro { .. } => true,
                    }
            }
        }
    }
}
