use crate::ClosedInterval;

use serde::{Deserialize, Serialize};
use std::{fmt, ops::RangeInclusive, str::FromStr};

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

impl FromStr for BatchId {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim_start_matches('B');

        let start_bracket = s.find('[').ok_or("Missing '[' in input")?;
        let comma = s.find(',').ok_or("Missing ',' in input")?;
        let end_bracket = s.find(']').ok_or("Missing ']' in input")?;

        let start = u64::from_str(&s[start_bracket + 1..comma])
            .map_err(|_| "Failed to parse start value")?;
        let end =
            u64::from_str(&s[comma + 1..end_bracket]).map_err(|_| "Failed to parse end value")?;

        let interval = ClosedInterval { start, end };
        Ok(BatchId(interval))
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
