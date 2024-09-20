use crate::traits::Backend;
use anyhow::Result;
use psyche_coordinator::Coordinator;
use psyche_core::NodeIdentity;
use std::mem::replace;

pub struct BackendWatcher<T, B>
where
    T: NodeIdentity,
    B: Backend<T> + 'static,
{
    backend: B,
    state: Option<Coordinator<T>>,
}

impl<T, B> BackendWatcher<T, B>
where
    T: NodeIdentity,
    B: Backend<T> + 'static,
{
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            state: None,
        }
    }

    pub async fn poll_next(&mut self) -> Result<(Option<Coordinator<T>>, &Coordinator<T>)> {
        let new_state = self.backend.wait_for_new_state().await?;
        let prev = replace(&mut self.state, Some(new_state));
        Ok((prev, self.state.as_ref().unwrap()))
    }
}
