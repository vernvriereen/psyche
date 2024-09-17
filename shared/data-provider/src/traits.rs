use anyhow::Result;
pub trait TokenizedDataProvider {
    fn get_sample(
        &mut self,
        data_id: usize,
    ) -> impl std::future::Future<Output = Result<Vec<i32>>> + Send;
}
