pub trait Backend<T> {
    fn select_new_clients(&self) -> &[T];
}
