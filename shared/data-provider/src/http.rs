use std::str::FromStr;

use anyhow::{anyhow, bail, Result};
use futures::future::join_all;
use psyche_coordinator::model::HttpTrainingDataLocation;
use psyche_core::{u8_to_string, BatchId, Shuffle, TokenSize};
use rand::seq::SliceRandom;
use rand_chacha::rand_core::SeedableRng;
use rand_chacha::ChaCha8Rng;
use regex::Regex;
use reqwest::IntoUrl;
use tokio::task::JoinHandle;
use tracing::{info, trace};

use crate::{
    file_extensions::DATA_FILE_EXTENSIONS,
    traits::{LengthKnownDataProvider, TokenizedDataProvider},
};

#[derive(Clone, Copy, Debug)]
struct SequencePointer {
    file_index: usize,
    byte_offset: usize,
}

pub struct HttpDataProvider {
    client: reqwest::Client,
    file_urls: Vec<reqwest::Url>,
    sequences: Vec<SequencePointer>,
    seq_len: u32,
    token_size_in_bytes: TokenSize,
}

impl LengthKnownDataProvider for HttpDataProvider {
    fn len(&self) -> usize {
        self.sequences.len()
    }
}

/// A Vec of (url, file size)
pub struct FileURLs(Vec<(reqwest::Url, u64)>);

impl FileURLs {
    pub async fn from_list(urls: &[impl IntoUrl + Clone]) -> Result<Self, anyhow::Error> {
        let client = reqwest::Client::new();
        let urls: Result<Vec<reqwest::Url>, reqwest::Error> =
            urls.iter().map(|u| u.clone().into_url()).collect();
        let urls_with_sizes = with_file_sizes(&client, &urls?).await?;

        Ok(FileURLs(urls_with_sizes))
    }

    pub async fn from_template(
        url_template: &str,
        start_index: u32,
        n_left_pad_zeros: u8,
        num_files: u32,
    ) -> Result<Self> {
        let num_templates = url_template
            .as_bytes()
            .windows(2)
            .filter(|&w| w == "{}".as_bytes())
            .count();
        if num_templates != 1 {
            bail!("invalid url {url_template}. expected 1 set of {{}} for number substitution, got {num_templates}");
        }

        let urls: Result<Vec<reqwest::Url>, <reqwest::Url as FromStr>::Err> = (0..num_files)
            .map(|index| {
                let number = start_index + index;
                let formatted_number =
                    format!("{:0>width$}", number, width = n_left_pad_zeros as usize);
                url_template.replace("{}", &formatted_number).parse()
            })
            .collect();

        let client = reqwest::Client::new();
        let urls_with_sizes = with_file_sizes(&client, &urls?).await?;

        Ok(Self(urls_with_sizes))
    }

    pub async fn from_gcp_bucket(
        bucket_url: impl IntoUrl,
        directory: Option<String>,
    ) -> Result<Self> {
        let bucket_url = bucket_url.into_url()?;
        let gcp_xml_contents = reqwest::get(bucket_url.clone()).await?.text().await?;
        let files_in_bucket: Vec<(String, u64)> = parse_gcp_xml(&gcp_xml_contents);
        let data_files_matching_directory: Result<Vec<(reqwest::Url, u64)>, anyhow::Error> =
            files_in_bucket
                .into_iter()
                .filter_map(|(file_path, size)| {
                    let path_parts: Vec<&str> = file_path.split('/').collect();

                    if let Some(match_dir) = &directory {
                        if path_parts.is_empty() || path_parts[0] != match_dir {
                            return None;
                        }
                    }

                    let file_ext = path_parts.last()?.split('.').last()?;
                    if !DATA_FILE_EXTENSIONS.contains(&file_ext) {
                        return None;
                    }

                    let full_url = bucket_url
                        .join(&file_path)
                        .map_err(|_| anyhow!("invalid url part {file_path}"));
                    Some(full_url.map(|full_url| (full_url, size)))
                })
                .collect();

        Ok(Self(data_files_matching_directory?))
    }

