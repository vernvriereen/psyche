use crate::Client;
use psyche_core::NodeIdentity;

pub trait Backend<T: NodeIdentity> {
    fn select_new_clients(&self) -> &[Client<T>];
}
