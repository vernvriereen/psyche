use {
    bv::BitVec,
    fnv::FnvHasher,
    serde::{Deserialize, Serialize},
    std::{fmt, hash::Hasher, marker::PhantomData},
};

// from https://github.com/solana-labs/solana/blob/27eff8408b7223bb3c4ab70523f8a8dca3ca6645/bloom/src/bloom.rs

/// Generate a stable hash of `self` for each `hash_index`
/// Best effort can be made for uniqueness of each hash.
pub trait BloomHashIndex {
    fn hash_at_index(&self, hash_index: u64) -> u64;
}

#[derive(Serialize, Deserialize, Default, Clone, PartialEq, Eq)]
pub struct Bloom<T: BloomHashIndex> {
    pub keys: Vec<u64>,
    pub bits: BitVec<u64>,
    num_bits_set: u64,
    _phantom: PhantomData<T>,
}

impl<T: BloomHashIndex> fmt::Debug for Bloom<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Bloom {{ keys.len: {} bits.len: {} num_set: {} bits: ",
            self.keys.len(),
            self.bits.len(),
            self.num_bits_set
        )?;
        const MAX_PRINT_BITS: u64 = 10;
        for i in 0..std::cmp::min(MAX_PRINT_BITS, self.bits.len()) {
            if self.bits.get(i) {
                write!(f, "1")?;
            } else {
                write!(f, "0")?;
            }
        }
        if self.bits.len() > MAX_PRINT_BITS {
            write!(f, "..")?;
        }
        write!(f, " }}")
    }
}

impl<T: BloomHashIndex> Bloom<T> {
    pub fn new(num_bits: usize, keys: Vec<u64>) -> Self {
        let bits = BitVec::new_fill(false, num_bits as u64);
        Bloom {
            keys,
            bits,
            num_bits_set: 0,
            _phantom: PhantomData,
        }
    }
    /// Create filter optimal for num size given the `FALSE_RATE`.
    ///
    /// The keys are randomized for picking data out of a collision resistant hash of size
    /// `keysize` bytes.
    ///
    /// See <https://hur.st/bloomfilter/>.
    #[cfg(feature = "rand")]
    pub fn random(num_items: usize, false_rate: f64, max_bits: usize) -> Self {
        use rand::Rng;
        let m = Self::num_bits(num_items as f64, false_rate);
        let num_bits = std::cmp::max(1, std::cmp::min(m as usize, max_bits));
        let num_keys = Self::num_keys(num_bits as f64, num_items as f64) as usize;
        let keys: Vec<u64> = (0..num_keys).map(|_| rand::thread_rng().gen()).collect();
        Self::new(num_bits, keys)
    }
    #[cfg(feature = "rand")]
    fn num_bits(num_items: f64, false_rate: f64) -> f64 {
        let n = num_items;
        let p = false_rate;
        ((n * p.ln()) / (1f64 / 2f64.powf(2f64.ln())).ln()).ceil()
    }
    #[cfg(feature = "rand")]
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
    fn pos(&self, key: &T, k: u64) -> u64 {
        key.hash_at_index(k)
            .checked_rem(self.bits.len())
            .unwrap_or(0)
    }
    pub fn clear(&mut self) {
        self.bits = BitVec::new_fill(false, self.bits.len());
        self.num_bits_set = 0;
    }
    pub fn add(&mut self, key: &T) {
        for k in &self.keys {
            let pos = self.pos(key, *k);
            if !self.bits.get(pos) {
                self.num_bits_set = self.num_bits_set.saturating_add(1);
                self.bits.set(pos, true);
            }
        }
    }
    pub fn contains(&self, key: &T) -> bool {
        for k in &self.keys {
            let pos = self.pos(key, *k);
            if !self.bits.get(pos) {
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
