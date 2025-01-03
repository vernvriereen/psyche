use crate::{DataProviderTcpClient, DummyDataProvider, TokenizedDataProvider};

use psyche_network::AuthenticatableIdentity;

pub enum DataProvider<T: AuthenticatableIdentity> {
    Server(DataProviderTcpClient<T>),
    Dummy(DummyDataProvider),
}

impl<T: AuthenticatableIdentity> TokenizedDataProvider for DataProvider<T> {
    async fn get_samples(
        &mut self,
        data_ids: &[psyche_core::BatchId],
    ) -> anyhow::Result<Vec<Vec<i32>>> {
        match self {
            DataProvider::Server(data_provider_tcp_client) => {
                data_provider_tcp_client.get_samples(data_ids).await
            }
            DataProvider::Dummy(dummy_data_provider) => {
                dummy_data_provider.get_samples(data_ids).await
            }
        }
    }
}
