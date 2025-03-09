use anyhow::Result;
use psyche_core::{BatchId, ClosedInterval, Shuffle, TokenSize};
use psyche_data_provider::{
    DummyDataProvider, LengthKnownDataProvider, TokenizedDataProvider, WeightedDataProvider,
};
use std::collections::HashMap;
use test_log::test;

struct MockDataProvider {
    id: usize,
    num_samples: usize,
    data_patterns: Vec<i32>,
}

impl MockDataProvider {
    fn new(id: usize, num_samples: usize, data_patterns: Vec<i32>) -> Self {
        Self {
            id,
            num_samples,
            data_patterns,
        }
    }
}

impl LengthKnownDataProvider for MockDataProvider {
    fn num_sequences(&self) -> usize {
        self.num_samples
    }
}

impl TokenizedDataProvider for MockDataProvider {
    async fn get_samples(&mut self, data_ids: BatchId) -> Result<Vec<Vec<i32>>> {
        // Create sample sequences where each token is provider_id * 1000 + sample_id
        let mut results = Vec::with_capacity(data_ids.len());

        for data_id in data_ids.iter() {
            let base_value = (self.id as i32) * 1000 + (data_id as i32);
            let sequence = self
                .data_patterns
                .iter()
                .map(|&pattern| base_value + pattern)
                .collect();
            results.push(sequence);
        }

        Ok(results)
    }
}

#[cfg(test)]
const TEST_SEED: [u8; 32] = [
    164, 143, 161, 123, 88, 50, 61, 10, 234, 184, 161, 204, 105, 1, 20, 184, 43, 140, 200, 117, 24,
    180, 247, 84, 141, 68, 110, 161, 228, 223, 32, 242,
];
#[test(tokio::test)]
async fn test_weighted_data_provider_equal_weights() -> Result<()> {
    let provider1 = MockDataProvider::new(1, 100, vec![0, 1, 2, 3]);
    let provider2 = MockDataProvider::new(2, 100, vec![0, 1, 2, 3]);

    let mut weighted_provider = WeightedDataProvider::new(
        vec![(provider1, 0.5), (provider2, 0.5)],
        Shuffle::Seeded(TEST_SEED),
    );
    assert_eq!(weighted_provider.num_sequences(), 200);

    let batch_id = BatchId(ClosedInterval { start: 0, end: 99 }); // get 100 samples
    let samples = weighted_provider.get_samples(batch_id).await?;

    let mut provider_counts = HashMap::new();
    for sample in &samples {
        // first token's value will tell us which provider it came from
        let provider_id = sample[0] / 1000;
        *provider_counts.entry(provider_id).or_insert(0) += 1;
    }

    // With equal weights, we should get approximately equal numbers of samples
    // Allow for some randomness, but it should be close
    let count1 = *provider_counts.get(&1).unwrap_or(&0);
    let count2 = *provider_counts.get(&2).unwrap_or(&0);

    println!("Provider 1 count: {}, Provider 2 count: {}", count1, count2);

    assert!((40..=60).contains(&count1));
    assert!((40..=60).contains(&count2));
    assert_eq!(count1 + count2, 100);

    Ok(())
}

#[test(tokio::test)]
async fn test_weighted_data_provider_unequal_weights() -> Result<()> {
    let provider1 = MockDataProvider::new(1, 100, vec![0, 1, 2, 3]);
    let provider2 = MockDataProvider::new(2, 100, vec![0, 1, 2, 3]);

    let mut weighted_provider = WeightedDataProvider::new(
        vec![(provider1, 0.75), (provider2, 0.25)],
        Shuffle::Seeded(TEST_SEED),
    );
    assert_eq!(weighted_provider.num_sequences(), 200);

    let batch_id = BatchId(ClosedInterval { start: 0, end: 99 });
    let samples = weighted_provider.get_samples(batch_id).await?;

    let mut provider_counts = HashMap::new();
    for sample in &samples {
        let provider_id = sample[0] / 1000;
        *provider_counts.entry(provider_id).or_insert(0) += 1;
    }

    let count1 = *provider_counts.get(&1).unwrap_or(&0);
    let count2 = *provider_counts.get(&2).unwrap_or(&0);

    println!("Provider 1 count: {}, Provider 2 count: {}", count1, count2);

    assert!((65..=85).contains(&count1));
    assert!((15..=35).contains(&count2));
    assert_eq!(count1 + count2, 100);

    Ok(())
}

#[test(tokio::test)]
async fn test_weighted_data_provider_auto_weights() -> Result<()> {
    let provider1 = MockDataProvider::new(1, 100, vec![0, 1, 2, 3]);
    let provider2 = MockDataProvider::new(2, 300, vec![0, 1, 2, 3]); // 3x larger

    let mut weighted_provider =
        WeightedDataProvider::new(vec![provider1, provider2], Shuffle::Seeded(TEST_SEED));
    assert_eq!(weighted_provider.num_sequences(), 400);

    let batch_id = BatchId(ClosedInterval { start: 0, end: 99 });
    let samples = weighted_provider.get_samples(batch_id).await?;

    let mut provider_counts = HashMap::new();
    for sample in &samples {
        let provider_id = sample[0] / 1000;
        *provider_counts.entry(provider_id).or_insert(0) += 1;
    }

    let count1 = *provider_counts.get(&1).unwrap_or(&0);
    let count2 = *provider_counts.get(&2).unwrap_or(&0);

    println!("Provider 1 count: {}, Provider 2 count: {}", count1, count2);

    assert!((15..=35).contains(&count1)); // ~25%
    assert!((65..=85).contains(&count2)); // ~75%
    assert_eq!(count1 + count2, 100);

    Ok(())
}

