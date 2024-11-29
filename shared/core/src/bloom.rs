use bitvec::array::BitArray;
use fnv::FnvHasher;
use std::{fmt, hash::Hasher};

#[cfg(target_os = "solana")]
use anchor_lang::prelude::*;
#[cfg(not(target_os = "solana"))]
use serde::{Deserialize, Deserializer, Serialize};

// Modified from https://github.com/solana-labs/solana/blob/27eff8408b7223bb3c4ab70523f8a8dca3ca6645/bloom/src/bloom.rs

/// Generate a stable hash of `self` for each `hash_index`
/// Best effort can be made for uniqueness of each hash.
pub trait BloomHashIndex {
    fn hash_at_index(&self, hash_index: u64) -> u64;
}

#[derive(Clone, PartialEq, Eq)]
pub struct Bloom<const U: usize, const K: usize> {
    pub keys: [u64; K],
    pub bits: BitArray<[u64; U]>,
}

impl<const U: usize, const K: usize> Default for Bloom<U, K> {
    fn default() -> Self {
        Self {
            keys: [0u64; K],
            bits: Default::default(),
        }
    }
}

#[cfg(not(target_os = "solana"))]
impl<const M: usize, const K: usize> Serialize for Bloom<M, K> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("Bloom", 3)?;
        state.serialize_field("keys", &self.keys.to_vec())?;
        state.serialize_field("bits", &self.bits)?;
        state.end()
    }
}

#[cfg(not(target_os = "solana"))]
impl<'de, const U: usize, const K: usize> Deserialize<'de> for Bloom<U, K> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct BloomHelper<const U: usize> {
            keys: Vec<u64>,
            bits: BitArray<[u64; U]>,
        }

        let helper = BloomHelper::deserialize(deserializer)?;

        if helper.keys.len() != K {
            return Err(serde::de::Error::custom(format!(
                "Expected {} keys, got {}",
                K,
                helper.keys.len()
            )));
        }

        let mut keys = [0u64; K];
        keys.copy_from_slice(&helper.keys);

        Ok(Bloom {
            keys,
            bits: helper.bits,
        })
    }
}

#[cfg(target_os = "solana")]
impl<const U: usize, const K: usize> AnchorSerialize for Bloom<U, K> {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        for key in &self.keys {
            key.serialize(writer)?;
        }

        let bits_data = self.bits.as_raw_slice();
        for bit in bits_data {
            bit.serialize(writer)?;
        }

        Ok(())
    }
}

#[cfg(target_os = "solana")]
impl<const U: usize, const K: usize> AnchorDeserialize for Bloom<U, K> {
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let mut keys = [0u64; K];
        for key in &mut keys {
            *key = u64::deserialize_reader(reader)?;
        }

        let mut bits_data = [0u64; U];
        for bit in &mut bits_data {
            *bit = u64::deserialize_reader(reader)?;
        }
        let bits = BitArray::new(bits_data);

        Ok(Bloom { keys, bits })
    }
}

#[cfg(target_os = "solana")]
impl<const U: usize, const K: usize> Space for Bloom<U, K> {
    const INIT_SPACE: usize = U * std::mem::size_of::<u64>() + K * std::mem::size_of::<u64>();
}

impl<const U: usize, const K: usize> fmt::Debug for Bloom<U, K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Bloom {{ keys.len: {} bits: ", self.keys.len(),)?;
        const MAX_PRINT_BITS: usize = 10;
        for i in 0..std::cmp::min(MAX_PRINT_BITS, Self::max_bits()) {
            match self.bits.get(i) {
                Some(x) => {
                    if *x {
                        write!(f, "1")?;
                    } else {
                        write!(f, "0")?;
                    }
                }
                None => write!(f, "X")?,
            }
        }
        if self.bits.len() > MAX_PRINT_BITS {
            write!(f, "..")?;
        }
        write!(f, " }}")
    }
}

impl<const U: usize, const K: usize> Bloom<U, K> {
    pub const fn max_bits() -> usize {
        U * std::mem::size_of::<u64>() * 8
    }

