use anyhow::{anyhow, Error, Result};
use memmap2;
use tch::{Device, Tensor};

pub struct Dataset {
    train_tokens: Vec<memmap2::Mmap>,
}

fn mmap_file(p: &std::path::PathBuf) -> Result<memmap2::Mmap> {
    let file = std::fs::File::open(p)?;
    let mmap = unsafe { memmap2::MmapOptions::new().map(&file)? };
    Ok(mmap)
}

impl Dataset {
    pub fn new<P: AsRef<std::path::Path>>(dir: P) -> Result<Self> {
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
        let train_tokens = bin_files
            .iter()
            .map(mmap_file)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { train_tokens })
    }

    pub fn train_tokens(&self) -> usize {
        self.train_tokens.len()
    }
}

pub struct DatasetRandomIter<'a> {
    all_tokens: &'a [memmap2::Mmap],
    tokens: Vec<&'a memmap2::Mmap>,
    current_tokens: &'a memmap2::Mmap,
    indexes_in_bytes: Vec<usize>,
    seq_len: usize,
    token_size_in_bytes: usize,
    device: tch::Device,
}

impl<'a> DatasetRandomIter<'a> {
    pub fn new(
        ds: &'a Dataset,
        seq_len: usize,
        token_size_in_bytes: usize,
        device: Device,
    ) -> Self {
        use rand::seq::SliceRandom;
        use rand::thread_rng;

        let all_tokens = &ds.train_tokens;
        let mut tokens = all_tokens.iter().collect::<Vec<_>>();
        tokens.shuffle(&mut thread_rng());
        let current_tokens = tokens.pop().unwrap();
        let seq_len_in_bytes = seq_len * token_size_in_bytes;
        let mut indexes_in_bytes = (0..current_tokens.len() - seq_len_in_bytes)
            .step_by(seq_len_in_bytes)
            .collect::<Vec<_>>();
        indexes_in_bytes.shuffle(&mut thread_rng());
        Self {
            all_tokens,
            tokens,
            current_tokens,
            indexes_in_bytes,
            seq_len,
            token_size_in_bytes,
            device,
        }
    }
}

impl<'a> Iterator for DatasetRandomIter<'a> {
    type Item = Result<(Tensor, Tensor)>;

    fn next(&mut self) -> Option<Self::Item> {
        use byteorder::{LittleEndian, ReadBytesExt};
        use rand::seq::SliceRandom;
        use rand::thread_rng;

        let seq_len = self.seq_len;
        if self.indexes_in_bytes.is_empty() {
            if self.tokens.is_empty() {
                self.tokens = self.all_tokens.iter().collect();
                self.tokens.shuffle(&mut thread_rng());
            }
            self.current_tokens = self.tokens.pop().unwrap();
            let seq_len_in_bytes = self.seq_len * self.token_size_in_bytes;
            self.indexes_in_bytes = (0..self.current_tokens.len() - seq_len_in_bytes)
                .step_by(seq_len_in_bytes)
                .collect::<Vec<_>>();
            self.indexes_in_bytes.shuffle(&mut thread_rng());
        }
        let start_idx = self.indexes_in_bytes.pop().unwrap();
        let bytes =
            &self.current_tokens[start_idx..start_idx + self.token_size_in_bytes * (seq_len + 1)];
        let tokens = match self.token_size_in_bytes {
            2 => {
                let mut tokens = vec![0u16; bytes.len() / self.token_size_in_bytes];
                if let Err(err) =
                    std::io::Cursor::new(bytes).read_u16_into::<LittleEndian>(&mut tokens)
                {
                    return Some(Err(err.into()));
                }
                tokens.into_iter().map(|v| v as i32).collect::<Vec<_>>()
            }
            4 => {
                let mut tokens = vec![0i32; bytes.len() / self.token_size_in_bytes];
                if let Err(err) =
                    std::io::Cursor::new(bytes).read_i32_into::<LittleEndian>(&mut tokens)
                {
                    return Some(Err(err.into()));
                }
                tokens
            }
            _ => {
                return Some(Err(anyhow!(
                    "unsupported token size {:}",
                    self.token_size_in_bytes
                )))
            }
        };
        let inputs = Tensor::from_slice(&tokens[..seq_len]).to(self.device);
        let targets = Tensor::from_slice(&tokens[1..]).to(self.device);
        Some(Ok((inputs, targets)))
    }
}
