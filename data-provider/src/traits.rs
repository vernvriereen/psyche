use anyhow::Result;
pub trait DataProvider {
    fn get_sample(
        &self,
        data_id: usize,
    ) -> impl std::future::Future<Output = Result<Vec<i32>>> + Send;
}