    pub fn new(num_bits: usize, keys_slice: &[u64]) -> Self {
        assert!(num_bits <= Self::max_bits());
        assert!(keys_slice.len() == K);
        let mut keys = [0u64; K];
        keys.copy_from_slice(keys_slice);
        let bits = BitArray::ZERO;
        Bloom { keys, bits }
    }

    /// Create filter optimal for num size given the `FALSE_RATE`.
    ///
    /// The keys are randomized for picking data out of a collision resistant hash of size
    /// `keysize` bytes.
    ///
    /// See <https://hur.st/bloomfilter/>.
    #[cfg(feature = "rand")]
    pub fn random(num_items: usize, false_rate: f64) -> Self {
        use rand::Rng;
        let m = Self::num_bits(num_items as f64, false_rate);
        let num_bits = std::cmp::max(1, std::cmp::min(m as usize, Self::max_bits()));
        let keys: Vec<u64> = (0..K).map(|_| rand::thread_rng().gen()).collect();
        Self::new(num_bits, &keys)
    }

    #[cfg(feature = "rand")]
    fn num_bits(num_items: f64, false_rate: f64) -> f64 {
        let n = num_items;
        let p = false_rate;
        ((n * p.ln()) / (1f64 / 2f64.powf(2f64.ln())).ln()).ceil()
    }

    #[cfg(feature = "rand")]
    #[allow(dead_code)]
    fn num_keys(num_bits: f64, num_items: f64) -> f64 {
        let n = num_items;
        let m = num_bits;
        // infinity as usize is zero in rust 1.43 but 2^64-1 in rust 1.45; ensure it's zero here
        if n == 0.0 {
            0.0
        } else {
            1f64.max(((m / n) * 2f64.ln()).round())
        }
    }

    fn pos<T: BloomHashIndex>(&self, key: &T, k: u64) -> u64 {
        key.hash_at_index(k)
            .checked_rem(self.bits.len() as u64)
            .unwrap_or(0)
    }

    pub fn clear(&mut self) {
        self.bits = BitArray::ZERO;
    }

    pub fn add<T: BloomHashIndex>(&mut self, key: &T) {
        for k in &self.keys {
            let pos = self.pos(key, *k) as usize;
            if !*self.bits.get(pos).unwrap() {
                self.bits.set(pos, true);
            }
        }
    }

    pub fn contains<T: BloomHashIndex>(&self, key: &T) -> bool {
        for k in &self.keys {
            let pos = self.pos(key, *k) as usize;
            if !*self.bits.get(pos).unwrap() {
                return false;
            }
        }
        true
    }
}

fn slice_hash(slice: &[u8], hash_index: u64) -> u64 {
    let mut hasher = FnvHasher::with_key(hash_index);
    hasher.write(slice);
    hasher.finish()
}

impl<T: AsRef<[u8]>> BloomHashIndex for T {
    fn hash_at_index(&self, hash_index: u64) -> u64 {
        slice_hash(self.as_ref(), hash_index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bloom_new() {
        let keys = [1, 2, 3];
        let bloom = Bloom::<16, 3>::new(100, &keys);

        assert_eq!(bloom.keys, keys);
    }

    #[test]
    fn test_bloom_add_and_contains() {
        let mut bloom = Bloom::<8, 2>::new(100, &[1, 2]);

        let item1 = vec![1, 2, 3];
        let item2 = vec![4, 5, 6];

        bloom.add(&item1);
        assert!(bloom.contains(&item1));
        assert!(!bloom.contains(&item2));
    }

    #[test]
    fn test_bloom_clear() {
        let mut bloom = Bloom::<8, 2>::new(100, &[1, 2]);

        let item = vec![1, 2, 3];
        bloom.add(&item);
        assert!(bloom.contains(&item));

        bloom.clear();
        assert!(!bloom.contains(&item));
    }

    #[test]
    fn test_multiple_items() {
        let mut bloom = Bloom::<16, 3>::new(1000, &[1, 2, 3]);

        let items = vec![vec![1, 2, 3], vec![4, 5, 6], vec![7, 8, 9]];

        for item in &items {
            bloom.add(item);
        }

        for item in &items {
            assert!(bloom.contains(item));
        }

        let non_existing = vec![1, 4, 7];
        assert!(!bloom.contains(&non_existing));
    }
}
