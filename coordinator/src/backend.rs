pub trait Backend<I> {
    fn select_new_clients(&self) -> &[I];
}