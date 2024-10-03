use anyhow::Result;
use psyche_coordinator::{Coordinator, HealthChecks, Witness};
use psyche_core::NodeIdentity;

#[async_trait::async_trait]
pub trait Backend<T: NodeIdentity>: Send + Sync {
    /// # Cancel safety
    ///
    /// This method must be cancel safe.
    async fn wait_for_new_state(&mut self) -> Result<Coordinator<T>>;
    async fn send_witness(&mut self, witness: Witness) -> Result<()>;
    async fn send_health_check(&mut self, health_check: HealthChecks) -> Result<()>;
}
