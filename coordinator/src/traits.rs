use crate::coordinator::Coordinator;

pub trait CoordinatorBackend<T> {
    fn select_new_clients(&self) -> &[T];
}

#[async_trait::async_trait]
pub trait WatcherBackend<T> {
    async fn wait_for_new_state(&self) -> Coordinator<T>;
}

#[async_trait::async_trait]
pub trait WatcherClient<T> {
    async fn on_run_state_change(&self, state: &Coordinator<T>, prev: &Option<Coordinator<T>>);
}