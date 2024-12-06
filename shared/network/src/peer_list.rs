use crate::Networkable;

use anyhow::Result;
use iroh::{base::base32, net::NodeAddr};
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};
use thiserror::Error;

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct PeerList(pub Vec<NodeAddr>);

impl fmt::Display for PeerList {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", base32::fmt(self.to_bytes()))
    }
}

#[derive(Error, Debug)]
pub enum ParsePeerListError {
    #[error("Failed to parse bytes out of base32 text")]
    Base32Parse,
    #[error("Failed to parse peerlist from bytes")]
    BytesParse,
}

impl FromStr for PeerList {
    type Err = ParsePeerListError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_bytes(&base32::parse_vec(s).map_err(|_| ParsePeerListError::Base32Parse)?)
            .map_err(|_| ParsePeerListError::BytesParse)
    }
}
