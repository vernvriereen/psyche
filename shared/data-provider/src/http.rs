use anyhow::{anyhow, bail, Result};
use futures::future::join_all;
use psyche_coordinator::model::HttpTrainingDataLocation;
use psyche_core::{u8_to_string, BatchId, Shuffle, TokenSize};
use rand::seq::SliceRandom;
use rand_chacha::rand_core::SeedableRng;
use rand_chacha::ChaCha8Rng;
use tokio::task::JoinHandle;
use tracing::{info, trace};

use crate::traits::{LengthKnownDataProvider, TokenizedDataProvider};

#[derive(Clone, Copy, Debug)]
struct SequencePointer {
    file_index: usize,
    byte_offset: usize,
}

pub struct HttpDataProvider {
    client: reqwest::Client,
    file_urls: Vec<String>,
    sequences: Vec<SequencePointer>,
    seq_len: u32,
    token_size_in_bytes: TokenSize,
}

impl LengthKnownDataProvider for HttpDataProvider {
    fn len(&self) -> usize {
        self.sequences.len()
    }
}

pub enum FileURLs {
    FixedList(Vec<String>),
    /// A url like https://example.com/{}.ds
    /// will be transformed into "https://example.com/000.ds", "https://example.com/001.ds", etc.
    NumberedFiles {
        url_template: String,
        start_index: usize,
        n_left_pad_zeros: usize,
        num_files: usize,
    },
}

impl FileURLs {
    pub fn from_list(urls: &[String]) -> Self {
        FileURLs::FixedList(urls.to_vec())
    }

    pub fn from_template(
        url_template: String,
        start_index: usize,
        n_left_pad_zeros: usize,
        num_files: usize,
    ) -> Result<Self> {
        let num_templates = url_template
            .as_bytes()
            .windows(2)
            .filter(|&w| w == "{}".as_bytes())
            .count();
        if num_templates != 1 {
            bail!("invalid url {url_template}. expected 1 set of {{}} for number substitution, got {num_templates}");
        }
        Ok(FileURLs::NumberedFiles {
            url_template,
            start_index,
            n_left_pad_zeros,
            num_files,
        })
    }
}

impl From<FileURLs> for Vec<String> {
    fn from(v: FileURLs) -> Self {
        match v {
            FileURLs::FixedList(v) => v,
            FileURLs::NumberedFiles {
                url_template,
                start_index,
                n_left_pad_zeros,
                num_files,
            } => (0..num_files)
                .map(|index| {
                    let number = start_index + index;
                    let formatted_number = format!("{:0>width$}", number, width = n_left_pad_zeros);
                    url_template.replace("{}", &formatted_number)
                })
                .collect(),
        }
    }
}

impl From<&HttpTrainingDataLocation> for FileURLs {
    fn from(val: &HttpTrainingDataLocation) -> Self {
        match val {
            HttpTrainingDataLocation::SingleUrl(u) => FileURLs::from_list(&[u8_to_string(u)]),
            HttpTrainingDataLocation::NumberedFiles {
                url_template,
                start_index,
                n_left_pad_zeros,
                num_files,
            } => FileURLs::from_template(
                u8_to_string(url_template),
                (*start_index).try_into().expect("u32 fits in usize"),
                (*n_left_pad_zeros).try_into().expect("u32 fits in usize"),
                (*num_files).try_into().expect("u32 fits in usize"),
            )
            .expect("URL was validated before byte-stringing!"),
        }
    }
}
impl HttpDataProvider {
    pub async fn new(
        file_urls: impl Into<FileURLs>,
        token_size_in_bytes: TokenSize,
        num_tokens_per_sequence: u32,
        shuffle: Shuffle,
    ) -> Result<Self> {
        let file_urls: Vec<_> = file_urls.into().into();
        let num_files = file_urls.len();

        let client = reqwest::Client::new();
        let sizes = get_file_sizes(&client, &file_urls).await?;

        let seq_len_in_bytes =
            num_tokens_per_sequence as u64 * usize::from(token_size_in_bytes) as u64;

        let sequences: Vec<SequencePointer> = {
            let mut all_indexes: Vec<_> = (0..num_files)
                .flat_map(|file_index| {
                    let file_size = sizes[file_index];
                    (0..file_size - (seq_len_in_bytes + usize::from(token_size_in_bytes) as u64)) // +1 token for pretraining data!
                        .step_by(seq_len_in_bytes as usize)
                        .map(move |byte_offset| SequencePointer {
                            file_index,
                            byte_offset: byte_offset as usize,
                        })
                })
                .collect();

            if let Shuffle::Seeded(seed) = shuffle {
                let mut rng = ChaCha8Rng::from_seed(seed);
                all_indexes.shuffle(&mut rng);
            }
            all_indexes
        };

        info!(
            "Created HTTP data provider for {} files with {} sequences",
            num_files,
            sequences.len()
        );

        Ok(Self {
            client,
            file_urls,
            sequences,
            seq_len: num_tokens_per_sequence,
            token_size_in_bytes,
        })
    }

