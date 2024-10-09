use cudarc::{
    driver::{sys::CUdeviceptr, DevicePtr, DevicePtrMut, DeviceSlice},
    nccl::safe::{Comm, ReduceOp},
};
use half::bf16;
use std::rc::Rc;
use tch::{
    nn::{self, Module}, Device, Kind, Tensor
};

#[derive(Debug)]
pub struct TensorParallelRowLinear {
    pub(crate) linear: nn::Linear,
    pub(crate) comm: Option<Rc<Comm>>,
}

unsafe impl Send for TensorParallelRowLinear {}

impl TensorParallelRowLinear {
    pub fn new(linear: nn::Linear, comm: Option<Rc<Comm>>) -> Self {
        Self { linear, comm }
    }
}

impl Module for TensorParallelRowLinear {
    fn forward(&self, x: &Tensor) -> Tensor {
        self.linear
            .forward(x)
            .contiguous()
            .all_reduce(&self.comm, ReduceOp::Sum)
    }
}

pub trait AllReduce {
    fn all_reduce(self, comm: &Option<Rc<Comm>>, op: ReduceOp) -> Tensor;
}

struct WrappedTensor {
    tensor: Tensor,
    ptr: CUdeviceptr,
}

impl From<Tensor> for WrappedTensor {
    fn from(tensor: Tensor) -> Self {
        let kind = tensor.kind();
        assert!(kind == Kind::BFloat16 || kind == Kind::Float, "Not BF16 or F32");
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

impl DeviceSlice<bf16> for WrappedTensor {
    fn len(&self) -> usize {
        self.tensor
            .size()
            .into_iter()
            .fold(1usize, |acc, e| acc * e as usize)
    }
}

impl DevicePtr<bf16> for WrappedTensor {
    fn device_ptr(&self) -> &CUdeviceptr {
        &self.ptr
    }
}

impl DevicePtrMut<bf16> for WrappedTensor {
    fn device_ptr_mut(&mut self) -> &mut CUdeviceptr {
        &mut self.ptr
    }
}

impl DeviceSlice<f32> for WrappedTensor {
    fn len(&self) -> usize {
        self.tensor
            .size()
            .into_iter()
            .fold(1usize, |acc, e| acc * e as usize)
    }
}

impl DevicePtr<f32> for WrappedTensor {
    fn device_ptr(&self) -> &CUdeviceptr {
        &self.ptr
    }
}

impl DevicePtrMut<f32> for WrappedTensor {
    fn device_ptr_mut(&mut self) -> &mut CUdeviceptr {
        &mut self.ptr
    }
}

impl WrappedTensor {
    fn unwrap(self) -> Tensor {
        self.tensor
    }
}

impl AllReduce for Tensor {
    fn all_reduce(self, comm: &Option<Rc<Comm>>, op: ReduceOp) -> Tensor {
        match comm {
            Some(comm) => {
                let rank = match self.device() {
                    Device::Cuda(rank) => rank as i64,
                    _ => unimplemented!()
                };
                
                let reduced_output = self.zeros_like();
                let output = WrappedTensor::from(self.detach());
                let mut reduced_output = WrappedTensor::from(reduced_output);

                if self.kind() == Kind::BFloat16 {
                    comm.all_reduce::<WrappedTensor, WrappedTensor, bf16>(&output, &mut reduced_output, &op)
                        .map_err(|x| format!("nccl error: {:?}", x.0))
                        .unwrap();
                } else {
                    comm.all_reduce::<WrappedTensor, WrappedTensor, f32>(&output, &mut reduced_output, &op)
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
}
