use psyche_coordinator::coordinator::Coordinator;

#[async_trait::async_trait]
pub trait Backend<T> {
    async fn wait_for_new_state(&self) -> Coordinator<T>;
}

#[async_trait::async_trait]
pub trait Client<T> {
    async fn on_run_state_change(&self, state: &Coordinator<T>, prev: &Option<Coordinator<T>>);
}
