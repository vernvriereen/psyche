use anyhow::Result;
use psyche_coordinator::Coordinator;
use psyche_core::NodeIdentity;

#[async_trait::async_trait]
pub trait Backend<T: NodeIdentity> {
    async fn wait_for_new_state(&mut self) -> Result<Coordinator<T>>;
}

