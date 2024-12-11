use psyche_network::NetworkableNodeIdentity;

use crate::{DataProviderTcpClient, DummyDataProvider, TokenizedDataProvider};

pub enum DataProvider<T: NetworkableNodeIdentity> {
    Server(DataProviderTcpClient<T>),
    Dummy(DummyDataProvider),
}

impl<T: NetworkableNodeIdentity> TokenizedDataProvider for DataProvider<T> {
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
