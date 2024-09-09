use crate::traits::Backend;
use psyche_serde::derive_serialize;

#[cfg(target_os = "solana")]
use anchor_lang::prelude::*;
#[cfg(not(target_os = "solana"))]
use serde::{Deserialize, Serialize};

pub enum Model {

}

pub enum LLM {

}

pub enum Checkpoint {
    HuggingFace
}