#[test(tokio::test)]
async fn test_weighted_data_provider_consistency() -> Result<()> {
    let provider1 = MockDataProvider::new(1, 100, vec![0, 1, 2, 3]);
    let provider2 = MockDataProvider::new(2, 100, vec![0, 1, 2, 3]);

    let seed: [u8; 32] = [
        126, 9, 9, 27, 212, 158, 163, 168, 134, 97, 31, 10, 56, 78, 2, 175, 107, 226, 111, 216,
        178, 207, 80, 230, 45, 98, 155, 87, 237, 191, 68, 22,
    ];
    let mut weighted_provider1 = WeightedDataProvider::new(
        vec![(provider1, 0.5), (provider2, 0.5)],
        Shuffle::Seeded(seed),
    );
    assert_eq!(weighted_provider1.num_sequences(), 200);

    let batch_id = BatchId(ClosedInterval { start: 0, end: 9 });
    let samples1 = weighted_provider1.get_samples(batch_id).await?;

    let provider3 = MockDataProvider::new(1, 100, vec![0, 1, 2, 3]);
    let provider4 = MockDataProvider::new(2, 100, vec![0, 1, 2, 3]);

    let mut weighted_provider2 = WeightedDataProvider::new(
        vec![(provider3, 0.5), (provider4, 0.5)],
        Shuffle::Seeded(seed),
    );
    assert_eq!(weighted_provider2.num_sequences(), 200);

    let samples2 = weighted_provider2.get_samples(batch_id).await?;
    assert_eq!(samples1, samples2);

    Ok(())
}

#[test(tokio::test)]
async fn test_weighted_data_provider_with_dummy_provider() -> Result<()> {
    let dummy1 = DummyDataProvider::new(TokenSize::TwoBytes, 10, 50); // 10 tokens per sequence
    let dummy2 = DummyDataProvider::new(TokenSize::TwoBytes, 10, 50);

    let mut weighted_provider = WeightedDataProvider::new(
        vec![(dummy1, 0.5), (dummy2, 0.5)],
        Shuffle::Seeded(TEST_SEED),
    );
    assert_eq!(weighted_provider.num_sequences(), 100);

    let batch_id = BatchId(ClosedInterval { start: 0, end: 9 });
    let samples = weighted_provider.get_samples(batch_id).await?;

    assert_eq!(samples.len(), 10);
    // each sample should have 11 tokens (10 + 1 for next token prediction)
    for sample in samples {
        assert_eq!(sample.len(), 11);
    }

    Ok(())
}

#[test(tokio::test)]
async fn test_weighted_data_provider_multi_batch() -> Result<()> {
    let provider1 = MockDataProvider::new(1, 100, vec![0, 1, 2, 3]);
    let provider2 = MockDataProvider::new(2, 100, vec![0, 1, 2, 3]);

    let mut weighted_provider = WeightedDataProvider::new(
        vec![(provider1, 0.5), (provider2, 0.5)],
        Shuffle::Seeded(TEST_SEED),
    );
    assert_eq!(weighted_provider.num_sequences(), 200);

    let batch1 = BatchId(ClosedInterval { start: 0, end: 9 });
    let samples1 = weighted_provider.get_samples(batch1).await?;

    let batch2 = BatchId(ClosedInterval { start: 10, end: 19 });
    let samples2 = weighted_provider.get_samples(batch2).await?;

    for sample1 in &samples1 {
        for sample2 in &samples2 {
            assert_ne!(sample1, sample2);
        }
    }

    let samples1_again = weighted_provider.get_samples(batch1).await?;
    assert_eq!(samples1, samples1_again);

    Ok(())
}

#[test(tokio::test)]
async fn test_weighted_data_provider_exhaustive() -> Result<()> {
    let provider1 = MockDataProvider::new(1, 5, vec![0, 1]);
    let provider2 = MockDataProvider::new(2, 5, vec![0, 1]);

    let mut weighted_provider = WeightedDataProvider::new(
        vec![(provider1, 0.5), (provider2, 0.5)],
        Shuffle::Seeded(TEST_SEED),
    );
    assert_eq!(weighted_provider.num_sequences(), 10);

    let batch = BatchId(ClosedInterval { start: 0, end: 9 });
    let samples = weighted_provider.get_samples(batch).await?;

    assert_eq!(samples.len(), 10);

    let mut provider1_samples = 0;
    let mut provider2_samples = 0;

    for sample in &samples {
        let provider_id = sample[0] / 1000;
        match provider_id {
            1 => provider1_samples += 1,
            2 => provider2_samples += 1,
            _ => panic!("Unexpected provider ID"),
        }
    }

    assert_eq!(provider1_samples, 5);
    assert_eq!(provider2_samples, 5);

    Ok(())
}
