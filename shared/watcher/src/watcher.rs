use crate::traits::{Backend, Client};
use psyche_coordinator::Coordinator;
use psyche_core::NodeIdentity;

pub async fn watcher<T, B, C>(backend: &B, client: &C)
where
    T: NodeIdentity,
    B: Backend<T> + 'static,
    C: Client<T> + 'static,
{
    let mut prev: Option<Coordinator<T>> = None;
    loop {
        let state = backend.wait_for_new_state().await;
        match prev {
            None => {
                client.on_run_state_change(&state, &prev).await;
            }
            Some(ref old) => {
                if old.run_state != state.run_state {
                    client.on_run_state_change(&state, &prev).await;
                }
            }
        }
        prev = Some(state);
    }
}
