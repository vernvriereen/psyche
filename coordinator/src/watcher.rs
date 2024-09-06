use crate::{
    coordinator::Coordinator,
    traits::{WatcherBackend, WatcherClient},
};

pub async fn watcher<T, B: WatcherBackend<T>, C: WatcherClient<T>>(
    backend: &dyn WatcherBackend<T>,
    client: &dyn WatcherClient<T>,
) {
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
