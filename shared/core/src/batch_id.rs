use crate::ClosedInterval;

use serde::{Deserialize, Serialize};
use std::{fmt, ops::RangeInclusive};

#[derive(PartialEq, Eq, Hash, Clone, Copy, PartialOrd, Ord, Serialize, Deserialize)]
pub struct BatchId(pub ClosedInterval<u64>);

impl fmt::Display for BatchId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "B{}", self.0)
    }
}

impl fmt::Debug for BatchId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "B{}", self.0)
    }
}

impl BatchId {
    pub fn iter(&self) -> RangeInclusive<u64> {
        self.0.start..=self.0.end
    }

    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        (self.0.end - self.0.start + 1) as usize
    }
}
