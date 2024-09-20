use anyhow::Result;
use psyche_core::NodeIdentity;
use psyche_watcher::{Backend, BackendWatcher};
use tokio::sync::mpsc;

pub struct Client {
    rx: mpsc::Receiver<Result<()>>,
}

impl Client {
    pub fn start<T: NodeIdentity, B: Backend<T> + 'static>(backend: B) -> Self {
        let (tx, rx) = mpsc::channel(1);
        Self::run(BackendWatcher::new(backend), tx);
        Self { rx }
    }

    pub async fn finish(&mut self) -> Result<()> {
        self.rx.recv().await.unwrap_or(Ok(()))
    }

    fn run<T: NodeIdentity, B: Backend<T> + 'static>(
        _watcher: BackendWatcher<T, B>,
        _tx: mpsc::Sender<Result<()>>,
    ) {
    }
}
