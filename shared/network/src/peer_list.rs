use std::{fmt, str::FromStr};

use anyhow::Result;
use iroh::{base::base32, net::NodeAddr};
use psyche_core::Networkable;
use serde::{Deserialize, Serialize};

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct PeerList(pub Vec<NodeAddr>);

impl fmt::Display for PeerList {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", base32::fmt(self.to_bytes()))
    }
}

impl FromStr for PeerList {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_bytes(&base32::parse_vec(s)?)
    }
}
