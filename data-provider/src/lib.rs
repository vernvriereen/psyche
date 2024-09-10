mod local;
mod remote;
mod traits;

pub use local::LocalDataProvider;
pub use remote::{DataProviderTcpClient, DataProviderTcpServer};
pub use traits::DataProvider;