    async fn fetch_data_range(
        client: reqwest::Client,
        url: String,
        start: usize,
        length: usize,
    ) -> Result<Vec<u8>> {
        trace!(
            "requesting bytes={}-{} from {url}",
            start,
            start + length - 1
        );

        let response = client
            .get(url)
            .header("Range", format!("bytes={}-{}", start, start + length - 1))
            .send()
            .await?;

        // Check if we got a 206 Partial Content response
        if !response.status().is_success()
            && response.status() != reqwest::StatusCode::PARTIAL_CONTENT
        {
            return Err(anyhow::anyhow!(
                "Server returned unexpected status code: {}",
                response.status()
            ));
        }

        let bytes = response.bytes().await?;
        let received_length = bytes.len();

        // Verify we got the expected amount of data
        if received_length != length {
            return Err(anyhow::anyhow!(
                "Received unexpected number of bytes: got {}, expected {}",
                received_length,
                length
            ));
        }

        Ok(bytes.to_vec())
    }

    async fn fetch_tokenized_data_range(
        client: reqwest::Client,
        url: String,
        start: usize,
        length: usize,
        token_size_in_bytes: TokenSize,
    ) -> Result<Vec<i32>> {
        let data = Self::fetch_data_range(client, url, start, length).await?;

        let tokens: Vec<i32> = data
            .chunks(token_size_in_bytes.into())
            .map(|t| {
                use TokenSize::*;
                match token_size_in_bytes {
                    TwoBytes => u16::from_le_bytes(t.try_into().unwrap()) as i32,
                    FourBytes => u32::from_le_bytes(t.try_into().unwrap()) as i32,
                }
            })
            .collect();

        Ok(tokens)
    }

    async fn internal_get_samples(&self, data_ids: &[BatchId]) -> Result<Vec<Vec<i32>>> {
        trace!("get samples for {data_ids:?}");
        let mut futures = Vec::new();

        let sequences: Result<Vec<SequencePointer>> = data_ids
            .iter()
            .map(|data_id| {
                self.sequences
                    .get(u64::from(*data_id) as usize)
                    .cloned()
                    .ok_or_else(|| {
                        anyhow!(
                            "index {data_id} is out of bounds, we only have {} samples.",
                            self.sequences.len()
                        )
                    })
            })
            .collect();
        let sequences = sequences?;

        // check if this is fully sequential
        // TODO: deal with stepping by seq_len but reading seq_len + 1 -- can we change this? datatrove steps by seq_len + 1
        // let first_file_index = sequences[0].file_index;
        // let offset_len  = usize::from(self.token_size_in_bytes) * (self.seq_len as usize);
        // let sequential = sequences.iter().all(|x| x.file_index == first_file_index)
        //     && sequences
        //         .windows(2)
        //         .all(|x| x[1].byte_offset - x[0].byte_offset == data_len);

        let data_len = usize::from(self.token_size_in_bytes) * (self.seq_len as usize + 1);
        for sequence in sequences {
            let future: JoinHandle<Result<Vec<i32>>> =
                tokio::spawn(Self::fetch_tokenized_data_range(
                    self.client.clone(),
                    self.file_urls[sequence.file_index].clone(),
                    sequence.byte_offset,
                    data_len,
                    self.token_size_in_bytes,
                ));

            futures.push(future);
        }
        let finished = join_all(futures.into_iter()).await;

        let mut ret = Vec::with_capacity(finished.len());
        for finish in finished {
            ret.push(finish??);
        }

        Ok(ret)
    }
}

impl TokenizedDataProvider for HttpDataProvider {
    async fn get_samples(&mut self, data_ids: &[BatchId]) -> Result<Vec<Vec<i32>>> {
        self.internal_get_samples(data_ids).await
    }
}

// i tried this nicely with streams and generators.
// there's some weird rust impl is not general enough for Send bug i hit
// so i just manually chunk instead of doing it fancy with a limited concurrency stream
async fn get_file_sizes(client: &reqwest::Client, urls: &[String]) -> Result<Vec<u64>> {
    let futures: Vec<_> = urls
        .iter()
        .map(|url| {
            let url = url.clone();
            async move {
                let response = client.head(&url).send().await?;

                if !response.status().is_success() {
                    bail!("URL validation failed for {}: {}", url, response.status());
                }

                // grab the Content-Length header
                let size = response
                    .headers()
                    .get(reqwest::header::CONTENT_LENGTH)
                    .and_then(|h| h.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok())
                    .ok_or_else(|| {
                        anyhow::anyhow!("Missing or invalid Content-Length header for {}", url)
                    })?;
                Ok::<u64, anyhow::Error>(size)
            }
        })
        .collect();

    let mut results = Vec::with_capacity(urls.len());
    let mut futures = futures.into_iter();

    // only pull 2 chunks at once
    while let Some(first) = futures.next() {
        let mut chunk = vec![first];
        for _ in 0..2 {
            if let Some(next) = futures.next() {
                chunk.push(next);
            } else {
                break;
            }
        }

        let chunk_results = futures::future::join_all(chunk).await;
        for result in chunk_results {
            results.push(result?);
        }
    }

    Ok(results)
}
