use crate::{coordinator::SOLANA_MAX_URL_STRING_LEN, SOLANA_MAX_STRING_LEN};

use anchor_lang::{
    prelude::{borsh, msg},
    AnchorDeserialize, AnchorSerialize, InitSpace,
};
use bytemuck::{Zeroable, ZeroableInOption};
use psyche_core::{
    serde_deserialize_optional_string, serde_deserialize_string, serde_serialize_optional_string,
    serde_serialize_string, u8_to_string, ConstantLR, LearningRateSchedule, OptimizerDefinition,
    Shuffle, TokenSize,
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

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
    TS,
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
    TS,
)]
#[repr(C)]
pub enum LLMArchitecture {
    HfLlama,
    HfDeepseek,
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
    TS,
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
    TS,
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
        #[ts(as = "String")]
        [u8; SOLANA_MAX_URL_STRING_LEN],
    ),
    Local(
        #[serde(
            serialize_with = "serde_serialize_string",
            deserialize_with = "serde_deserialize_string"
        )]
        #[ts(as = "String")]
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
    TS,
)]
#[repr(C)]
#[allow(clippy::large_enum_variant)]
pub enum HttpTrainingDataLocation {
    SingleUrl(
        #[serde(
            serialize_with = "serde_serialize_string",
            deserialize_with = "serde_deserialize_string"
        )]
        #[ts(as = "String")]
        [u8; SOLANA_MAX_URL_STRING_LEN],
    ),
    NumberedFiles {
        #[serde(
            serialize_with = "serde_serialize_string",
            deserialize_with = "serde_deserialize_string"
        )]
        #[ts(as = "String")]
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
        #[ts(as = "String")]
        bucket_name: [u8; SOLANA_MAX_URL_STRING_LEN],

        /// 0 len === no filter
        #[serde(
            serialize_with = "serde_serialize_string",
            deserialize_with = "serde_deserialize_string"
        )]
        #[ts(as = "String")]
        filter_directory: [u8; SOLANA_MAX_URL_STRING_LEN],
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
    TS,
)]
#[repr(C)]
pub struct LLM {
    pub architecture: LLMArchitecture,
    pub checkpoint: Checkpoint,
    pub max_seq_len: u32,
    pub data_type: LLMTrainingDataType,
    pub data_location: LLMTrainingDataLocation,
    pub lr_schedule: LearningRateSchedule,
    pub optimizer: OptimizerDefinition,
}

impl LLM {
    pub fn dummy() -> Self {
        Self {
            architecture: LLMArchitecture::HfLlama,
            checkpoint: Checkpoint::Dummy(HubRepo::dummy()),
            data_location: LLMTrainingDataLocation::Dummy,
            data_type: LLMTrainingDataType::Pretraining,
            lr_schedule: LearningRateSchedule::Constant(ConstantLR::default()),
            max_seq_len: 2048,
            optimizer: OptimizerDefinition::Dummy,
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
    TS,
)]
pub struct HubRepo {
    #[serde(
        serialize_with = "serde_serialize_string",
        deserialize_with = "serde_deserialize_string"
    )]
    #[ts(as = "String")]
    pub repo_id: [u8; SOLANA_MAX_STRING_LEN],
    #[serde(
        serialize_with = "serde_serialize_optional_string",
        deserialize_with = "serde_deserialize_optional_string",
        default
    )]
    #[ts(as = "Option<String>")]
    pub revision: Option<[u8; SOLANA_MAX_STRING_LEN]>,
}

impl HubRepo {
    pub fn dummy() -> Self {
        Self {
            repo_id: [0; SOLANA_MAX_STRING_LEN],
            revision: None,
        }
    }
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
    TS,
)]
#[repr(C)]
pub enum Checkpoint {
    Ephemeral,
    Dummy(HubRepo),
    Hub(HubRepo),
    P2P(HubRepo),
}

impl std::fmt::Display for Checkpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Checkpoint::Dummy(_hub_repo) => write!(f, "Dummy"),
            Checkpoint::Ephemeral => write!(f, "Ephemeral"),
            Checkpoint::Hub(hub_repo) => write!(f, "{}", u8_to_string(&hub_repo.repo_id)),
            Checkpoint::P2P(hub_repo) => {
                write!(f, "P2P - Hub repo: {}", u8_to_string(&hub_repo.repo_id))
            }
        }
    }
}

impl Model {
    pub fn check(&self) -> bool {
        match self {
            Model::LLM(llm) => {
                if llm.max_seq_len == 0 {
                    msg!("model check failed: max_seq_len is 0.");
                    return false;
                }
                let bad_data_location = match llm.data_location {
                    LLMTrainingDataLocation::Dummy => false,
                    LLMTrainingDataLocation::Server(url) => url[0] == 0,
                    LLMTrainingDataLocation::Local(_) => false,
                    LLMTrainingDataLocation::Http { location, .. } => match location {
                        HttpTrainingDataLocation::SingleUrl(url) => url[0] == 0,
                        HttpTrainingDataLocation::NumberedFiles {
                            url_template,
                            num_files,
                            ..
                        } => url_template[0] == 0 || num_files == 0,
                        HttpTrainingDataLocation::Gcp { bucket_name, .. } => bucket_name[0] == 0,
                    },
                };
                if bad_data_location {
                    msg!("model check failed: bad LLM training data location.");
                    return false;
                }
                if !match llm.checkpoint {
                    Checkpoint::Dummy(_hub_repo) => false,
                    Checkpoint::Ephemeral => true,
                    Checkpoint::Hub(hub_repo) => !hub_repo.repo_id[0] != 0,
                    Checkpoint::P2P(hub_repo) => !hub_repo.repo_id[0] != 0,
                } {
                    msg!("model check failed: bad checkpoint");
                    return false;
                }
                if !match llm.optimizer {
                    OptimizerDefinition::Dummy => false,
                    OptimizerDefinition::AdamW { .. } => true,
                    OptimizerDefinition::Distro { .. } => true,
                } {
                    msg!("model check failed: bad optimizer");
                    return false;
                }
                true
            }
        }
    }
}
