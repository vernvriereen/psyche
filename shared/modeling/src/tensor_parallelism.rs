use std::rc::Rc;
use tch::{
    nn::{self, Module},
    Tensor,
};

#[cfg(feature = "parallelism")]
use cudarc::{
    driver::{sys::CUdeviceptr, DevicePtr, DevicePtrMut, DeviceSlice},
    nccl::safe::{Id, Comm, ReduceOp},
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
pub struct CommunicatorId{}

#[cfg(not(feature = "parallelism"))]
impl Communicator {
    pub fn world_size(&self) -> usize {
        unimplemented!()
    }

    pub fn rank(&self) -> usize {
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
struct WrappedTensor {
    tensor: Tensor,
    ptr: CUdeviceptr,
}

#[cfg(feature = "parallelism")]
impl From<Tensor> for WrappedTensor {
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
impl DeviceSlice<bf16> for WrappedTensor {
    fn len(&self) -> usize {
        self.tensor
            .size()
            .into_iter()
            .fold(1usize, |acc, e| acc * e as usize)
    }
}

#[cfg(feature = "parallelism")]
impl DevicePtr<bf16> for WrappedTensor {
    fn device_ptr(&self) -> &CUdeviceptr {
        &self.ptr
    }
}

#[cfg(feature = "parallelism")]
impl DevicePtrMut<bf16> for WrappedTensor {
    fn device_ptr_mut(&mut self) -> &mut CUdeviceptr {
        &mut self.ptr
    }
}

#[cfg(feature = "parallelism")]
impl DeviceSlice<f32> for WrappedTensor {
    fn len(&self) -> usize {
        self.tensor
            .size()
            .into_iter()
            .fold(1usize, |acc, e| acc * e as usize)
    }
}

#[cfg(feature = "parallelism")]
impl DevicePtr<f32> for WrappedTensor {
    fn device_ptr(&self) -> &CUdeviceptr {
        &self.ptr
    }
}

#[cfg(feature = "parallelism")]
impl DevicePtrMut<f32> for WrappedTensor {
    fn device_ptr_mut(&mut self) -> &mut CUdeviceptr {
        &mut self.ptr
    }
}

#[cfg(feature = "parallelism")]
impl WrappedTensor {
    fn unwrap(self) -> Tensor {
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
                let output = WrappedTensor::from(self.detach());
                let mut reduced_output = WrappedTensor::from(reduced_output);

                if self.kind() == Kind::BFloat16 {
                    comm.all_reduce::<WrappedTensor, WrappedTensor, bf16>(
                        &output,
                        &mut reduced_output,
                        &op.into(),
                    )
                    .map_err(|x| format!("nccl error: {:?}", x.0))
                    .unwrap();
                } else {
                    comm.all_reduce::<WrappedTensor, WrappedTensor, f32>(
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
