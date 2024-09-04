use crate::traits::CoordinatorBackend;

pub struct Coordinator<B: CoordinatorBackend>  {
    unix_timestamp: u64,
    backend: B
}

impl<B: CoordinatorBackend> Coordinator<B> {
    pub fn step() {

    }
}