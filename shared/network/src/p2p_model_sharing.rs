use anyhow::Result;
use futures_lite::future::Boxed as BoxedFuture;
use iroh::{endpoint::Connecting, protocol::ProtocolHandler};
use tokio::sync::mpsc::UnboundedSender;

pub const ALPN: &[u8] = b"model-parameter-sharing/0";

#[derive(Debug, Clone)]
pub struct ModelParameterSharing {
    tx_model_parameter_req: UnboundedSender<String>,
}

impl ModelParameterSharing {
    pub fn new(tx_model_parameter_req: UnboundedSender<String>) -> Self {
        Self {
            tx_model_parameter_req,
        }
    }
}

impl ProtocolHandler for ModelParameterSharing {
    fn accept(&self, connecting: Connecting) -> BoxedFuture<Result<()>> {
        Box::pin(async move { Ok(()) })
    }
}
