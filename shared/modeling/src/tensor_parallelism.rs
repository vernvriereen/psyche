use std::rc::Rc;
use tch::{
    nn::{self, Module, Shard},
    Tensor,
};

#[cfg(feature = "parallelism")]
use cudarc::{
    driver::{sys::CUdeviceptr, DevicePtr, DevicePtrMut, DeviceSlice},
    nccl::safe::{Comm, Id, ReduceOp},
};
#[cfg(feature = "parallelism")]
use half::bf16;
#[cfg(feature = "parallelism")]
use tch::{Device, Kind};

#[cfg(feature = "parallelism")]
pub type Communicator = Comm;

#[cfg(feature = "parallelism")]
pub type CommunicatorId = Id;

#[cfg(not(feature = "parallelism"))]
#[derive(Debug)]
pub struct Communicator {}

#[cfg(not(feature = "parallelism"))]
#[derive(Debug, Copy, Clone)]
pub struct CommunicatorId {}

#[cfg(not(feature = "parallelism"))]
impl Communicator {
    pub fn world_size(&self) -> usize {
        unimplemented!()
    }

    pub fn rank(&self) -> usize {
        unimplemented!()
    }
}

#[cfg(not(feature = "parallelism"))]
impl CommunicatorId {
    fn new() -> Option<Self> {
        unimplemented!()
    }
}

pub enum ReduceType {
    Sum,
    Max,
}

#[cfg(feature = "parallelism")]
impl From<ReduceType> for ReduceOp {
    fn from(value: ReduceType) -> Self {
        match value {
            ReduceType::Sum => ReduceOp::Sum,
            ReduceType::Max => ReduceOp::Max,
        }
    }
}

#[derive(Debug)]
pub struct TensorParallelRowLinear {
    pub(crate) linear: nn::Linear,
    pub(crate) comm: Option<Rc<Communicator>>,
}

unsafe impl Send for TensorParallelRowLinear {}

impl TensorParallelRowLinear {
    pub fn new(linear: nn::Linear, comm: Option<Rc<Communicator>>) -> Self {
        Self { linear, comm }
    }
}

impl Module for TensorParallelRowLinear {
    #[cfg(feature = "parallelism")]
    fn forward(&self, x: &Tensor) -> Tensor {
        self.linear
            .forward(x)
            .contiguous()
            .all_reduce(&self.comm, ReduceType::Sum)
    }

    #[cfg(not(feature = "parallelism"))]
    fn forward(&self, x: &Tensor) -> Tensor {
        assert!(self.comm.is_none());
        self.linear.forward(x).contiguous()
    }
}

pub trait AllReduce {
    fn all_reduce(self, comm: &Option<Rc<Communicator>>, op: ReduceType) -> Tensor;
}

#[cfg(feature = "parallelism")]
pub trait SendTensor {
    fn send(self, comm: &Rc<Communicator>, peer: i32) -> Tensor;
}

#[cfg(feature = "parallelism")]
pub trait ReceiveTensor {
    fn receive(self, comm: &Rc<Communicator>, peer: i32) -> Tensor;
}

#[cfg(feature = "parallelism")]
pub struct CUDATensor {
    tensor: Tensor,
    ptr: CUdeviceptr,
}

#[cfg(feature = "parallelism")]
impl From<Tensor> for CUDATensor {
    fn from(tensor: Tensor) -> Self {
        let kind = tensor.kind();
        assert!(
            kind == Kind::BFloat16 || kind == Kind::Float,
            "Not BF16 or F32"
        );
        assert!(tensor.is_contiguous(), "Not contiguous");
        if let tch::Device::Cuda(_) = tensor.device() {
            Self {
                ptr: (tensor.data_ptr() as usize) as CUdeviceptr,
                tensor,
            }
        } else {
            unimplemented!()
        }
    }
}

#[cfg(feature = "parallelism")]
impl DeviceSlice<bf16> for CUDATensor {
    fn len(&self) -> usize {
        self.tensor
            .size()
            .into_iter()
            .fold(1usize, |acc, e| acc * e as usize)
    }
}

#[cfg(feature = "parallelism")]
impl DevicePtr<bf16> for CUDATensor {
    fn device_ptr(&self) -> &CUdeviceptr {
        &self.ptr
    }
}

