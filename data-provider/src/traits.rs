use anyhow::Result;
pub trait DataProvider {
    fn get_raw_sample(
        &self,
        data_id: usize,
    ) -> impl std::future::Future<Output = Result<Vec<u8>>> + Send;
}
