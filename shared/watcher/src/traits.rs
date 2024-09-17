use psyche_coordinator::Coordinator;
use psyche_core::NodeIdentity;

#[async_trait::async_trait]
pub trait Backend<T: NodeIdentity> {
    async fn wait_for_new_state(&self) -> Coordinator<T>;
}

#[async_trait::async_trait]
pub trait Client<T: NodeIdentity> {
    async fn on_run_state_change(&self, state: &Coordinator<T>, prev: &Option<Coordinator<T>>);
}
