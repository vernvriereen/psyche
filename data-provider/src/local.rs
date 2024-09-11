use anyhow::{anyhow, Result};
use rand::seq::SliceRandom;
use rand_chacha::rand_core::SeedableRng;
use rand_chacha::ChaCha8Rng;
use std::fs;
use tracing::info;

use crate::traits::DataProvider;

fn mmap_file(p: &std::path::PathBuf) -> Result<memmap2::Mmap> {
    let file = std::fs::File::open(p)?;
    let mmap = unsafe { memmap2::MmapOptions::new().map(&file)? };
    Ok(mmap)
}

struct SequencePointer {
    file_index: usize,
    byte_offset: usize,
}

pub struct LocalDataProvider {
    data_files: Vec<memmap2::Mmap>,
    sequences: Vec<SequencePointer>,
    seq_len: usize,
    token_size_in_bytes: usize,
}

impl LocalDataProvider {
    pub fn new_from_directory(
        dir: impl AsRef<std::path::Path>,
        token_size_in_bytes: usize,
        num_tokens_per_sequence: usize, // num tokens per sequence
        random_seed: <ChaCha8Rng as SeedableRng>::Seed,
    ) -> Result<Self> {
        let dir = dir.as_ref();
        let mut bin_files = vec![];
        for file in std::fs::read_dir(dir)?.flatten() {
            let file = file.path();
            if let Some(extension) = file.extension() {
                if extension == "bin" || extension == "npy" {
                    bin_files.push(file)
                }
            }
        }
        let data_files = bin_files
            .iter()
            .map(mmap_file)
            .collect::<Result<Vec<_>>>()?;

        info!(
            "Loaded {} files ({}) of training data from directory {}",
            bin_files.len(),
            bin_files
                .iter()
                .map(|f| fs::metadata(f).unwrap().len())
                .sum::<u64>(),
            dir.display()
        );

        let mut deterministic_rng = ChaCha8Rng::from_seed(random_seed);
        let seq_len_in_bytes = num_tokens_per_sequence * token_size_in_bytes;

        let sequences: Vec<SequencePointer> = {
            let mut all_indexes: Vec<_> = data_files
                .iter()
                .enumerate()
                // find every sequence in every file
                .flat_map(|(file_index, current_tokens)| {
                    (0..current_tokens.len() - seq_len_in_bytes)
                        .step_by(seq_len_in_bytes)
                        .map(move |byte_offset| SequencePointer {
                            file_index,
                            byte_offset,
                        })
                })
                .collect();
            // and shuffle the whole collection, to avoid bias from a specific file
            all_indexes.shuffle(&mut deterministic_rng);
            all_indexes
        };

        Ok(Self {
            data_files,
            sequences,
            seq_len: num_tokens_per_sequence,
            token_size_in_bytes,
        })
    }
    /// len in data_ids
    pub fn len(&self) -> usize {
        self.sequences.len()
    }

    fn internal_get_raw_sample(&self, data_id: usize) -> Result<&[u8]> {
        let SequencePointer {
            byte_offset,
            file_index,
        } = self.sequences.get(data_id).ok_or_else(|| {
            anyhow!(
                "index {data_id} is out of bounds, we only have {} samples.",
                self.sequences.len()
            )
        })?;

        let file = &self.data_files[*file_index];
        let data_len = self.token_size_in_bytes * (self.seq_len + 1);
        let data = &file[*byte_offset..*byte_offset + data_len];

        Ok(data)
    }
}

impl DataProvider for LocalDataProvider {
    /// NOTE: pretraining only for now since it reads an extra token.
    /// Do we want to build two traits, one for pretrain and one for finetuning?
    async fn get_raw_sample(&self, data_id: usize) -> Result<Vec<u8>> {
        self.internal_get_raw_sample(data_id).map(|x| x.to_vec())
    }
}

pub struct LocalDataProviderIter<'a> {
    provider: &'a LocalDataProvider,
    current_index: usize,
}

impl<'a> Iterator for LocalDataProviderIter<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_index < self.provider.len() {
            let result = self
                .provider
                .internal_get_raw_sample(self.current_index)
                .unwrap();
            self.current_index += 1;
            Some(result)
        } else {
            None
        }
    }
}

impl<'a> IntoIterator for &'a LocalDataProvider {
    type Item = &'a [u8];
    type IntoIter = LocalDataProviderIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        LocalDataProviderIter {
            provider: self,
            current_index: 0,
        }
    }
}
