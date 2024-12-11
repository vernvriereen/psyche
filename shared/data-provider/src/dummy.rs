use crate::token_size::TokenSize;
use crate::traits::TokenizedDataProvider;
use anyhow::Result;
use psyche_core::BatchId;

pub struct DummyDataProvider {
    seq_len: usize,
    token_size_in_bytes: TokenSize,
}

impl DummyDataProvider {
    pub fn new(
        token_size_in_bytes: TokenSize,
        num_tokens_per_sequence: usize, // num tokens per sequence
    ) -> Self {
        Self {
            seq_len: num_tokens_per_sequence,
            token_size_in_bytes,
        }
    }

    fn internal_get_samples(&self, num_samples: usize) -> Result<Vec<Vec<i32>>> {
        let mut ret: Vec<_> = Vec::new();
        for _ in 0..num_samples {
            let data_len = usize::from(self.token_size_in_bytes) * (self.seq_len + 1);
            let data = vec![0; data_len];

            let tokens: Vec<i32> = data
                .chunks(self.token_size_in_bytes.into())
                .map(|t| {
                    use TokenSize::*;
                    match self.token_size_in_bytes {
                        TwoBytes => u16::from_le_bytes(t.try_into().unwrap()) as i32,
                        FourBytes => u32::from_le_bytes(t.try_into().unwrap()) as i32,
                    }
                })
                .collect();
            ret.push(tokens);
        }
        Ok(ret)
    }
}

impl TokenizedDataProvider for DummyDataProvider {
    async fn get_samples(&mut self, data_ids: &[BatchId]) -> Result<Vec<Vec<i32>>> {
        self.internal_get_samples(data_ids.len())
    }
}
