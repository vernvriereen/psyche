use std::{future::Future, pin::Pin};

pub type BoxedFuture<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;
