use psyche_core::NodeIdentity;
use psyche_watcher::{Backend, BackendWatcher};

pub struct Client<T: NodeIdentity, B: Backend<T> + 'static> {
    _watcher: BackendWatcher<T, B>,
}

impl<T: NodeIdentity, B: Backend<T> + 'static> Client<T, B> {
    pub fn start(backend: B) -> Self {
        Self {
            _watcher: BackendWatcher::new(backend),
        }
    }
}
