use anyhow::Result;
use serde::{Deserialize, Serialize};

pub trait Networkable: Serialize + for<'a> Deserialize<'a> + Send + Sync + 'static {
    fn from_bytes(bytes: &[u8]) -> Result<Self> {
        postcard::from_bytes(bytes).map_err(Into::into)
    }
    fn to_bytes(&self) -> Vec<u8> {
        postcard::to_stdvec(self).expect("postcard::to_stdvec is infallible")
    }
}

impl<T: Serialize + for<'a> Deserialize<'a> + Send + Sync + 'static> Networkable for T {}
