use anyhow::Result;
use psyche_core::BatchId;
pub trait TokenizedDataProvider {
    fn get_samples(
        &mut self,
        data_ids: BatchId,
    ) -> impl std::future::Future<Output = Result<Vec<Vec<i32>>>> + Send;
}

pub trait LengthKnownDataProvider {
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