    pub async fn from_location(location: &HttpTrainingDataLocation) -> Result<Self> {
        match location {
            HttpTrainingDataLocation::NumberedFiles {
                url_template,
                start_index,
                n_left_pad_zeros,
                num_files,
            } => {
                Self::from_template(
                    &u8_to_string(url_template),
                    *start_index,
                    *n_left_pad_zeros,
                    *num_files,
                )
                .await
            }
            HttpTrainingDataLocation::SingleUrl(url) => Self::from_list(&[u8_to_string(url)]).await,
            HttpTrainingDataLocation::Gcp {
                bucket_url,
                filter_directory,
            } => {
                let filter_directory = u8_to_string(filter_directory);
                Self::from_gcp_bucket(
                    u8_to_string(bucket_url),
                    if filter_directory.is_empty() {
                        None
                    } else {
                        Some(filter_directory)
                    },
                )
                .await
            }
        }
    }
}

fn parse_gcp_xml(xml: &str) -> Vec<(String, u64)> {
    let re = Regex::new(r"(?s)<Key>([^<]+)</Key>.*?<Size>(\d+)</Size>").unwrap();
    re.captures_iter(xml)
        .map(|cap| (cap[1].to_string(), cap[2].parse().unwrap()))
        .collect()
}

impl HttpDataProvider {
    pub fn new(
        file_urls: FileURLs,
        token_size_in_bytes: TokenSize,
        num_tokens_per_sequence: u32,
        shuffle: Shuffle,
    ) -> Result<Self> {
        let file_urls = file_urls.0;
        let num_files = file_urls.len();

        let client = reqwest::Client::new();

        let seq_len_in_bytes =
            num_tokens_per_sequence as u64 * usize::from(token_size_in_bytes) as u64;

        let sequences: Vec<SequencePointer> = {
            let mut all_indexes: Vec<_> = (0..num_files)
                .flat_map(|file_index| {
                    let file_size = file_urls[file_index].1;
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
            file_urls: file_urls.into_iter().map(|f| f.0).collect(),
            sequences,
            seq_len: num_tokens_per_sequence,
            token_size_in_bytes,
        })
    }

    async fn fetch_data_range(
        client: reqwest::Client,
        url: reqwest::Url,
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
        url: reqwest::Url,
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
async fn with_file_sizes(
    client: &reqwest::Client,
    urls: &[reqwest::Url],
) -> Result<Vec<(reqwest::Url, u64)>> {
    let futures: Vec<_> = urls
        .iter()
        .map(|url| {
            let url = url.clone();
            async move {
                let response = client.head(url.clone()).send().await?;

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
                Ok::<(reqwest::Url, u64), anyhow::Error>((url, size))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_gcp_xml() {
        let xml = r#"
        <ListBucketResult xmlns="http://doc.s3.amazonaws.com/2006-03-01"><Name>nous-pretraining-public-us</Name><Prefix /><Marker /><IsTruncated>false</IsTruncated><Contents><Key>fineweb-1pct-tokenized-llama3/000_fineweb.ds.index</Key><Generation>1736437740991720</Generation><MetaGeneration>1</MetaGeneration><LastModified>2025-01-09T15:49:01.055Z</LastModified><ETag>"58bfa7b320e9ddf7163d67cb95f5c4ac"</ETag><Size>118694800</Size>
        </Contents>
        <Contents>
            <Key>fineweb-1pct-tokenized-llama3/000_fineweb.ds.metadata</Key>
            <Generation>1736437738841326</Generation>
            <MetaGeneration>1</MetaGeneration>
            <LastModified>2025-01-09T15:48:58.887Z</LastModified>
            <ETag>"333054bbb946a240ca09e6c312135314"</ETag>
            <Size>50</Size>
        </Contents>
        "#;

        let expected = vec![
            (
                "fineweb-1pct-tokenized-llama3/000_fineweb.ds.index".to_string(),
                118694800,
            ),
            (
                "fineweb-1pct-tokenized-llama3/000_fineweb.ds.metadata".to_string(),
                50,
            ),
        ];

        assert_eq!(parse_gcp_xml(xml), expected);
    }
}