#[cfg(feature = "parallelism")]
impl DevicePtrMut<bf16> for CUDATensor {
    fn device_ptr_mut(&mut self) -> &mut CUdeviceptr {
        &mut self.ptr
    }
}

#[cfg(feature = "parallelism")]
impl DeviceSlice<f32> for CUDATensor {
    fn len(&self) -> usize {
        self.tensor
            .size()
            .into_iter()
            .fold(1usize, |acc, e| acc * e as usize)
    }
}

#[cfg(feature = "parallelism")]
impl DevicePtr<f32> for CUDATensor {
    fn device_ptr(&self) -> &CUdeviceptr {
        &self.ptr
    }
}

#[cfg(feature = "parallelism")]
impl DevicePtrMut<f32> for CUDATensor {
    fn device_ptr_mut(&mut self) -> &mut CUdeviceptr {
        &mut self.ptr
    }
}

#[cfg(feature = "parallelism")]
impl CUDATensor {
    pub fn unwrap(self) -> Tensor {
        self.tensor
    }
}

impl AllReduce for Tensor {
    #[cfg(feature = "parallelism")]
    fn all_reduce(self, comm: &Option<Rc<Communicator>>, op: ReduceType) -> Tensor {
        match comm {
            Some(comm) => {
                let rank = match self.device() {
                    Device::Cuda(rank) => rank as i64,
                    _ => unimplemented!(),
                };

                let reduced_output = self.zeros_like();
                let output = CUDATensor::from(self.detach());
                let mut reduced_output = CUDATensor::from(reduced_output);

                if self.kind() == Kind::BFloat16 {
                    comm.all_reduce::<CUDATensor, CUDATensor, bf16>(
                        &output,
                        &mut reduced_output,
                        &op.into(),
                    )
                    .map_err(|x| format!("nccl error: {:?}", x.0))
                    .unwrap();
                } else {
                    comm.all_reduce::<CUDATensor, CUDATensor, f32>(
                        &output,
                        &mut reduced_output,
                        &op.into(),
                    )
                    .map_err(|x| format!("nccl error: {:?}", x.0))
                    .unwrap();
                }

                // without this you get all sort of weird hangs
                tch::Cuda::synchronize(rank);

                // this an STE-like trick to pass the gradients through the all-reduce without a custom backwards
                (reduced_output.unwrap() - self.detach()) + self
            }
            None => self,
        }
    }

    #[cfg(not(feature = "parallelism"))]
    fn all_reduce(self, comm: &Option<Rc<Communicator>>, _op: ReduceType) -> Tensor {
        assert!(comm.is_none());
        self
    }
}

#[cfg(feature = "parallelism")]
impl SendTensor for Tensor {
    fn send(self, comm: &Rc<Communicator>, peer: i32) -> Tensor {
        let kind = self.kind();
        let cuda_tensor = CUDATensor::from(self);
        if kind == Kind::BFloat16 {
            comm.send::<CUDATensor, bf16>(&cuda_tensor, peer)
                .map_err(|x| format!("nccl error: {:?}", x.0))
                .unwrap();
        } else {
            comm.send::<CUDATensor, f32>(&cuda_tensor, peer)
                .map_err(|x| format!("nccl error: {:?}", x.0))
                .unwrap();
        }
        cuda_tensor.unwrap()
    }
}

#[cfg(feature = "parallelism")]
impl ReceiveTensor for Tensor {
    fn receive(self, comm: &Rc<Communicator>, peer: i32) -> Tensor {
        let kind = self.kind();
        let mut cuda_tensor = CUDATensor::from(self);
        if kind == Kind::BFloat16 {
            comm.recv::<CUDATensor, bf16>(&mut cuda_tensor, peer)
                .map_err(|x| format!("nccl error: {:?}", x.0))
                .unwrap();
        } else {
            comm.recv::<CUDATensor, f32>(&mut cuda_tensor, peer)
                .map_err(|x| format!("nccl error: {:?}", x.0))
                .unwrap();
        }
        cuda_tensor.unwrap()
    }
}

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

pub fn unsharded_tensor_size(reference_shape: &[i64], shard: &Shard) -> Vec<i64> {
    let Shard {
        dim, world_size, ..
    } = *shard;

    let shard_size = reference_shape[dim as usize];
    let total_size = shard_size * (world_size as i64);

    let mut unsharded_shape = reference_shape.to_vec();
    unsharded_shape[dim as usize] = total_size;

    unsharded_shape
}
