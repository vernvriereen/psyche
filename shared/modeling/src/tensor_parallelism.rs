use std::sync::Arc;
use tch::{
    nn::{self, Module, Shard},
    Tensor,
};

#[cfg(feature = "parallelism")]
use tch::{CStore, Device, ReduceOpType, CNCCL};

#[cfg(feature = "parallelism")]
pub type Communicator = CNCCL;

#[cfg(feature = "parallelism")]
pub type CommunicatorId = CStore;

#[cfg(not(feature = "parallelism"))]
#[derive(Debug)]
pub struct Communicator;

#[cfg(not(feature = "parallelism"))]
#[derive(Debug, Copy, Clone)]
pub struct CommunicatorId;

#[cfg(not(feature = "parallelism"))]
impl Communicator {
    pub fn size(&self) -> i64 {
        unimplemented!()
    }

    pub fn rank(&self) -> usize {
        unimplemented!()
    }
}

#[cfg(not(feature = "parallelism"))]
impl CommunicatorId {
    pub fn new() -> Option<Self> {
        unimplemented!()
    }
}

pub enum ReduceType {
    Sum,
    Max,
}

#[cfg(feature = "parallelism")]
impl From<ReduceType> for ReduceOpType {
    fn from(value: ReduceType) -> Self {
        match value {
            ReduceType::Sum => ReduceOpType::Sum,
            ReduceType::Max => ReduceOpType::Max,
        }
    }
}

#[derive(Debug)]
pub struct TensorParallelRowLinear {
    pub(crate) linear: nn::Linear,
    pub(crate) comm: Option<Arc<Communicator>>,
}

unsafe impl Send for TensorParallelRowLinear {}

impl TensorParallelRowLinear {
    pub fn new(linear: nn::Linear, comm: Option<Arc<Communicator>>) -> Self {
        Self { linear, comm }
    }
}

impl Module for TensorParallelRowLinear {
    #[cfg(feature = "parallelism")]
    fn forward(&self, x: &Tensor) -> Tensor {
        let mut x = self.linear.forward(x).contiguous();
        x.differentiable_all_reduce_sum_(&self.comm);
        x
    }

    #[cfg(not(feature = "parallelism"))]
    fn forward(&self, x: &Tensor) -> Tensor {
        assert!(self.comm.is_none());
        self.linear.forward(x).contiguous()
    }
}

pub trait AllReduce {
    fn all_reduce_(&mut self, comm: &Option<Arc<Communicator>>, op: ReduceType);
}

pub trait DifferentiableAllReduceSum {
    fn differentiable_all_reduce_sum_(&mut self, comm: &Option<Arc<Communicator>>);
}

pub trait CudaSynchronize {
    fn cuda_synchronize(&self);
}

impl AllReduce for Tensor {
    #[cfg(feature = "parallelism")]
    fn all_reduce_(&mut self, comm: &Option<Arc<Communicator>>, op: ReduceType) {
        if let Some(comm) = comm {
            let device = self.device();
            comm.all_reduce(&[self], op.into()).unwrap();
            device.cuda_synchronize();
        }
    }

    #[cfg(not(feature = "parallelism"))]
    fn all_reduce_(&mut self, comm: &Option<Arc<Communicator>>, _op: ReduceType) {
        assert!(comm.is_none());
    }
}

impl DifferentiableAllReduceSum for Tensor {
    #[cfg(feature = "parallelism")]
    fn differentiable_all_reduce_sum_(&mut self, comm: &Option<Arc<Communicator>>) {
        if let Some(comm) = comm {
            comm.differentiable_all_reduce_sum(&self).unwrap();
            self.device().cuda_synchronize();
        }
    }

    #[cfg(not(feature = "parallelism"))]
    fn all_reduce_(&mut self, comm: &Option<Arc<Communicator>>, _op: ReduceType) {
        assert!(comm.is_none());
    }
}

impl CudaSynchronize for Device {
    fn cuda_synchronize(&self) {
        match &self {
            Device::Cuda(rank) => tch::Cuda::synchronize(*rank as i64),
            _ => panic!("Cannot CUDA synchronize non-CUDA device"),
        }
    }
}

#[allow(unused)]
pub fn unshard_tensor(sharded_tensors: Vec<Tensor>, shard: &Shard) -> Tensor {
    let Shard {
        dim, world_size, ..
    } = *shard;

    let mut full_shape = sharded_tensors[0].size();
    let shard_size = full_shape[dim];
    full_shape[dim] = shard_size * (world_size as i64);

    let full_tensor = Tensor::empty(
        &full_shape,
        (sharded_tensors[0].kind(), sharded_tensors[0].device()),
    );

    for (rank, shard_tensor) in sharded_tensors.into_iter().enumerate() {
        let start = (rank as i64) * shard_size;
        let end = ((rank + 1) as i64) * shard_size;

        let mut slice = full_tensor.slice(dim as i64, start, Some(end), 1);
        slice.copy_(&shard_tensor);
    }

    full_tensor
}

#[allow(unused)]
pub fn tensor_shard(full_tensor: &Tensor, shard: &Shard, n: usize) -> Tensor {
    let Shard {
        dim, world_size, ..
    } = *shard;

    let full_shape = full_tensor.size();
    let total_size = full_shape[dim];

    let shard_size = total_size / (world_size as i64);
    let start = (n as i64) * shard_size;
    let end = ((n + 1) as i64) * shard_size;

    full_tensor.slice(dim as i64, start, Some(end), 1)
}

#[allow(unused)]
pub fn unsharded_tensor_size(reference_shape: &[i64], shard: &Shard) -> Vec<i64> {
    let Shard {
        dim, world_size, ..
    } = *shard;

    let shard_size = reference_shape[dim];
    let total_size = shard_size * (world_size as i64);

    let mut unsharded_shape = reference_shape.to_vec();
    unsharded_shape[dim] = total_size;

    unsharded_shape
}
