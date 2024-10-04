use anyhow::Result;
pub trait TokenizedDataProvider {
    fn get_samples(
        &mut self,
        data_ids: Vec<usize>,
    ) -> impl std::future::Future<Output = Result<Vec<Vec<i32>>>> + Send;
}

pub trait LengthKnownDataProvider {
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
